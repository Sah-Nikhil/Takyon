//! The calculator Source (ROADMAP v0.4).
//!
//! Three layers, and the split is the point: [`expr`] is arithmetic, [`units`]
//! is conversion, [`fmt`] is display, and **this file is policy** — when the
//! calculator answers at all. Policy is the part that is actually hard, because
//! every Source sees every keystroke and `EntryKind::Calc` wins its tier
//! outright, so a wrong answer does not sit politely at the bottom of the list.
//! It takes the top row and takes Enter with it.
//!
//! Currency is absent by decision, not omission: live rates are an outbound
//! request on the Bangless path, which ADR-0002 calls a correctness bug. It
//! waits for v0.9's Bangs — `docs/tbd/v0.4.md`.

mod date;
mod expr;
mod fmt;
mod tz;
mod units;

pub use tz::Clock;

use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use crate::actions;
use crate::entry::{Action, Entry, EntryId, EntryKind, Query, Source, SourceId};
use crate::rank;

pub const SOURCE_ID: SourceId = SourceId("calc");

/// Every Calc EntryId starts here, and the rest of the id is the answer itself.
///
/// The Entry is computed per keystroke and belongs to no index, so there is
/// nothing to look up when Enter arrives. Carrying the answer in the id keeps
/// activation stateless — no cache to go stale between drawing and copying.
pub const ID_PREFIX: &str = "calc:";

/// The character that forces a calculation.
///
/// Raycast has no such key, so this is ours: it is what makes [`Policy::Explicit`]
/// usable, and in [`Policy::Automatic`] it reaches the answers the conservative
/// rules below decline.
pub const FORCE: char = '=';

/// When the calculator is allowed to answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// Answer a Bangless query whenever it is unambiguously arithmetic. Raycast's
    /// behaviour, and the default. The cost is real and visible: `2022` shows a
    /// Calc Entry above "Adobe Photoshop 2022" and takes Enter from it.
    Automatic,
    /// Answer only when the input starts with [`FORCE`]. No expression can ever
    /// outrank an app, because none is produced unless it was asked for.
    Explicit,
}

impl Policy {
    /// Parse the frontend's spelling. Anything unrecognised is the default rather
    /// than an error: a bad value in storage must not disable the calculator.
    pub fn parse(s: &str) -> Policy {
        match s {
            "explicit" => Policy::Explicit,
            _ => Policy::Automatic,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Policy::Automatic => "automatic",
            Policy::Explicit => "explicit",
        }
    }
}

/// A finished calculation, ready to be a row.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    /// Formatted, grouped, and with a unit label where there is one. What the row
    /// shows and what the clipboard gets — one string, so the two cannot drift.
    pub display: String,
    /// Second line: what the user typed, less any forcing character, or for a
    /// time zone the zone in words. A result with nothing beside it is unverifiable.
    pub subtitle: String,
    /// Labels under the card's two halves. `None` draws `Expression` and
    /// `Result`; a time zone answer names both zones in words.
    pub labels: Option<[String; 2]>,
}

/// Decide whether one input is a calculation, and what its answer is.
///
/// Pure, so every rule below is testable without a Palette, a window or a Source.
pub fn answer(input: &str, policy: Policy) -> Option<Answer> {
    answer_with(input, policy, Clock::system)
}

/// [`answer`] at a fixed instant and zone, for tests.
pub fn answer_at(input: &str, policy: Policy, clock: &Clock) -> Option<Answer> {
    answer_with(input, policy, || clock.clone())
}

/// `clock` is lazy: only a date or time zone question pays for reading the system zone.
fn answer_with(input: &str, policy: Policy, clock: impl Fn() -> Clock) -> Option<Answer> {
    let raw = input.trim();
    let forced = raw.starts_with(FORCE);
    let body = if forced {
        raw[FORCE.len_utf8()..].trim()
    } else {
        raw
    };
    if body.is_empty() {
        return None;
    }
    // The Explicit Policy is the whole reason this setting exists: without the
    // forcing character, nothing here ever produces an Entry.
    if !forced && policy == Policy::Explicit {
        return None;
    }

    if let Some(conversion) = units::split(body) {
        let value = expr::eval(conversion.expression)?;
        let (converted, label) = conversion.apply(value.value)?;
        return Some(Answer {
            display: format!("{} {label}", fmt::number(converted)?),
            subtitle: body.to_string(),
            labels: None,
        });
    }

    // After units, share one clock read across date and time-zone parsing.
    let clock = clock();
    // Dates first: their grammar needs whole phrases, so a zone never parses.
    if let Some(reply) = date::answer(body, &clock) {
        return Some(Answer {
            display: reply.display,
            subtitle: reply.from,
            labels: Some(reply.labels),
        });
    }
    if let Some(reply) = tz::answer(body, &clock) {
        return Some(Answer {
            display: reply.display,
            subtitle: reply.from,
            labels: Some(reply.labels),
        });
    }

    let value = expr::eval(body)?;
    let display = fmt::number(value.value)?;

    // Raycast's rule: a bare number is worth showing only when formatting
    // *changes* it. `202` is already written as we would write it; `2024` becomes
    // `2,024`. Known cost — `2022` clears it and Photoshop 2022 loses the top
    // row, which is what `Policy::Explicit` exists to answer.
    if !forced && value.literal && display == body {
        return None;
    }

    Some(Answer {
        display,
        subtitle: body.to_string(),
        labels: None,
    })
}

/// Inline arithmetic, units, dates and time zones. Holds no index and touches
/// no disk beyond jiff's bundled tzdb.
pub struct CalcSource {
    /// [`Policy`], as a `u8` because it is written from an IPC command and read on
    /// the keystroke path. Relaxed ordering: a policy change landing one keystroke
    /// late is invisible, and a lock here would put contention on the 20 ms budget.
    policy: AtomicU8,
    /// [`Clock::system`] outside tests; a fixed instant inside them.
    clock: fn() -> Clock,
}

impl CalcSource {
    pub fn new() -> Self {
        Self::with_clock(Clock::system)
    }

    pub fn with_clock(clock: fn() -> Clock) -> Self {
        CalcSource {
            policy: AtomicU8::new(Policy::Automatic as u8),
            clock,
        }
    }

    pub fn policy(&self) -> Policy {
        match self.policy.load(Ordering::Relaxed) {
            x if x == Policy::Explicit as u8 => Policy::Explicit,
            _ => Policy::Automatic,
        }
    }

    pub fn set_policy(&self, policy: Policy) {
        self.policy.store(policy as u8, Ordering::Relaxed);
    }

    /// The clipboard text behind a Calc EntryId, or `None` if it is not one.
    pub fn answer_of(id: &EntryId) -> Option<&str> {
        id.as_str().strip_prefix(ID_PREFIX)
    }
}

impl Default for CalcSource {
    fn default() -> Self {
        Self::new()
    }
}

impl Source for CalcSource {
    fn id(&self) -> SourceId {
        SOURCE_ID
    }

    /// No budget check: this is a parse of one short string with no allocation
    /// beyond the token vector, and nothing here can block. The parameter stays
    /// because the trait's contract is per-Source, not per-implementation.
    fn query(&self, q: &Query, _budget: Duration) -> Vec<Entry> {
        // `raw`, not `needle`: the forcing character survives trimming but not
        // the lowercasing-and-trimming that `needle` is for, and `°C` should not
        // come back as `°c`.
        let Some(answer) = answer_with(&q.raw, self.policy(), self.clock) else {
            return Vec::new();
        };

        vec![Entry {
            id: EntryId(format!("{ID_PREFIX}{}", answer.display)),
            title: answer.display,
            // The expression, or the zone in words, so the answer can be checked at
            // a glance. A result with nothing beside it is a number you have to trust.
            subtitle: Some(answer.subtitle),
            kind: EntryKind::Calc,
            icon: None,
            // The top rung. `EntryKind::Calc` already wins the tier outright, so
            // this only orders Calc against itself — there is never more than one
            // — and keeps the number from looking arbitrary next to the ladder.
            score: rank::TIER_ALIAS_EXACT,
            actions: actions::for_calc(),
            version: None,
            labels: answer.labels,
        }]
    }

    fn actions(&self, entry: &Entry) -> Vec<Action> {
        actions::for_entry(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auto(input: &str) -> Option<String> {
        answer(input, Policy::Automatic).map(|a| a.display)
    }

    /// Step 1 of the manual verification script, end to end through policy.
    #[test]
    fn v0_4_the_two_worked_examples_from_the_plan() {
        assert_eq!(auto("12*1.18").as_deref(), Some("14.16"));
        assert_eq!(auto("40 kg to lb").as_deref(), Some("88.1849 lb"));
    }

    /// Step 3: the trap the phase is named for. An app search must not become a
    /// calculation, whatever it starts with.
    #[test]
    fn v0_4_an_app_search_is_never_a_calculation() {
        for name in ["1password", "7zip", "x264", "code", "steam", "notion to do"] {
            assert_eq!(auto(name), None, "{name} produced a Calc Entry");
        }
    }

    /// The observed Raycast rule, from the screenshots: `202` yields no
    /// calculator row and `2024` yields `2,024`. The difference is whether
    /// formatting changes anything.
    #[test]
    fn v0_4_a_bare_number_answers_only_when_formatting_changes_it() {
        assert_eq!(auto("202"), None);
        assert_eq!(auto("45"), None);
        assert_eq!(auto("2024").as_deref(), Some("2,024"));
        assert_eq!(auto("1000000").as_deref(), Some("1,000,000"));
    }

    /// The known cost of the rule above, asserted rather than left as folklore:
    /// `2022` does take the top row from "Adobe Photoshop 2022". The Explicit Policy is
    /// the deliberate answer, and the next test is the proof.
    #[test]
    fn v0_4_a_four_digit_year_does_answer_in_automatic_mode() {
        assert_eq!(auto("2022").as_deref(), Some("2,022"));
    }

    /// The Explicit Policy produces nothing at all without a forcing character, so no
    /// expression can outrank an app.
    #[test]
    fn v0_4_explicit_mode_answers_only_when_forced() {
        assert_eq!(answer("12*1.18", Policy::Explicit), None);
        assert_eq!(answer("2022", Policy::Explicit), None);
        assert_eq!(
            answer("=12*1.18", Policy::Explicit)
                .map(|a| a.display)
                .as_deref(),
            Some("14.16")
        );
        assert_eq!(
            answer("=40 kg to lb", Policy::Explicit)
                .map(|a| a.display)
                .as_deref(),
            Some("88.1849 lb")
        );
    }

    /// Forcing also reaches past the bare-number rule, which is what stops the
    /// conservative default from costing anything.
    #[test]
    fn v0_4_forcing_reaches_answers_the_conservative_rules_decline() {
        assert_eq!(auto("45"), None);
        assert_eq!(auto("=45").as_deref(), Some("45"));
        assert_eq!(auto("=202").as_deref(), Some("202"));
    }

    /// A lone `=`, or one with nothing but spaces after it, is someone starting
    /// to type. It is not a calculation and must not be an empty row.
    #[test]
    fn v0_4_the_forcing_character_alone_is_not_a_calculation() {
        assert_eq!(auto("="), None);
        assert_eq!(auto("=   "), None);
        assert_eq!(answer("=", Policy::Explicit), None);
    }

    /// Forcing does not make nonsense calculable. `=1password` is still an app
    /// name, and answering it would be worse than declining.
    #[test]
    fn v0_4_forcing_does_not_lower_the_bar_for_what_parses() {
        assert_eq!(auto("=1password"), None);
        assert_eq!(auto("=45+"), None);
        assert_eq!(auto("=100 usd to inr"), None);
    }

    /// A bad value in storage must leave the calculator working, not disable it.
    #[test]
    fn v0_4_an_unrecognised_mode_falls_back_to_the_default() {
        assert_eq!(Policy::parse("automatic"), Policy::Automatic);
        assert_eq!(Policy::parse("explicit"), Policy::Explicit);
        assert_eq!(Policy::parse("nonsense"), Policy::Automatic);
        assert_eq!(Policy::parse(""), Policy::Automatic);
        // The spellings round-trip, or a saved setting stops loading.
        for policy in [Policy::Automatic, Policy::Explicit] {
            assert_eq!(Policy::parse(policy.as_str()), policy);
        }
    }

    /// Activation is stateless: the answer travels in the id, so nothing has to
    /// survive between the keystroke that drew the row and the Enter that copies
    /// it.
    #[test]
    fn v0_4_the_answer_is_recoverable_from_the_entry_id() {
        let source = CalcSource::new();
        let entries = source.query(&Query::new("12*1.18"), Duration::from_millis(20));
        assert_eq!(entries.len(), 1);
        assert_eq!(CalcSource::answer_of(&entries[0].id), Some("14.16"));
        assert_eq!(entries[0].title, "14.16");
        assert_eq!(entries[0].subtitle.as_deref(), Some("12*1.18"));
        assert_eq!(entries[0].kind, EntryKind::Calc);
    }

    /// An id from any other Source must not be mistaken for an answer, or Enter
    /// on an application would copy its path instead of launching it.
    #[test]
    fn v0_4_only_a_calc_id_yields_an_answer() {
        assert_eq!(
            CalcSource::answer_of(&EntryId(r"c:\windows\notepad.exe".into())),
            None
        );
        assert_eq!(CalcSource::answer_of(&EntryId("steam:440".into())), None);
    }

    /// The Source is a parse and nothing else. If this ever needs a lock, a file
    /// or a socket, ADR-0002 has been broken somewhere upstream.
    #[test]
    fn v0_4_the_source_answers_an_empty_query_with_nothing() {
        let source = CalcSource::new();
        assert!(source
            .query(&Query::new("   "), Duration::from_millis(20))
            .is_empty());
    }

    /// Wed 7 Oct 2026, 09:00 UTC, on a machine set to Berlin and 12-hour clocks.
    fn clock() -> Clock {
        Clock {
            now: "2026-10-07T09:00:00Z".parse().unwrap(),
            local: jiff::tz::TimeZone::get("Europe/Berlin").unwrap(),
            hour24: false,
        }
    }

    /// Title, left half, and the two zone labels: everything the card draws.
    type Card = (String, String, [String; 2]);

    fn card_at(input: &str, clock: &Clock) -> Option<Card> {
        let a = answer_at(input, Policy::Automatic, clock)?;
        Some((a.display, a.subtitle, a.labels?))
    }

    fn tz(input: &str) -> Option<Card> {
        card_at(input, &clock())
    }

    fn card(title: &str, from: &str, from_label: &str, to_label: &str) -> Option<Card> {
        Some((
            title.into(),
            from.into(),
            [from_label.into(), to_label.into()],
        ))
    }

    /// The plan's worked conversion, every string the card draws.
    #[test]
    fn v0_17_a_time_converts_between_two_named_zones() {
        assert_eq!(
            tz("12 pm utc to ist"),
            card(
                "5:30 PM IST",
                "12:00 PM UTC",
                "Coordinated Universal Time \u{b7} Wed 7 Oct",
                "Kolkata \u{b7} India Standard Time, UTC+5:30 \u{b7} Wed 7 Oct",
            )
        );
    }

    /// No target means the machine's zone, named `your time` rather than a city.
    #[test]
    fn v0_17_a_time_with_no_target_converts_to_the_local_zone() {
        assert_eq!(
            tz("12 pm utc"),
            card(
                "2:00 PM CEST",
                "12:00 PM UTC",
                "Coordinated Universal Time \u{b7} Wed 7 Oct",
                "your time \u{b7} Central European Summer Time, UTC+2 \u{b7} Wed 7 Oct",
            )
        );
    }

    /// `time in X`, `now in X` and `X time` all ask for now, against the local
    /// time. London names its code in words: BST in October, GMT in January.
    #[test]
    fn v0_17_time_in_a_city_names_the_zone_in_words() {
        let summer = card(
            "10:00 AM BST",
            "11:00 AM CEST",
            "your time \u{b7} Central European Summer Time, UTC+2 \u{b7} Wed 7 Oct",
            "London \u{b7} British Summer Time, UTC+1 \u{b7} Wed 7 Oct",
        );
        assert_eq!(tz("time in london"), summer);
        assert_eq!(tz("london time"), summer);
        assert_eq!(tz("now in london"), summer);
        assert_eq!(tz("Time in London"), summer);

        let january = Clock {
            now: "2027-01-06T09:00:00Z".parse().unwrap(),
            ..clock()
        };
        assert_eq!(
            card_at("time in london", &january),
            card(
                "9:00 AM GMT",
                "10:00 AM CET",
                "your time \u{b7} Central European Time, UTC+1 \u{b7} Wed 6 Jan",
                "London \u{b7} Greenwich Mean Time, UTC+0 \u{b7} Wed 6 Jan",
            )
        );
    }

    /// Crossing midnight puts the weekday in the title; each label carries its
    /// own date. `pst` in October is Pacific time, so it reads PDT.
    #[test]
    fn v0_17_a_conversion_that_changes_the_day_says_which_day() {
        assert_eq!(
            tz("11 pm pst to ist"),
            card(
                "Thu 11:30 AM IST",
                "11:00 PM PDT",
                "Los Angeles \u{b7} Pacific Daylight Time, UTC-7 \u{b7} Wed 7 Oct",
                "Kolkata \u{b7} India Standard Time, UTC+5:30 \u{b7} Thu 8 Oct",
            )
        );
        assert_eq!(
            tz("1 am ist to new york"),
            card(
                "Tue 3:30 PM EDT",
                "1:00 AM IST",
                "Kolkata \u{b7} India Standard Time, UTC+5:30 \u{b7} Wed 7 Oct",
                "New York \u{b7} Eastern Daylight Time, UTC-4 \u{b7} Tue 6 Oct",
            )
        );
    }

    /// The three codes that collide each have one reading, and a zone sharing
    /// the code is still named correctly: Dublin's IST is Irish.
    #[test]
    fn v0_17_colliding_codes_have_one_reading_and_the_right_name() {
        let to = |input: &str| tz(input).map(|(_, _, [_, to])| to).unwrap();
        assert!(to("12 pm utc to ist").contains("India Standard Time"));
        assert!(to("12 pm utc to cst").contains("Central Daylight Time"));
        assert!(to("12 pm utc to bst").contains("British Summer Time"));
        let july = Clock {
            now: "2026-07-01T09:00:00Z".parse().unwrap(),
            ..clock()
        };
        let (title, _, [_, to]) = card_at("time in dublin", &july).unwrap();
        assert_eq!(title, "10:00 AM IST");
        assert!(to.contains("Irish Standard Time"), "{to}");
    }

    /// Spellings of a time, and the off-by-twelve trap.
    #[test]
    fn v0_17_times_of_day_parse_in_every_usual_spelling() {
        let title = |input: &str| tz(input).map(|(t, ..)| t);
        for input in [
            "12pm utc to utc",
            "12 pm utc to utc",
            "12:00pm utc to utc",
            "12 p.m. utc to utc",
            "noon utc to utc",
            "12:00 utc to utc",
        ] {
            assert_eq!(title(input).as_deref(), Some("12:00 PM UTC"), "{input}");
        }
        assert_eq!(title("12 am utc to utc").as_deref(), Some("12:00 AM UTC"));
        assert_eq!(
            title("midnight utc to utc").as_deref(),
            Some("12:00 AM UTC")
        );
        assert_eq!(title("14:30 utc to utc").as_deref(), Some("2:30 PM UTC"));
        assert_eq!(
            title("12:00 utc in kolkata").as_deref(),
            Some("5:30 PM IST")
        );
    }

    /// IANA ids and numeric offsets on either side.
    #[test]
    fn v0_17_iana_ids_and_offsets_resolve() {
        let title = |input: &str| tz(input).map(|(t, ..)| t);
        assert_eq!(
            title("9 am asia/tokyo to utc").as_deref(),
            Some("12:00 AM UTC")
        );
        assert_eq!(
            title("14:00 utc+5:30 to utc").as_deref(),
            Some("8:30 AM UTC")
        );
        assert_eq!(
            title("12 pm utc to gmt-3").as_deref(),
            Some("9:00 AM UTC-3")
        );
    }

    /// tzdb writes `+04` for Dubai. The title says `UTC+4` and no name is invented.
    #[test]
    fn v0_17_a_zone_with_no_code_shows_its_offset_alone() {
        let (title, _, [_, to]) = tz("time in dubai").unwrap();
        assert_eq!(title, "1:00 PM UTC+4");
        assert_eq!(to, "Dubai \u{b7} UTC+4 \u{b7} Wed 7 Oct");
    }

    /// 2:30 am on 2026-03-08 never happens in New York; 1:30 am on 2026-11-01
    /// happens twice. Gap shifts forward, fold takes the earlier (EDT).
    #[test]
    fn v0_17_dst_gaps_and_folds_resolve_compatibly() {
        let at = |now: &str, input: &str| {
            let c = Clock {
                now: now.parse().unwrap(),
                ..clock()
            };
            card_at(input, &c).map(|(t, ..)| t)
        };
        assert_eq!(
            at("2026-03-08T12:00:00Z", "2:30 am new york to utc").as_deref(),
            Some("7:30 AM UTC")
        );
        assert_eq!(
            at("2026-11-01T12:00:00Z", "1:30 am new york to utc").as_deref(),
            Some("5:30 AM UTC")
        );
        assert_eq!(
            at("2026-03-29T12:00:00Z", "2:30 am europe/berlin to utc").as_deref(),
            Some("1:30 AM UTC")
        );
    }

    /// A 24-hour user never sees PM.
    #[test]
    fn v0_17_a_24_hour_clock_answers_in_24_hours() {
        let c = Clock {
            hour24: true,
            ..clock()
        };
        let (title, from, _) = card_at("12 pm utc to ist", &c).unwrap();
        assert_eq!(title, "17:30 IST");
        assert_eq!(from, "12:00 UTC");
    }

    /// The Palette's top row is not for guessing: a bare time or zone is a
    /// search, and units keep their own answers, with no zone labels.
    #[test]
    fn v0_17_bare_times_and_zones_never_answer() {
        for input in [
            "12 pm",
            "9:30",
            "tokyo",
            "ist",
            "london",
            "utc",
            "12 utc",
            "12 m utc",
            "screen time",
            "time in narnia",
            "12 pmx utc",
        ] {
            assert_eq!(
                answer_at(input, Policy::Automatic, &clock()),
                None,
                "{input} answered"
            );
        }
        assert_eq!(auto("5 in to cm").as_deref(), Some("12.7 cm"));
        assert_eq!(auto("5 ft in in").as_deref(), Some("60 in"));
        assert_eq!(auto("90 min to h").as_deref(), Some("1.5 h"));
        assert_eq!(
            answer("40 kg to lb", Policy::Automatic).unwrap().labels,
            None
        );
        assert_eq!(answer_at("12 pm utc", Policy::Explicit, &clock()), None);
        assert!(answer_at("=12 pm utc", Policy::Explicit, &clock()).is_some());
    }

    /// The card a time zone answer becomes: Enter copies the title, the left
    /// half is the time asked from, and the labels name both zones in words.
    #[test]
    fn v0_17_a_time_zone_answer_is_one_calc_card() {
        let source = CalcSource::with_clock(clock);
        let entries = source.query(&Query::new("time in tokyo"), Duration::from_millis(20));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, EntryKind::Calc);
        assert_eq!(entries[0].title, "6:00 PM JST");
        assert_eq!(entries[0].subtitle.as_deref(), Some("11:00 AM CEST"));
        assert_eq!(
            entries[0].labels,
            Some([
                "your time \u{b7} Central European Summer Time, UTC+2 \u{b7} Wed 7 Oct".to_string(),
                "Tokyo \u{b7} Japan Standard Time, UTC+9 \u{b7} Wed 7 Oct".to_string(),
            ])
        );
        assert_eq!(CalcSource::answer_of(&entries[0].id), Some("6:00 PM JST"));
    }

    /// Wed 28 Oct 2026, 09:00 UTC, Berlin: today is the 28th in both zones.
    fn day() -> Clock {
        Clock {
            now: "2026-10-28T09:00:00Z".parse().unwrap(),
            ..clock()
        }
    }

    fn date(input: &str) -> Option<Card> {
        card_at(input, &day())
    }

    fn date_title(input: &str) -> Option<String> {
        date(input).map(|(t, ..)| t)
    }

    /// The asked-for spellings of a shift, every string the card draws.
    #[test]
    fn date_a_shift_from_today_answers_a_full_date() {
        let expected = card(
            "27 November 2026",
            "today + 30 days",
            "today \u{b7} Wed 28 Oct 2026",
            "Friday",
        );
        for input in [
            "30 days from now is what date",
            "30 days from now",
            "what date is 30 days from today",
            "30 days from today",
            "What date is 30 days from today?",
            "30 days after today",
            "today + 30 days",
            "today plus 30 days",
        ] {
            assert_eq!(date(input), expected, "{input}");
        }
    }

    /// Backwards, other anchors, and the other units.
    #[test]
    fn date_shifts_run_backwards_and_by_weeks_months_years() {
        assert_eq!(
            date_title("45 days ago was?").as_deref(),
            Some("13 September 2026")
        );
        let (title, _, [_, weekday]) = date("which day falls 4123 days from 21 dec 2027").unwrap();
        assert_eq!(title, "5 April 2039");
        assert_eq!(weekday, "Tuesday");
        assert_eq!(
            date_title("30 days ago").as_deref(),
            Some("28 September 2026")
        );
        assert_eq!(
            date_title("2 weeks before today").as_deref(),
            Some("14 October 2026")
        );
        assert_eq!(
            date_title("today - 1 day").as_deref(),
            Some("27 October 2026")
        );
        assert_eq!(
            date_title("today minus 1 week").as_deref(),
            Some("21 October 2026")
        );
        assert_eq!(
            date_title("a week from tomorrow").as_deref(),
            Some("5 November 2026")
        );
        assert_eq!(
            date_title("3 days from yesterday").as_deref(),
            Some("30 October 2026")
        );
        assert_eq!(
            date_title("2 months from now").as_deref(),
            Some("28 December 2026")
        );
        assert_eq!(
            date_title("1 year from today").as_deref(),
            Some("28 October 2027")
        );
        assert_eq!(
            date_title("what date is in 3 weeks").as_deref(),
            Some("18 November 2026")
        );
        let (_, from, [left, right]) = date("1 day ago").unwrap();
        assert_eq!(from, "today - 1 day");
        assert_eq!(left, "today \u{b7} Wed 28 Oct 2026");
        assert_eq!(right, "Tuesday");
    }

    /// Months clamp to the month's end, leap years included, and a year past
    /// 29 Feb lands on the 28th.
    #[test]
    fn date_month_ends_and_leap_years_clamp() {
        assert_eq!(
            date_title("1 month from 31 jan 2027").as_deref(),
            Some("28 February 2027")
        );
        assert_eq!(
            date_title("1 month from 31 jan 2028").as_deref(),
            Some("29 February 2028")
        );
        assert_eq!(
            date_title("1 year from 29 feb 2028").as_deref(),
            Some("28 February 2029")
        );
        assert_eq!(
            date_title("1 day from 28 feb 2028").as_deref(),
            Some("29 February 2028")
        );
        assert_eq!(
            date_title("1 day from 28 feb 2027").as_deref(),
            Some("1 March 2027")
        );
        let (_, from, [left, _]) = date("30 days from 1 jan 2027").unwrap();
        assert_eq!(from, "1 January 2027 + 30 days");
        assert_eq!(left, "Fri 1 Jan 2027");
    }

    /// The asked-for countdown spellings, every string the card draws.
    #[test]
    fn date_days_until_a_date_counts_from_today_exclusive() {
        let expected = card(
            "327 days",
            "20 September 2027",
            "Monday",
            "days until \u{b7} today is Wed 28 Oct 2026",
        );
        for input in [
            "how many days is 20th September 2027 in",
            "how many days until 20th September 2027",
            "days until 20 Sep 2027",
            "how many days till the 20th of september 2027?",
            "days until september 20, 2027",
            "days until sept 20th 2027",
            "days until 2027-09-20",
            "how many days is it until 20 sep 2027",
            "how many days away is 20 sep 2027",
            "how many days left until 20 sep 2027",
        ] {
            assert_eq!(date(input), expected, "{input}");
        }
        assert_eq!(date_title("days until tomorrow").as_deref(), Some("1 day"));
        assert_eq!(date_title("days until today").as_deref(), Some("0 days"));
        assert_eq!(
            date_title("days until 29 oct 2026").as_deref(),
            Some("1 day")
        );
    }

    /// A past date is negative under `until` and positive under `since`.
    #[test]
    fn date_a_past_date_counts_negative() {
        assert_eq!(
            date_title("days until 28 sep 2026").as_deref(),
            Some("-30 days")
        );
        assert_eq!(
            date_title("days until yesterday").as_deref(),
            Some("-1 day")
        );
        assert_eq!(
            date_title("days since 28 sep 2026").as_deref(),
            Some("30 days")
        );
        let (_, _, [_, right]) = date("how many days since 1 jan 2026").unwrap();
        assert_eq!(right, "days since \u{b7} today is Wed 28 Oct 2026");
    }

    /// No year means the next one on or after today, or for `since` the last
    /// one on or before it. 29 Feb waits for a leap year.
    #[test]
    fn date_a_date_with_no_year_takes_the_nearest_occurrence() {
        let (title, from, _) = date("days until 25 dec").unwrap();
        assert_eq!(
            (title.as_str(), from.as_str()),
            ("58 days", "25 December 2026")
        );
        let (_, from, _) = date("days until 1 jan").unwrap();
        assert_eq!(from, "1 January 2027");
        assert_eq!(date_title("days until 28 oct").as_deref(), Some("0 days"));
        let (_, from, _) = date("days since 1 nov").unwrap();
        assert_eq!(from, "1 November 2025");
        let (_, from, _) = date("days until 29 feb").unwrap();
        assert_eq!(from, "29 February 2028");
    }

    /// Today is the local date, not UTC's: 23:30 UTC is already the 29th in Berlin.
    #[test]
    fn date_today_is_the_local_zone_date() {
        let late = Clock {
            now: "2026-10-28T23:30:00Z".parse().unwrap(),
            ..clock()
        };
        let (title, _, [left, _]) = card_at("1 day from today", &late).unwrap();
        assert_eq!(title, "30 October 2026");
        assert_eq!(left, "today \u{b7} Thu 29 Oct 2026");
    }

    /// Wrapped questions about a lone anchor answer; the bare word does not.
    #[test]
    fn date_what_date_is_tomorrow_answers_but_tomorrow_alone_does_not() {
        assert_eq!(
            date("what date is tomorrow"),
            card(
                "29 October 2026",
                "tomorrow",
                "today \u{b7} Wed 28 Oct 2026",
                "Thursday"
            )
        );
        assert_eq!(
            date_title("what day is today").as_deref(),
            Some("28 October 2026")
        );
        assert_eq!(
            date_title("what date was it yesterday").as_deref(),
            Some("27 October 2026")
        );
    }

    /// The Palette's top row is not for guessing: titles, file names and
    /// half-typed phrases stay searches.
    #[test]
    fn date_searches_that_look_like_dates_never_answer() {
        for input in [
            "today",
            "tomorrow",
            "yesterday",
            "now",
            "7 days to die",
            "28 days later",
            "500 days of summer",
            "days gone",
            "days until dawn",
            "days until",
            "30 days",
            "in 30 days",
            "30 days from",
            "30 days from monday",
            "september 2027",
            "20 september 2027",
            "days until 31 feb 2027",
            "days until 20/09/2027",
            "days until 20 sep 27",
            "30 fortnights from now",
            "-30 days from now",
            "1000000 days from now",
            "99999 years from now",
            "how many days",
            "how many days is 20 sep 2027",
            "what date is",
            "date",
            "friday",
            "Friday Night Funkin",
            "next",
            "next week",
            "next fri",
            "next friday night",
            "friday the 13th",
            "seven days to die",
            "two weeks",
            "two weeks from",
            "2 days from friday",
            "two weeks from friday night",
            "days until friday",
            "what weekday is",
            "what day of the week is",
            "what day of the week",
            "what weekday is friday",
            "days between",
            "days between 21 dec 2027",
            "days between 21 dec 2027 and",
            "days between us",
            "how long",
            "how long until",
            "how long until dawn",
            "in 45 days",
            "what date will it be",
            "what date was",
        ] {
            assert_eq!(
                answer_at(input, Policy::Automatic, &day()),
                None,
                "{input} answered"
            );
        }
        assert_eq!(auto("2 days to hours").as_deref(), Some("48 h"));
        assert_eq!(
            answer_at("30 days from now", Policy::Explicit, &day()),
            None
        );
        assert!(answer_at("=30 days from now", Policy::Explicit, &day()).is_some());
    }

    /// Question-first and question-last shifts, forwards and back.
    #[test]
    fn date_shift_questions_in_any_order() {
        assert_eq!(
            date_title("what date was 1 nov").as_deref(),
            Some("1 November 2025")
        );
        let ahead = card(
            "12 December 2026",
            "today + 45 days",
            "today \u{b7} Wed 28 Oct 2026",
            "Saturday",
        );
        for input in [
            "what's the date 45 days from now?",
            "What\u{2019}s the date 45 days from now?",
            "in 45 days, what date will it be?",
            "what date will it be in 45 days",
        ] {
            assert_eq!(date(input), ahead, "{input}");
        }
        assert_eq!(
            date("what date was 45 days ago?"),
            card(
                "13 September 2026",
                "today - 45 days",
                "today \u{b7} Wed 28 Oct 2026",
                "Sunday"
            )
        );
    }

    /// Weekday questions answer the weekday; the full date sits under it.
    #[test]
    fn date_weekday_questions_answer_the_weekday() {
        let monday = card(
            "Monday",
            "20 September 2027",
            "Mon 20 Sep 2027",
            "20 September 2027",
        );
        for input in [
            "what day of the week is 20 Sep 2027?",
            "what weekday is 20 sep 2027",
            "which day of the week is 2027-09-20",
        ] {
            assert_eq!(date(input), monday, "{input}");
        }
        assert_eq!(
            date("what weekday was 45 days ago?"),
            card(
                "Sunday",
                "today - 45 days",
                "today \u{b7} Wed 28 Oct 2026",
                "13 September 2026"
            )
        );
        assert_eq!(
            date_title("what day of the week will it be in 45 days").as_deref(),
            Some("Saturday")
        );
        // Year-less `was` looks back, like `since`.
        let (_, from, _) = date("what weekday was 1 nov").unwrap();
        assert_eq!(from, "1 November 2025");
    }

    /// Second minus first, signed; `how long until` is `days until`.
    #[test]
    fn date_days_between_two_dates_is_second_minus_first() {
        let expected = card(
            "4123 days",
            "21 December 2027",
            "to 5 April 2039",
            "days between \u{b7} second minus first",
        );
        for input in [
            "days between 21 Dec 2027 and 5 Apr 2039",
            "how many days between 21 december 2027 and 5 april 2039?",
            "how many days are there between 21 dec 2027 and 5 apr 2039",
        ] {
            assert_eq!(date(input), expected, "{input}");
        }
        assert_eq!(
            date_title("days between 5 apr 2039 and 21 dec 2027").as_deref(),
            Some("-4123 days")
        );
        assert_eq!(
            date_title("days between today and tomorrow").as_deref(),
            Some("1 day")
        );
        assert_eq!(date_title("days between 1 jan and 31 dec"), None);
        assert_eq!(
            date("how long until 20 Sep 2027?"),
            card(
                "327 days",
                "20 September 2027",
                "Monday",
                "days until \u{b7} today is Wed 28 Oct 2026"
            )
        );
        assert_eq!(
            date_title("how long since 28 sep 2026").as_deref(),
            Some("30 days")
        );
    }

    /// `next Friday` is the first Friday strictly after today; a bare Friday
    /// after `weeks from` is that same Friday.
    #[test]
    fn date_next_weekday_is_strictly_after_today() {
        assert_eq!(
            date("next Friday"),
            card(
                "30 October 2026",
                "next Friday",
                "Fri 30 Oct 2026",
                "Friday"
            )
        );
        assert_eq!(
            date_title("what date is next friday").as_deref(),
            Some("30 October 2026")
        );
        assert_eq!(
            date_title("next wednesday").as_deref(),
            Some("4 November 2026")
        );
        assert_eq!(
            date("two weeks from Friday"),
            card(
                "13 November 2026",
                "Friday + 2 weeks",
                "Fri 30 Oct 2026",
                "Friday"
            )
        );
        assert_eq!(
            date_title("a week from next friday").as_deref(),
            Some("6 November 2026")
        );
        assert_eq!(
            date_title("days until next friday").as_deref(),
            Some("2 days")
        );
        // On a Friday, next Friday is a week away.
        let friday = Clock {
            now: "2026-10-30T09:00:00Z".parse().unwrap(),
            ..clock()
        };
        assert_eq!(
            card_at("next friday", &friday).map(|c| c.0).as_deref(),
            Some("6 November 2026")
        );
        assert_eq!(
            card_at("two weeks from friday", &friday)
                .map(|c| c.0)
                .as_deref(),
            Some("20 November 2026")
        );
    }

    /// Enter copies the title: the date, or the day count with its unit.
    #[test]
    fn date_an_answer_is_one_calc_card_and_copies_its_title() {
        fn fixed() -> Clock {
            Clock {
                now: "2026-10-28T09:00:00Z".parse().unwrap(),
                local: jiff::tz::TimeZone::get("Europe/Berlin").unwrap(),
                hour24: false,
            }
        }
        let source = CalcSource::with_clock(fixed);
        for (input, copied) in [
            ("30 days from now", "27 November 2026"),
            ("days until 20 Sep 2027", "327 days"),
            ("what day of the week is 20 Sep 2027", "Monday"),
            ("days between 21 Dec 2027 and 5 Apr 2039", "4123 days"),
            ("next friday", "30 October 2026"),
        ] {
            let entries = source.query(&Query::new(input), Duration::from_millis(20));
            assert_eq!(entries.len(), 1, "{input}");
            assert_eq!(entries[0].kind, EntryKind::Calc);
            assert_eq!(entries[0].title, copied);
            assert_eq!(CalcSource::answer_of(&entries[0].id), Some(copied));
        }
    }

    /// Switching policys takes effect on the next keystroke, with no restart.
    #[test]
    fn v0_4_the_mode_is_switchable_at_runtime() {
        let source = CalcSource::new();
        assert_eq!(source.policy(), Policy::Automatic);
        assert_eq!(source.query(&Query::new("2+2"), Duration::ZERO).len(), 1);

        source.set_policy(Policy::Explicit);
        assert_eq!(source.policy(), Policy::Explicit);
        assert!(source.query(&Query::new("2+2"), Duration::ZERO).is_empty());
        assert_eq!(source.query(&Query::new("=2+2"), Duration::ZERO).len(), 1);
    }
}
