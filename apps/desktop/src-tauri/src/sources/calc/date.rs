//! Date arithmetic: shifts (`30 days from now`) and countdowns (`days until
//! 20 Sep 2027`). Civil dates through `jiff`, today from the local zone the
//! [`Clock`] carries. Offline; parses whole phrases word by word.
//!
//! A count of days is target minus today, so tomorrow is 1 and a past date is
//! negative. Month and year shifts clamp to month end (31 Jan + 1 month is 28
//! or 29 Feb). A date with no year is the nearest occurrence: next for a shift
//! or `until`, last for `since`. Numeric dates other than ISO are refused, as
//! `20/09` and `09/20` read differently by locale.
//!
//! Never answers a bare date or a bare `today`: those are searches.

use jiff::civil::{Date, Weekday};
use jiff::Span;

use super::tz::{Clock, Reply, MONTHS};

/// Answer a date question, or decline.
pub fn answer(body: &str, clock: &Clock) -> Option<Reply> {
    let lower = body.trim().to_lowercase().replace(',', " ");
    let lower = lower.trim_end_matches(['?', '.', '!']);
    let words: Vec<&str> = lower.split_whitespace().collect();
    let today = clock.now.to_zoned(clock.local.clone()).date();
    countdown(&words, today).or_else(|| shift(&words, today))
}

/// Strip `prefix` from the front of `words`.
fn strip<'a, 'b>(words: &'a [&'b str], prefix: &[&str]) -> Option<&'a [&'b str]> {
    (words.len() >= prefix.len() && words[..prefix.len()] == *prefix)
        .then(|| &words[prefix.len()..])
}

fn strip_end<'a, 'b>(words: &'a [&'b str], suffix: &[&str]) -> Option<&'a [&'b str]> {
    let cut = words.len().checked_sub(suffix.len())?;
    (words[cut..] == *suffix).then(|| &words[..cut])
}

/// First prefix that matches, and what follows it.
fn strip_any<'a, 'b>(words: &'a [&'b str], prefixes: &[&[&str]]) -> Option<&'a [&'b str]> {
    prefixes.iter().find_map(|p| strip(words, p))
}

const UNTIL: &[&[&str]] = &[
    &["how", "many", "days", "until"],
    &["how", "many", "days", "till"],
    &["how", "many", "days", "til"],
    &["how", "many", "days", "to"],
    &["how", "many", "days", "left", "until"],
    &["how", "many", "days", "left", "till"],
    &["how", "many", "days", "is", "it", "until"],
    &["how", "many", "days", "is", "it", "till"],
    &["how", "many", "days", "are", "there", "until"],
    &["how", "many", "days", "away", "is"],
    &["days", "until"],
    &["days", "till"],
    &["days", "til"],
    &["days", "left", "until"],
];

const SINCE: &[&[&str]] = &[
    &["how", "many", "days", "since"],
    &["how", "many", "days", "has", "it", "been", "since"],
    &["days", "since"],
];

/// `days until <date>`, `how many days is <date> in`, `days since <date>`.
fn countdown(words: &[&str], today: Date) -> Option<Reply> {
    let (target, since) = if let Some(rest) = strip_any(words, SINCE) {
        (day(rest, today, false)?, true)
    } else if let Some(rest) = strip_any(words, UNTIL) {
        (day(rest, today, true)?, false)
    } else {
        // `how many days is <date> in|away|from now`: the date sits mid-phrase.
        let rest = strip(words, &["how", "many", "days", "is"])?;
        let rest = [&["in"][..], &["away"], &["from", "now"], &["from", "today"]]
            .iter()
            .find_map(|s| strip_end(rest, s))?;
        (day(rest, today, true)?, false)
    };
    let days = today.until(target.date).ok()?.get_days();
    let days = if since { -days } else { days };
    let unit = if days.abs() == 1 { "day" } else { "days" };
    let word = if since { "since" } else { "until" };
    Some(Reply {
        display: format!("{days} {unit}"),
        from: full(target.date),
        labels: [
            weekday_name(target.date.weekday()).to_string(),
            format!("days {word} \u{b7} today is {}", short(today)),
        ],
    })
}

/// Wrappers that turn a phrase into a question. Only a wrapped phrase may be
/// a lone anchor (`what date is tomorrow`) or `in 3 weeks`.
const ASK: &[&[&str]] = &[
    &["what", "date", "is"],
    &["what", "day", "is"],
    &["what", "date", "is", "it"],
    &["what", "date", "will", "it", "be"],
    &["what", "day", "will", "it", "be"],
    &["what", "date", "was", "it"],
    &["what", "day", "was", "it"],
    &["what", "is", "the", "date"],
    &["what's", "the", "date"],
    &["what", "was", "the", "date"],
];

const ASKED: &[&[&str]] = &[
    &["is", "what", "date"],
    &["is", "what", "day"],
    &["will", "be", "what", "date"],
    &["was", "what", "date"],
];

/// `30 days from now`, `2 weeks ago`, `today + 1 month`, `what date is tomorrow`.
fn shift(words: &[&str], today: Date) -> Option<Reply> {
    let wrapped = ASK
        .iter()
        .rev()
        .find_map(|p| strip(words, p))
        .or_else(|| ASKED.iter().find_map(|s| strip_end(words, s)));
    let core = wrapped.unwrap_or(words);
    if core.is_empty() {
        return None;
    }

    let (anchor, by) = match parse_shift(core, today) {
        Some(found) => found,
        None if wrapped.is_some() => match core {
            ["in", n, unit] => (anchor_today(today), Some(step(n, unit, 1)?)),
            _ => (day(core, today, true)?, None),
        },
        None => return None,
    };

    let (date, from) = match by {
        Some(step) => {
            let sign = if step.sign > 0 { '+' } else { '-' };
            let to = anchor.date.checked_add(step.span()?).ok()?;
            (to, format!("{} {sign} {}", anchor.said, step.said()))
        }
        None => (anchor.date, anchor.said.clone()),
    };
    Some(Reply {
        display: full(date),
        from,
        labels: [anchor.label, weekday_name(date.weekday()).to_string()],
    })
}

/// A shift with its anchor, in any of the unwrapped spellings.
fn parse_shift(core: &[&str], today: Date) -> Option<(Anchor, Option<Step>)> {
    if let [n, unit, "ago"] = core {
        return Some((anchor_today(today), Some(step(n, unit, -1)?)));
    }
    if let [n, unit, dir @ ("from" | "after" | "before"), rest @ ..] = core {
        let sign = if *dir == "before" { -1 } else { 1 };
        return Some((day(rest, today, true)?, Some(step(n, unit, sign)?)));
    }
    // `<anchor> + <n> <unit>`: the operator is third from the end.
    let [rest @ .., op, n, unit] = core else {
        return None;
    };
    let sign = match *op {
        "+" | "plus" => 1,
        "-" | "minus" => -1,
        _ => return None,
    };
    Some((day(rest, today, true)?, Some(step(n, unit, sign)?)))
}

#[derive(Clone, Copy)]
enum Unit {
    Day,
    Week,
    Month,
    Year,
}

struct Step {
    count: i64,
    unit: Unit,
    sign: i64,
}

impl Step {
    fn span(&self) -> Option<Span> {
        let n = self.count * self.sign;
        let span = Span::new();
        match self.unit {
            Unit::Day => span.try_days(n),
            Unit::Week => span.try_weeks(n),
            Unit::Month => span.try_months(n),
            Unit::Year => span.try_years(n),
        }
        .ok()
    }

    /// `30 days`, `1 week`.
    fn said(&self) -> String {
        let word = match self.unit {
            Unit::Day => "day",
            Unit::Week => "week",
            Unit::Month => "month",
            Unit::Year => "year",
        };
        let plural = if self.count == 1 { "" } else { "s" };
        format!("{} {word}{plural}", self.count)
    }
}

/// `30 days`, `a week`. Digits only, at most six: no signs, no decimals.
fn step(n: &str, unit: &str, sign: i64) -> Option<Step> {
    let count = match n {
        "a" | "an" | "one" => 1,
        _ if (1..=6).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_digit()) => {
            n.parse().ok()?
        }
        _ => return None,
    };
    let unit = match unit {
        "day" | "days" => Unit::Day,
        "week" | "weeks" => Unit::Week,
        "month" | "months" => Unit::Month,
        "year" | "years" => Unit::Year,
        _ => return None,
    };
    Some(Step { count, unit, sign })
}

/// A resolved date, as the left half of the card says it.
struct Anchor {
    date: Date,
    /// `today`, or the date in full.
    said: String,
    label: String,
}

fn anchor_today(today: Date) -> Anchor {
    relative("today", today, today)
}

fn relative(said: &str, date: Date, today: Date) -> Anchor {
    Anchor {
        date,
        said: said.to_string(),
        label: format!("today \u{b7} {}", short(today)),
    }
}

/// `today`, `tomorrow`, `yesterday`, `now`, or a calendar date. `forward`
/// picks the next occurrence of a year-less date, else the last.
fn day(words: &[&str], today: Date, forward: bool) -> Option<Anchor> {
    let offset = match words {
        ["now"] | ["today"] => 0,
        ["tomorrow"] => 1,
        ["yesterday"] => -1,
        _ => {
            let date = calendar(words, today, forward)?;
            return Some(Anchor {
                date,
                said: full(date),
                label: short(date),
            });
        }
    };
    let date = today.checked_add(Span::new().days(offset)).ok()?;
    // `now` reads `today` on the card.
    let said = if offset == 0 { "today" } else { words[0] };
    Some(relative(said, date, today))
}

/// `20th september 2027`, `the 20th of sep`, `sept 20 2027`, `2027-09-20`.
fn calendar(words: &[&str], today: Date, forward: bool) -> Option<Date> {
    let words = strip(words, &["the"]).unwrap_or(words);
    let (d, m, y) = match words {
        [iso] => return iso_date(iso),
        [d, "of", m, rest @ ..] | [d, m, rest @ ..] if day_of_month(d).is_some() => {
            (day_of_month(d)?, month(m)?, rest)
        }
        [m, d, rest @ ..] => (day_of_month(d)?, month(m)?, rest),
        _ => return None,
    };
    match y {
        [] => nearest(d, m, today, forward),
        [y] if y.len() == 4 && y.bytes().all(|b| b.is_ascii_digit()) => {
            Date::new(y.parse().ok()?, m, d).ok()
        }
        _ => None,
    }
}

/// The next (or last) valid `d m` around today, within one leap cycle.
fn nearest(d: i8, m: i8, today: Date, forward: bool) -> Option<Date> {
    let years: Vec<i16> = if forward {
        (0..=8).map(|k| today.year() + k).collect()
    } else {
        (0..=8).map(|k| today.year() - k).collect()
    };
    years
        .into_iter()
        .filter_map(|y| Date::new(y, m, d).ok())
        .find(|date| {
            if forward {
                *date >= today
            } else {
                *date <= today
            }
        })
}

fn iso_date(s: &str) -> Option<Date> {
    let parts: Vec<&str> = s.split('-').collect();
    let [y, m, d] = parts.as_slice() else {
        return None;
    };
    let digits = |p: &str, n: usize| p.len() == n && p.bytes().all(|b| b.is_ascii_digit());
    if !(digits(y, 4) && digits(m, 2) && digits(d, 2)) {
        return None;
    }
    Date::new(y.parse().ok()?, m.parse().ok()?, d.parse().ok()?).ok()
}

/// `20`, `20th`, `1st`. Refuse mismatched suffixes such as `1th`.
fn day_of_month(s: &str) -> Option<i8> {
    let digits = s.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let suffix = &s[digits.len()..];
    if !(1..=2).contains(&digits.len()) || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let d: i8 = digits.parse().ok()?;
    let expected = if (11..=13).contains(&d) {
        "th"
    } else {
        match d % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    };
    if !suffix.is_empty() && suffix != expected {
        return None;
    }
    (1..=31).contains(&d).then_some(d)
}

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Full name, three-letter short, or `sept`.
fn month(s: &str) -> Option<i8> {
    if s == "sept" {
        return Some(9);
    }
    let at = MONTH_NAMES.iter().position(|full| {
        full.eq_ignore_ascii_case(s) || (s.len() == 3 && full[..3].eq_ignore_ascii_case(s))
    })?;
    Some(at as i8 + 1)
}

/// `27 November 2026`: what the card shows and Enter copies.
fn full(date: Date) -> String {
    format!(
        "{} {} {}",
        date.day(),
        MONTH_NAMES[date.month() as usize - 1],
        date.year()
    )
}

/// `Wed 28 Oct 2026`, for labels.
fn short(date: Date) -> String {
    format!(
        "{} {} {} {}",
        super::tz::weekday(date.weekday()),
        date.day(),
        MONTHS[date.month() as usize - 1],
        date.year()
    )
}

fn weekday_name(w: Weekday) -> &'static str {
    match w {
        Weekday::Monday => "Monday",
        Weekday::Tuesday => "Tuesday",
        Weekday::Wednesday => "Wednesday",
        Weekday::Thursday => "Thursday",
        Weekday::Friday => "Friday",
        Weekday::Saturday => "Saturday",
        Weekday::Sunday => "Sunday",
    }
}
