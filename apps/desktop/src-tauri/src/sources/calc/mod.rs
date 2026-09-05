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

mod expr;
mod fmt;
mod units;

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
    /// What the user typed, less any forcing character. Shown as the second line,
    /// because a result with no expression beside it is unverifiable.
    pub expression: String,
}

/// Decide whether one input is a calculation, and what its answer is.
///
/// Pure, so every rule below is testable without a Palette, a window or a Source.
pub fn answer(input: &str, policy: Policy) -> Option<Answer> {
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
            expression: body.to_string(),
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
        expression: body.to_string(),
    })
}

/// Inline arithmetic and unit conversion. Holds no index and touches no disk.
pub struct CalcSource {
    /// [`Policy`], as a `u8` because it is written from an IPC command and read on
    /// the keystroke path. Relaxed ordering: a policy change landing one keystroke
    /// late is invisible, and a lock here would put contention on the 20 ms budget.
    policy: AtomicU8,
}

impl CalcSource {
    pub fn new() -> Self {
        CalcSource {
            policy: AtomicU8::new(Policy::Automatic as u8),
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
        let Some(answer) = answer(&q.raw, self.policy()) else {
            return Vec::new();
        };

        vec![Entry {
            id: EntryId(format!("{ID_PREFIX}{}", answer.display)),
            title: answer.display,
            // The expression, so the answer can be checked at a glance. A result
            // with nothing beside it is a number you have to trust.
            subtitle: Some(answer.expression),
            kind: EntryKind::Calc,
            icon: None,
            // The top rung. `EntryKind::Calc` already wins the tier outright, so
            // this only orders Calc against itself — there is never more than one
            // — and keeps the number from looking arbitrary next to the ladder.
            score: rank::TIER_ALIAS_EXACT,
            actions: actions::for_calc(),
            version: None,
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
            answer("=12*1.18", Policy::Explicit).map(|a| a.display).as_deref(),
            Some("14.16")
        );
        assert_eq!(
            answer("=40 kg to lb", Policy::Explicit).map(|a| a.display).as_deref(),
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
