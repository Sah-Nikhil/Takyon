//! Reading a Focus session's length from what the user typed.
//!
//! One parser for the start form's Duration field and `!f`'s tail. Pure: the
//! caller passes local time of day, so no clock, no timezone crate.

/// Longest Focus session: a day, as Raycast.
pub const MAX_MS: u64 = 24 * 60 * 60 * 1000;

const MINUTE_MS: u64 = 60 * 1000;

/// Why a duration was refused. Shown in words by the form.
#[derive(Debug, PartialEq, Eq)]
pub enum DurationError {
    /// Not a length or a time we read.
    Unrecognised,
    /// Parsed, but zero long.
    Zero,
    /// Longer than [`MAX_MS`].
    OverDay,
}

/// Length in ms of a session described by `text`.
///
/// `local_ms_of_day` = ms since local midnight now; only `until …` reads it.
pub fn parse(text: &str, local_ms_of_day: u32) -> Result<u64, DurationError> {
    let t = text.trim().to_ascii_lowercase();
    let ms = match until(&t) {
        Some(time) => clock(time.trim()).map(|targets| wait(&targets, local_ms_of_day)),
        None => length(&t),
    }
    .ok_or(DurationError::Unrecognised)?;
    match ms {
        0 => Err(DurationError::Zero),
        ms if ms > MAX_MS => Err(DurationError::OverDay),
        ms => Ok(ms),
    }
}

/// Sum of `<number><unit>` terms: `1h 30m`, `1.5h`, `1h30`.
///
/// Unitless number allowed only last: minutes after hours, or alone.
fn length(t: &str) -> Option<u64> {
    let mut rest = t;
    let mut total = 0.0;
    while !rest.is_empty() {
        let num_end = rest
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(rest.len());
        let n: f64 = rest[..num_end].parse().ok()?;
        rest = rest[num_end..].trim_start();
        let unit_end = rest
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(rest.len());
        let unit = &rest[..unit_end];
        rest = rest[unit_end..].trim_start();
        if unit.is_empty() && !rest.is_empty() {
            return None;
        }
        total += n * unit_ms(unit)? as f64;
    }
    (total.is_finite() && t.chars().any(|c| c.is_ascii_digit())).then(|| total.round() as u64)
}

/// A `!f` line read as goal plus trailing duration.
#[derive(Debug, PartialEq, Eq)]
pub struct Tail<'a> {
    pub goal: &'a str,
    /// None = no duration at the end; whole line is the goal.
    pub duration: Option<Result<u64, DurationError>>,
}

/// Split `!f write report 45m` into goal and length.
///
/// Longest trailing run of words that reads as a duration wins. That run must
/// hold a letter (a unit or `until`), so "read chapter 3" stays all goal.
pub fn split_tail(line: &str, local_ms_of_day: u32) -> Tail<'_> {
    let line = line.trim();
    let starts = line
        .char_indices()
        .filter(|&(i, c)| {
            !c.is_whitespace() && (i == 0 || line[..i].ends_with(char::is_whitespace))
        })
        .map(|(i, _)| i);
    for start in starts {
        let tail = &line[start..];
        if !tail.chars().any(|c| c.is_ascii_alphabetic()) {
            continue;
        }
        let duration = parse(tail, local_ms_of_day);
        if duration != Err(DurationError::Unrecognised) {
            return Tail {
                goal: line[..start].trim_end(),
                duration: Some(duration),
            };
        }
    }
    Tail {
        goal: line,
        duration: None,
    }
}

/// Text after `until`/`till`/`til`, or None when `t` is a length.
fn until(t: &str) -> Option<&str> {
    let (word, rest) = t.split_once(char::is_whitespace).unwrap_or((t, ""));
    matches!(word, "until" | "till" | "til").then_some(rest)
}

/// Candidate targets, ms of day, for a clock time.
///
/// Bare `4:30` gives two (04:30, 16:30); [`wait`] takes the nearer. Leading zero
/// or hour over 12 = 24-hour clock, one target.
fn clock(time: &str) -> Option<Vec<u32>> {
    let hm = |h: u32, m: u32| (h * 60 + m) * 60 * 1000;
    match time {
        "noon" => return Some(vec![hm(12, 0)]),
        "midnight" => return Some(vec![hm(0, 0)]),
        _ => {}
    }
    let (core, pm) = match time.strip_suffix("pm").or_else(|| time.strip_suffix("am")) {
        Some(core) => (core.trim_end(), Some(time.ends_with("pm"))),
        None => (time, None),
    };
    let (h_text, m_text) = core.split_once(':').unwrap_or((core, "00"));
    let digits = |s: &str, len: std::ops::RangeInclusive<usize>| {
        (len.contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse::<u32>().ok())
            .flatten()
    };
    let (h, m) = (digits(h_text, 1..=2)?, digits(m_text, 2..=2)?);
    if m >= 60 {
        return None;
    }
    match pm {
        Some(pm) if (1..=12).contains(&h) => Some(vec![hm(h % 12 + if pm { 12 } else { 0 }, m)]),
        Some(_) => None,
        None if h > 23 => None,
        None if h == 0 || h > 12 || h_text.starts_with('0') => Some(vec![hm(h, m)]),
        None => Some(vec![hm(h % 12, m), hm(h % 12 + 12, m)]),
    }
}

/// ms from `now` to the nearest target. Now itself = a full day, not zero.
fn wait(targets: &[u32], now: u32) -> u64 {
    targets
        .iter()
        .map(
            |&t| match (i64::from(t) - i64::from(now)).rem_euclid(MAX_MS as i64) {
                0 => MAX_MS,
                ms => ms as u64,
            },
        )
        .min()
        .unwrap_or(MAX_MS)
}

/// ms per unit word. Empty = minutes: "90" means ninety minutes.
fn unit_ms(unit: &str) -> Option<u64> {
    match unit {
        "" | "m" | "min" | "mins" | "minute" | "minutes" => Some(MINUTE_MS),
        "h" | "hr" | "hrs" | "hour" | "hours" => Some(60 * MINUTE_MS),
        "d" | "day" | "days" => Some(24 * 60 * MINUTE_MS),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: u64 = 60 * 1000;

    #[test]
    fn v0_16_minutes_with_a_suffix() {
        assert_eq!(parse("45m", 0), Ok(45 * MIN));
    }

    #[test]
    fn v0_16_a_bare_number_is_minutes() {
        assert_eq!(parse("90", 0), Ok(90 * MIN));
    }

    #[test]
    fn v0_16_units_are_read_in_any_common_spelling() {
        for (text, want) in [
            ("2h", 120),
            ("2 hours", 120),
            ("1 hr", 60),
            ("3hrs", 180),
            ("45 min", 45),
            ("45 minutes", 45),
            ("1 minute", 1),
            ("  45 MIN ", 45),
        ] {
            assert_eq!(parse(text, 0), Ok(want * MIN), "{text}");
        }
    }

    #[test]
    fn v0_16_hours_and_minutes_combine() {
        for (text, want) in [
            ("1h30", 90),
            ("1h30m", 90),
            ("1h 30m", 90),
            ("1 hour 30 minutes", 90),
            ("1.5h", 90),
            ("2.25 hours", 135),
        ] {
            assert_eq!(parse(text, 0), Ok(want * MIN), "{text}");
        }
    }

    #[test]
    fn v0_16_a_whole_day_is_the_ceiling() {
        assert_eq!(parse("24h", 0), Ok(MAX_MS));
        assert_eq!(parse("1 day", 0), Ok(MAX_MS));
        assert_eq!(parse("24h 1m", 0), Err(DurationError::OverDay));
        assert_eq!(parse("2 days", 0), Err(DurationError::OverDay));
    }

    #[test]
    fn v0_16_zero_is_refused() {
        assert_eq!(parse("0", 0), Err(DurationError::Zero));
        assert_eq!(parse("0h 0m", 0), Err(DurationError::Zero));
    }

    /// Local time of day, ms since midnight.
    fn at(h: u32, m: u32, s: u32) -> u32 {
        ((h * 60 + m) * 60 + s) * 1000
    }

    #[test]
    fn v0_16_until_a_clock_time() {
        let two_pm = at(14, 0, 0);
        for (text, want) in [
            ("until 4:30pm", 150),
            ("until 4:30 pm", 150),
            ("until 4:30PM", 150),
            ("until 16:30", 150),
            ("until 4pm", 120),
            ("till 4pm", 120),
            ("until noon", 22 * 60),
            ("until midnight", 10 * 60),
        ] {
            assert_eq!(parse(text, two_pm), Ok(want * MIN), "{text}");
        }
    }

    #[test]
    fn v0_16_until_ends_on_the_minute_not_a_minute_from_now() {
        assert_eq!(parse("until 4:30pm", at(14, 0, 30)), Ok(150 * MIN - 30_000));
    }

    #[test]
    fn v0_16_a_time_already_past_today_means_tomorrow() {
        assert_eq!(parse("until 9am", at(14, 0, 0)), Ok(19 * 60 * MIN));
        assert_eq!(parse("until 12am", at(23, 0, 0)), Ok(60 * MIN));
        assert_eq!(parse("until 4:30pm", at(16, 30, 0)), Ok(MAX_MS));
    }

    #[test]
    fn v0_16_a_bare_twelve_hour_time_is_its_next_occurrence() {
        assert_eq!(parse("until 4:30", at(14, 0, 0)), Ok(150 * MIN));
        assert_eq!(parse("until 4:30", at(3, 0, 0)), Ok(90 * MIN));
        assert_eq!(parse("until 4:30", at(5, 0, 0)), Ok(690 * MIN));
        assert_eq!(parse("until 12", at(11, 0, 0)), Ok(60 * MIN));
        assert_eq!(parse("until 12pm", at(11, 0, 0)), Ok(60 * MIN));
    }

    #[test]
    fn v0_16_impossible_clock_times_are_unrecognised() {
        for text in [
            "until",
            "until later",
            "until 13pm",
            "until 4:75",
            "until 24:00",
            "until 0pm",
            "until 4:3",
            "until 4:30xm",
        ] {
            assert_eq!(
                parse(text, at(14, 0, 0)),
                Err(DurationError::Unrecognised),
                "{text:?}"
            );
        }
    }

    #[test]
    fn v0_16_bang_line_splits_into_goal_and_duration() {
        let two_pm = at(14, 0, 0);
        for (line, goal, want) in [
            ("write report 45m", "write report", 45),
            ("write report until 4:30pm", "write report", 150),
            ("write report 1 hour 30 minutes", "write report", 90),
            ("  write report   1h 30m ", "write report", 90),
            ("wait until it rains 30m", "wait until it rains", 30),
            ("45m", "", 45),
        ] {
            let tail = split_tail(line, two_pm);
            assert_eq!(
                (tail.goal, tail.duration),
                (goal, Some(Ok(want * MIN))),
                "{line:?}"
            );
        }
    }

    /// "read chapter 3" is a goal, not three minutes. Bare numbers belong to
    /// the form's Duration field only.
    #[test]
    fn v0_16_bang_tail_needs_a_unit_or_until() {
        for line in ["read chapter 3", "write report", ""] {
            let tail = split_tail(line, 0);
            assert_eq!((tail.goal, tail.duration), (line.trim(), None), "{line:?}");
        }
    }

    #[test]
    fn v0_16_bang_tail_out_of_range_is_reported_not_swallowed() {
        let tail = split_tail("write report 25h", 0);
        assert_eq!(
            (tail.goal, tail.duration),
            ("write report", Some(Err(DurationError::OverDay)))
        );
    }

    #[test]
    fn v0_16_anything_else_is_unrecognised() {
        for text in [
            "", "   ", "soon", "m", "h30", "-5m", "45x", "30 1h", "1..5h", "1h30x", ".",
        ] {
            assert_eq!(parse(text, 0), Err(DurationError::Unrecognised), "{text:?}");
        }
    }
}
