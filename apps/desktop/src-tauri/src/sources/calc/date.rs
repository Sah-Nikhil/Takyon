//! Date arithmetic: shifts (`3 months and 10 days from now`), countdowns in
//! days, weeks or business days, spans, weekdays, `last Friday`, period ends,
//! leap years, nth weekdays, and ages from a typed birth date. Civil dates via
//! `jiff`, today in the [`Clock`]'s zone; offline, nothing stored.
//!
//! Count = target minus today: tomorrow 1, past negative. Business days skip
//! Sat/Sun only. Shift units apply in spoken order, each clamping to month end.
//! Year-less date = next for a shift or `until`, last for `since`; a span's
//! second end follows its first. Non-ISO numeric dates refused (locale). Never
//! answers a bare date, weekday or `today`: those are searches.

use jiff::civil::{Date, Weekday};
use jiff::Span;

use super::tz::{Clock, Reply, MONTHS};

/// Answer a date question, or decline.
pub fn answer(body: &str, clock: &Clock) -> Option<Reply> {
    let lower = body
        .trim()
        .to_lowercase()
        .replace(',', " ")
        .replace('\u{2019}', "'");
    let lower = lower.trim_end_matches(['?', '.', '!']);
    let words: Vec<&str> = lower.split_whitespace().collect();
    let today = clock.now.to_zoned(clock.local.clone()).date();
    left(&words, today)
        .or_else(|| countdown(&words, today))
        .or_else(|| between(&words, today))
        .or_else(|| weekday(&words, today))
        .or_else(|| leap(&words))
        .or_else(|| nth(&words, today))
        .or_else(|| age(&words, today))
        .or_else(|| shift(&words, today))
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

/// What a count is in: days, weeks plus days, or Mon-Fri days.
#[derive(Clone, Copy)]
enum Measure {
    Days,
    Weeks,
    /// Singular word the user typed: `weekday`, `business day`, `working day`.
    Weekdays(&'static str),
}

impl Measure {
    /// `days`, `weeks`, `business days`: the label's opening word.
    fn word(self) -> String {
        match self {
            Measure::Days => "days".into(),
            Measure::Weeks => "weeks".into(),
            Measure::Weekdays(w) => format!("{w}s"),
        }
    }

    /// `from` to `to`, signed, in this measure.
    fn count(self, from: Date, to: Date) -> Option<String> {
        let days = from.until(to).ok()?.get_days();
        Some(match self {
            Measure::Days => count(days),
            Measure::Weeks => weeks(days),
            Measure::Weekdays(w) => plural(weekdays_between(from, to)?, w),
        })
    }

    /// Label detail beside the word: the day total for weeks, the weekday rule.
    fn note(self, from: Date, to: Date) -> Option<String> {
        match self {
            Measure::Days => None,
            Measure::Weeks => Some(count(from.until(to).ok()?.get_days())),
            Measure::Weekdays(_) => Some("Mon-Fri, holidays count".into()),
        }
    }

    /// `<word> <what> · <note> · <tail>`.
    fn label(self, what: &str, from: Date, to: Date, tail: &str) -> String {
        let mut parts = vec![format!("{} {what}", self.word())];
        parts.extend(self.note(from, to));
        parts.push(tail.to_string());
        parts.join(" \u{b7} ")
    }
}

/// `how many <measure>` or a bare `<measure>`, and whether `how many` led.
fn measure<'a, 'b>(words: &'a [&'b str]) -> Option<(Measure, bool, &'a [&'b str])> {
    let (how, words) = match strip(words, &["how", "many"]) {
        Some(rest) => (true, rest),
        None => (false, words),
    };
    let (m, rest) = match words {
        ["days", rest @ ..] => (Measure::Days, rest),
        ["weeks", rest @ ..] => (Measure::Weeks, rest),
        ["weekdays", rest @ ..] => (Measure::Weekdays("weekday"), rest),
        ["business", "days", rest @ ..] => (Measure::Weekdays("business day"), rest),
        ["working", "days", rest @ ..] => (Measure::Weekdays("working day"), rest),
        _ => return None,
    };
    Some((m, how, rest))
}

/// After `<measure>`; `how many` also takes [`HOW_UNTIL`].
const UNTIL: &[&[&str]] = &[&["until"], &["till"], &["til"], &["to"], &["left", "until"]];

const HOW_UNTIL: &[&[&str]] = &[
    &["left", "till"],
    &["is", "it", "until"],
    &["is", "it", "till"],
    &["are", "there", "until"],
    &["away", "is"],
];

const HOW_SINCE: &[&[&str]] = &[&["since"], &["has", "it", "been", "since"]];

/// `how long`: days only.
const LONG_UNTIL: &[&[&str]] = &[
    &["how", "long", "until"],
    &["how", "long", "till"],
    &["how", "long", "is", "it", "until"],
    &["how", "long", "is", "it", "till"],
];

const LONG_SINCE: &[&[&str]] = &[
    &["how", "long", "since"],
    &["how", "long", "has", "it", "been", "since"],
];

/// `days until <date>`, `how many weeks is <date> in`, `days since <date>`.
fn countdown(words: &[&str], today: Date) -> Option<Reply> {
    let (m, rest, since) = if let Some(rest) = strip_any(words, LONG_SINCE) {
        (Measure::Days, rest, true)
    } else if let Some(rest) = strip_any(words, LONG_UNTIL) {
        (Measure::Days, rest, false)
    } else {
        let (m, how, rest) = measure(words)?;
        if let Some(rest) =
            strip(rest, &["since"]).or_else(|| how.then(|| strip_any(rest, HOW_SINCE)).flatten())
        {
            (m, rest, true)
        } else if let Some(rest) =
            strip_any(rest, UNTIL).or_else(|| how.then(|| strip_any(rest, HOW_UNTIL)).flatten())
        {
            (m, rest, false)
        } else {
            // `how many days is <date> in|away|from now`: the date sits mid-phrase.
            let rest = strip(rest, &["is"]).filter(|_| how)?;
            let rest = [&["in"][..], &["away"], &["from", "now"], &["from", "today"]]
                .iter()
                .find_map(|s| strip_end(rest, s))?;
            (m, rest, false)
        }
    };
    let (target, what) = match birthday(rest, today) {
        Some(date) if !since => (date, "until your birthday"),
        _ => (
            day(rest, today, !since)?.date,
            if since { "since" } else { "until" },
        ),
    };
    let (from, to) = if since {
        (target, today)
    } else {
        (today, target)
    };
    Some(Reply {
        display: m.count(from, to)?,
        from: full(target),
        labels: [
            weekday_name(target.weekday()).to_string(),
            m.label(what, from, to, &format!("today is {}", short(today))),
        ],
    })
}

/// `days left in this month`, `weekdays left this year`: after today to the last day.
fn left(words: &[&str], today: Date) -> Option<Reply> {
    let (m, _, rest) = measure(words)?;
    let rest = strip_any(rest, &[&["are", "there"], &["are"]]).unwrap_or(rest);
    let rest = strip_any(rest, &[&["left"], &["remaining"]])?;
    let unit = match rest {
        ["in", "this" | "the", unit] | ["this" | "the", unit] | ["in", unit] => *unit,
        _ => return None,
    };
    let end = period_end(&[unit], today)?;
    let name = match unit {
        "month" => format!(
            "{} {}",
            MONTH_NAMES[today.month() as usize - 1],
            today.year()
        ),
        _ => today.year().to_string(),
    };
    let tail = format!("after today \u{b7} today is {}", short(today));
    Some(Reply {
        display: m.count(today, end)?,
        from: full(end),
        labels: [
            weekday_name(end.weekday()).to_string(),
            m.label(&format!("left in {name}"), today, end, &tail),
        ],
    })
}

/// `days between <date> and <date>`: second minus first, signed. A yearless
/// second end is the first occurrence on or after the first, so order holds.
fn between(words: &[&str], today: Date) -> Option<Reply> {
    let (m, how, rest) = measure(words)?;
    let rest = strip(rest, &["between"]).or_else(|| {
        how.then(|| strip(rest, &["are", "there", "between"]))
            .flatten()
    })?;
    let at = rest.iter().position(|w| *w == "and")?;
    let first = day(&rest[..at], today, true)?;
    let second = day_near(&rest[at + 1..], today, first.date, true)?;
    Some(Reply {
        display: m.count(first.date, second.date)?,
        from: first.said,
        labels: [
            format!("to {}", second.said),
            m.label("between", first.date, second.date, "second minus first"),
        ],
    })
}

/// `3 days`, `-1 day`.
fn count(days: i32) -> String {
    plural(i64::from(days), "day")
}

fn plural(n: i64, unit: &str) -> String {
    let s = if n.abs() == 1 { "" } else { "s" };
    format!("{n} {unit}{s}")
}

/// `45 weeks, 2 days`; negative signs both halves: `-1 week, -1 day`.
fn weeks(days: i32) -> String {
    let (w, d) = (i64::from(days / 7), i64::from(days % 7));
    if d == 0 {
        plural(w, "week")
    } else {
        format!("{}, {}", plural(w, "week"), plural(d, "day"))
    }
}

/// Mon-Fri days in `(from, to]`, or minus those in `[to, from)` when `to` is earlier.
fn weekdays_between(from: Date, to: Date) -> Option<i64> {
    if to < from {
        let back = |d: Date| d.yesterday().ok();
        return Some(-weekdays_between(back(to)?, back(from)?)?);
    }
    let days = i64::from(from.until(to).ok()?.get_days());
    let start = i64::from(from.weekday().to_monday_zero_offset());
    // Every 7 days hold 5 weekdays; walk the leftover by offset alone.
    let tail = (1..=days % 7).filter(|k| (start + k) % 7 < 5).count() as i64;
    Some(days / 7 * 5 + tail)
}

/// `is 2028 a leap year`: yes or no, with the year's length.
fn leap(words: &[&str]) -> Option<Reply> {
    let (y, rest) = match words {
        ["is" | "was", y, rest @ ..] | ["will", y, "be", rest @ ..] => (*y, rest),
        _ => return None,
    };
    if !matches!(rest, ["a", "leap", "year"] | ["leap", "year"]) {
        return None;
    }
    let year = four_digits(y)?;
    let is_leap = |y: i16| Date::new(y, 1, 1).is_ok_and(|d| d.in_leap_year());
    let (display, labels) = if is_leap(year) {
        let feb29 = Date::new(year, 2, 29).ok()?;
        let on = weekday_name(feb29.weekday());
        let labels = ["366 days".into(), format!("29 February {year} is a {on}")];
        (format!("{year} is a leap year"), labels)
    } else {
        let next = (1..=8).map(|k| year + k).find(|y| is_leap(*y))?;
        let labels = ["365 days".into(), format!("next leap year {next}")];
        (format!("{year} is not a leap year"), labels)
    };
    Some(Reply {
        display,
        from: year.to_string(),
        labels,
    })
}

const NTH_ASK: &[&[&str]] = &[
    &["what's", "the"],
    &["what", "is", "the"],
    &["when", "is", "the"],
    &["when's", "the"],
    &["what", "date", "is", "the"],
    &["which", "day", "is", "the"],
];

/// `what's the first Monday in November 2027`: nth or last weekday of a month.
/// Year-less = first such date on or after today. Only asked questions answer.
fn nth(words: &[&str], today: Date) -> Option<Reply> {
    let [ord, name, "in" | "of", m, year @ ..] = strip_any(words, NTH_ASK)? else {
        return None;
    };
    let (n, ord) = match *ord {
        "first" | "1st" => (1, "first"),
        "second" | "2nd" => (2, "second"),
        "third" | "3rd" => (3, "third"),
        "fourth" | "4th" => (4, "fourth"),
        "fifth" | "5th" => (5, "fifth"),
        "last" => (-1, "last"),
        _ => return None,
    };
    let w = WEEKDAYS
        .into_iter()
        .find(|w| weekday_name(*w).eq_ignore_ascii_case(name))?;
    let m = month(m)?;
    let find = |y: i16| Date::new(y, m, 1).ok()?.nth_weekday_of_month(n, w).ok();
    let date = match year {
        [] => (0i16..=400)
            .filter_map(|k| today.year().checked_add(k))
            .filter_map(find)
            .find(|d| *d >= today)?,
        _ => find(year_of(year, today)?)?,
    };
    let month_name = MONTH_NAMES[m as usize - 1];
    Some(Reply {
        display: full(date),
        from: format!("{ord} {} in {month_name} {}", weekday_name(w), date.year()),
        labels: [short(date), weekday_name(w).to_string()],
    })
}

/// `how old will I be on <date> if born <date>`: whole years. The birth date
/// must carry its year; nothing is stored or guessed.
fn age(words: &[&str], today: Date) -> Option<Reply> {
    let at = words.iter().position(|w| *w == "born")?;
    let born = &words[at + 1..];
    let born = strip(born, &["on"]).unwrap_or(born);
    if !(born.len() == 1 || born.last().is_some_and(|y| four_digits(y).is_some())) {
        return None;
    }
    let born = calendar(born, today, today, true)?;
    let ask = strip_end(&words[..at], &["if", "i", "was"])
        .or_else(|| strip_end(&words[..at], &["if"]))?;
    let on = match ask {
        ["how", "old", "am", "i"] => today,
        ["how", "old", "will", "i", "be", "on", rest @ ..] => day(rest, today, true)?.date,
        ["how", "old", "was", "i", "on", rest @ ..] => day(rest, today, false)?.date,
        _ => return None,
    };
    if on < born {
        return None;
    }
    let before = (on.month(), on.day()) < (born.month(), born.day());
    let years = i64::from(on.year() - born.year()) - i64::from(before);
    Some(Reply {
        display: plural(years, "year"),
        from: format!("born {}", full(born)),
        labels: [short(born), format!("age on {}", full(on))],
    })
}

/// `my birthday on 10 Sep`, `my birthday if born 10 Sep 2000`: next anniversary
/// on or after today. A typed year is ignored; no birthday is remembered.
fn birthday(words: &[&str], today: Date) -> Option<Date> {
    let rest = strip(words, &["my", "birthday"])?;
    let rest = strip_any(
        rest,
        &[
            &["is", "on"],
            &["on"],
            &["is"],
            &["if", "i", "was", "born", "on"],
            &["if", "i", "was", "born"],
            &["if", "born", "on"],
            &["if", "born"],
        ],
    )?;
    let date = calendar(rest, today, today, true)?;
    nearest(date.day(), date.month(), today, true)
}

/// Weekday questions, and whether a year-less date looks forward (`was` looks back).
const WEEKDAY_ASK: &[(&[&str], bool)] = &[
    (&["what", "day", "of", "the", "week", "is"], true),
    (&["what", "day", "of", "the", "week", "was"], false),
    (
        &["what", "day", "of", "the", "week", "will", "it", "be"],
        true,
    ),
    (&["which", "day", "of", "the", "week", "is"], true),
    (&["which", "day", "of", "the", "week", "was"], false),
    (&["what", "weekday", "is"], true),
    (&["what", "weekday", "was"], false),
    (&["what", "weekday", "will", "it", "be"], true),
];

/// `what weekday is 20 Sep 2027`: the weekday, with the full date under it.
fn weekday(words: &[&str], today: Date) -> Option<Reply> {
    let (core, forward) = WEEKDAY_ASK
        .iter()
        .find_map(|(ask, forward)| Some((strip(words, ask)?, *forward)))?;
    let to = target(core, true, today, forward)?;
    Some(Reply {
        display: weekday_name(to.date.weekday()).to_string(),
        from: to.from,
        labels: [to.label, full(to.date)],
    })
}

/// Wrappers that turn a phrase into a question. Only a wrapped phrase may be
/// a lone anchor (`what date is tomorrow`) or `in 3 weeks`.
const ASK: &[&[&str]] = &[
    &["which", "day", "falls"],
    &["what", "date", "is"],
    &["what", "day", "is"],
    &["what", "date", "is", "it"],
    &["what", "date", "will", "it", "be"],
    &["what", "day", "will", "it", "be"],
    &["what", "date", "was"],
    &["what", "day", "was"],
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
    &["what", "date", "will", "it", "be"],
    &["what", "day", "will", "it", "be"],
];

/// `30 days from now`, `2 weeks ago`, `today + 1 month`, `what date is tomorrow`.
fn shift(words: &[&str], today: Date) -> Option<Reply> {
    let wrapped = ASK
        .iter()
        .rev()
        .find_map(|p| Some((strip(words, p)?, !p.contains(&"was"))))
        .or_else(|| {
            ASKED
                .iter()
                .find_map(|s| Some((strip_end(words, s)?, !s.contains(&"was"))))
        });
    let core = wrapped.map(|(core, _)| core).unwrap_or(words);
    let forward = wrapped.map(|(_, forward)| forward).unwrap_or(true);
    let to = target(core, wrapped.is_some(), today, forward)?;
    Some(Reply {
        display: full(to.date),
        from: to.from,
        labels: [to.label, weekday_name(to.date.weekday()).to_string()],
    })
}

/// A shift's result, its left half, and that half's label.
struct Target {
    date: Date,
    from: String,
    label: String,
}

/// Resolve a shift or anchor. Only a `wrapped` question may be a lone anchor
/// or `in 3 weeks`; [`standalone`] ones such as `next Friday` need no wrapper.
fn target(core: &[&str], wrapped: bool, today: Date, forward: bool) -> Option<Target> {
    let (anchor, by) = match parse_shift(core, today) {
        Some(found) => found,
        None => match core {
            [] => return None,
            ["in", rest @ ..] if wrapped => (anchor_today(today), steps_all(rest, 1)?),
            _ if wrapped || standalone(core) => (day(core, today, forward)?, Vec::new()),
            _ => return None,
        },
    };
    // Spoken order, one step at a time, so each month or year step clamps alone.
    let (mut date, mut from) = (anchor.date, anchor.said);
    for step in &by {
        date = step.apply(date)?;
        let sign = if step.sign > 0 { '+' } else { '-' };
        from = format!("{from} {sign} {}", step.said());
    }
    Some(Target {
        date,
        from,
        label: anchor.label,
    })
}

/// A shift with its anchor, in any of the unwrapped spellings.
fn parse_shift(core: &[&str], today: Date) -> Option<(Anchor, Vec<Step>)> {
    let led = steps(core).and_then(|(mut by, rest)| {
        let (sign, anchor) = match rest {
            ["ago"] | ["ago", "was"] => (-1, anchor_today(today)),
            [dir @ ("from" | "after" | "before"), rest @ ..] => {
                // Bare weekday only in `<n> weeks from Friday`; `30 days from monday` stays a search.
                let weeks = by.iter().all(|s| matches!(s.unit, Unit::Week));
                let anchor = day(rest, today, true).or_else(|| match rest {
                    [w] if *dir == "from" && weeks => upcoming(w, today, ""),
                    _ => None,
                })?;
                (if *dir == "before" { -1 } else { 1 }, anchor)
            }
            _ => return None,
        };
        by.iter_mut().for_each(|s| s.sign = sign);
        Some((anchor, by))
    });
    if led.is_some() {
        return led;
    }
    // `<anchor> + <steps>`: the last operator splits them.
    let at = core
        .iter()
        .rposition(|w| matches!(*w, "+" | "plus" | "-" | "minus"))?;
    let sign = if matches!(core[at], "+" | "plus") {
        1
    } else {
        -1
    };
    Some((
        day(&core[..at], today, true)?,
        steps_all(&core[at + 1..], sign)?,
    ))
}

/// Leading `<n> <unit>` steps, `and` allowed between, and the words after.
fn steps<'a, 'b>(words: &'a [&'b str]) -> Option<(Vec<Step>, &'a [&'b str])> {
    let mut found = Vec::new();
    let mut rest = words;
    while let Some((s, after)) = one_step(rest) {
        found.push(s);
        rest = strip(after, &["and"])
            .filter(|next| one_step(next).is_some())
            .unwrap_or(after);
    }
    (!found.is_empty() && found.len() <= 4).then_some((found, rest))
}

/// [`steps`] using every word, all with `sign`.
fn steps_all(words: &[&str], sign: i64) -> Option<Vec<Step>> {
    let (mut by, rest) = steps(words)?;
    by.iter_mut().for_each(|s| s.sign = sign);
    rest.is_empty().then_some(by)
}

/// `30 days`, `10 business days`, and what follows.
fn one_step<'a, 'b>(words: &'a [&'b str]) -> Option<(Step, &'a [&'b str])> {
    match words {
        [n, kind @ ("business" | "working"), "day" | "days", rest @ ..] => {
            let word = if *kind == "business" {
                "business day"
            } else {
                "working day"
            };
            let count = number(n)?;
            let unit = Unit::Business(word);
            Some((
                Step {
                    count,
                    unit,
                    sign: 1,
                },
                rest,
            ))
        }
        [n, unit, rest @ ..] => Some((step(n, unit, 1)?, rest)),
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum Unit {
    Day,
    Week,
    Month,
    Year,
    /// Mon-Fri only; the singular word typed.
    Business(&'static str),
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
            Unit::Business(_) => return None,
        }
        .ok()
    }

    fn apply(&self, date: Date) -> Option<Date> {
        match self.unit {
            Unit::Business(_) => business(date, self.count, self.sign),
            _ => date.checked_add(self.span()?).ok(),
        }
    }

    /// `30 days`, `1 week`, `10 business days`.
    fn said(&self) -> String {
        let word = match self.unit {
            Unit::Day => "day",
            Unit::Week => "week",
            Unit::Month => "month",
            Unit::Year => "year",
            Unit::Business(w) => w,
        };
        plural(self.count, word)
    }
}

/// `n` Mon-Fri days from `date` toward `sign`; whole weeks jump 7 days at once,
/// keeping five for the walk so a weekend start still lands on a weekday.
fn business(date: Date, n: i64, sign: i64) -> Option<Date> {
    let (mut weeks, mut rem) = (n / 5, n % 5);
    if rem == 0 && weeks > 0 {
        (weeks, rem) = (weeks - 1, 5);
    }
    let mut d = date
        .checked_add(Span::new().try_days(sign * weeks * 7).ok()?)
        .ok()?;
    while rem > 0 {
        d = d.checked_add(Span::new().days(sign)).ok()?;
        if d.weekday().to_monday_zero_offset() < 5 {
            rem -= 1;
        }
    }
    Some(d)
}

/// `30`, `a`, `two`. Digits at most six: no signs, no decimals.
fn number(n: &str) -> Option<i64> {
    const WORDS: [&str; 9] = [
        "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
    ];
    match n {
        "a" | "an" | "one" => Some(1),
        _ if WORDS.contains(&n) => Some(WORDS.iter().position(|w| *w == n)? as i64 + 2),
        _ if (1..=6).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_digit()) => n.parse().ok(),
        _ => None,
    }
}

/// `30 days`, `a week`, `two weeks`, `10 weekdays`.
fn step(n: &str, unit: &str, sign: i64) -> Option<Step> {
    let unit = match unit {
        "day" | "days" => Unit::Day,
        "week" | "weeks" => Unit::Week,
        "month" | "months" => Unit::Month,
        "year" | "years" => Unit::Year,
        "weekday" | "weekdays" => Unit::Business("weekday"),
        _ => return None,
    };
    Some(Step {
        count: number(n)?,
        unit,
        sign,
    })
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

/// First `name`d weekday strictly after today, so a week on that weekday.
/// `said` prefixes the name on the card.
fn upcoming(name: &str, today: Date, said: &str) -> Option<Anchor> {
    nearby("next", name, today, said)
}

/// `next` = strictly after today, `last` = strictly before, `this` = in
/// today's Mon-Sun week, today included.
fn nearby(rel: &str, name: &str, today: Date, said: &str) -> Option<Anchor> {
    let w = WEEKDAYS
        .into_iter()
        .find(|w| weekday_name(*w).eq_ignore_ascii_case(name))?;
    let gap = i64::from(w.to_monday_zero_offset() - today.weekday().to_monday_zero_offset());
    let days = match rel {
        "next" if gap <= 0 => gap + 7,
        "last" if gap >= 0 => gap - 7,
        _ => gap,
    };
    let date = today.checked_add(Span::new().days(days)).ok()?;
    Some(Anchor {
        date,
        said: format!("{said}{}", weekday_name(w)),
        label: short(date),
    })
}

/// `Friday in 3 weeks`: next Friday, then whole weeks, as `3 weeks from Friday`.
fn weekday_in(name: &str, n: &str, unit: &str, today: Date) -> Option<Anchor> {
    let by = step(n, unit, 1).filter(|s| matches!(s.unit, Unit::Week))?;
    let next = upcoming(name, today, "")?;
    let date = next.date.checked_add(by.span()?).ok()?;
    Some(Anchor {
        date,
        said: format!("{} in {}", next.said, by.said()),
        label: short(date),
    })
}

/// Last day of `month`, `this month`, `next year`, `the year`: today's period,
/// or the one either side.
fn period_end(period: &[&str], today: Date) -> Option<Date> {
    let (shift, unit) = match period {
        [unit] | ["this" | "the", unit] => (0, *unit),
        ["next", unit] => (1, *unit),
        ["last", unit] => (-1, *unit),
        _ => return None,
    };
    match unit {
        "month" => {
            let first = today.first_of_month();
            Some(
                first
                    .checked_add(Span::new().months(shift))
                    .ok()?
                    .last_of_month(),
            )
        }
        "year" => Date::new(today.year().checked_add(shift as i16)?, 12, 31).ok(),
        _ => None,
    }
}

/// Unwrapped anchors specific enough for the top row: `next Friday`, `day
/// after tomorrow`, `10 Sep next year`. `the day after tomorrow` is a film.
fn standalone(core: &[&str]) -> bool {
    match core {
        ["next" | "last" | "this", _] | [_, "in", _, _] => true,
        ["day", "after", "tomorrow"] | ["day", "before", "yesterday"] => true,
        [first, .., "next" | "this" | "last", "year"] => !matches!(*first, "end" | "the"),
        _ => false,
    }
}

/// `today`, `tomorrow`, `day after tomorrow`, `next Friday`, `end of next month`
/// or a calendar date. `forward` picks the next occurrence of a year-less date.
fn day(words: &[&str], today: Date, forward: bool) -> Option<Anchor> {
    day_near(words, today, today, forward)
}

/// [`day`], a year-less date resolved around `near` rather than today.
fn day_near(words: &[&str], today: Date, near: Date, forward: bool) -> Option<Anchor> {
    let (offset, said) = match words {
        ["now"] | ["today"] => (0, "today"),
        ["tomorrow"] => (1, "tomorrow"),
        ["yesterday"] => (-1, "yesterday"),
        ["the", "day", "after", "tomorrow"] | ["day", "after", "tomorrow"] => {
            (2, "day after tomorrow")
        }
        ["the", "day", "before", "yesterday"] | ["day", "before", "yesterday"] => {
            (-2, "day before yesterday")
        }
        [rel @ ("next" | "last" | "this"), w] => return nearby(rel, w, today, &format!("{rel} ")),
        [w, "in", n, unit] => return weekday_in(w, n, unit, today),
        ["the", "end", "of", period @ ..] | ["end", "of", period @ ..] => {
            let date = period_end(period, today)?;
            return Some(relative(
                &format!("end of {}", period.join(" ")),
                date,
                today,
            ));
        }
        _ => {
            let date = calendar(words, today, near, forward)?;
            return Some(Anchor {
                date,
                said: full(date),
                label: short(date),
            });
        }
    };
    let date = today.checked_add(Span::new().days(offset)).ok()?;
    Some(relative(said, date, today))
}

/// `20th september 2027`, `the 20th of sep`, `sept 20 next year`, `2027-09-20`.
fn calendar(words: &[&str], today: Date, near: Date, forward: bool) -> Option<Date> {
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
        [] => nearest(d, m, near, forward),
        _ => Date::new(year_of(y, today)?, m, d).ok(),
    }
}

/// `2027`, `this year`, `next year`, `last year`.
fn year_of(words: &[&str], today: Date) -> Option<i16> {
    match words {
        [y] => four_digits(y),
        ["this", "year"] => Some(today.year()),
        ["next", "year"] => today.year().checked_add(1),
        ["last", "year"] => today.year().checked_sub(1),
        _ => None,
    }
}

fn four_digits(s: &str) -> Option<i16> {
    (s.len() == 4 && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
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

const WEEKDAYS: [Weekday; 7] = [
    Weekday::Monday,
    Weekday::Tuesday,
    Weekday::Wednesday,
    Weekday::Thursday,
    Weekday::Friday,
    Weekday::Saturday,
    Weekday::Sunday,
];

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
