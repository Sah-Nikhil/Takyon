//! Time zone conversion (v0.17). tzdb through `jiff`, offline: bundled in the
//! binary on Windows, `/usr/share/zoneinfo` on macOS (ADR-0038).
//!
//! Recognises `<time> <zone> [to|in <zone>]`, `time in <zone>`, `now in <zone>`
//! and `<zone> time`. Policy stays in `mod.rs`; this file only parses and
//! formats. Grammar and answer shape: `docs/plans/v0.17-time-zones.md`.
//!
//! Never answers a bare time or a bare zone: an app called Tokyo keeps its row.

use jiff::civil::{Date, Time, Weekday};
use jiff::tz::{Offset, TimeZone};
use jiff::{Timestamp, Zoned};

/// Everything an answer depends on besides the input. Injected so tests fix
/// the instant and the zone; one reading the wall clock fails on DST day.
#[derive(Debug, Clone)]
pub struct Clock {
    pub now: Timestamp,
    pub local: TimeZone,
    /// Windows short-time format uses `H`. Never `PM` for a 24-hour user.
    pub hour24: bool,
}

impl Clock {
    /// The machine's. `TimeZone::system` is cached by jiff for 5 min, so a
    /// zone change in Settings shows within that.
    pub fn system() -> Clock {
        Clock {
            now: Timestamp::now(),
            local: TimeZone::system(),
            hour24: hour24(),
        }
    }
}

/// One time zone card: the answer, the time asked from, and both zones in words.
pub struct Reply {
    pub display: String,
    pub from: String,
    pub labels: [String; 2],
}

/// Answer a time zone question, or decline.
pub fn answer(body: &str, clock: &Clock) -> Option<Reply> {
    let lower = body.trim().to_ascii_lowercase();
    if let Some(zone) = ask_now(&lower) {
        return Some(now_in(&zone, clock));
    }

    let (left, target) = match split_keyword(&lower) {
        Some((left, right)) => (left, Some(zone(right)?)),
        None => (lower.as_str(), None),
    };
    let (time, rest) = time_of_day(left)?;
    let source = zone(rest)?;
    let target = target.unwrap_or_else(|| Zone::local(clock));
    convert(time, &source, &target, clock)
}

/// `time in X`, `now in X`, `X time`. Anything else is not asking the time.
fn ask_now(lower: &str) -> Option<Zone> {
    let name = ["time in ", "now in "]
        .iter()
        .find_map(|p| lower.strip_prefix(p))
        .or_else(|| lower.strip_suffix(" time"))?;
    zone(name)
}

/// The time there, set against the time here.
fn now_in(zone: &Zone, clock: &Clock) -> Reply {
    let at = clock.now.to_zoned(zone.tz.clone());
    let here = clock.now.to_zoned(clock.local.clone());
    Reply {
        display: title(&at, here.date(), clock.hour24),
        from: format!("{} {}", clock_face(here.time(), clock.hour24), code(&here)),
        labels: [describe(&Zone::local(clock), &here), describe(zone, &at)],
    }
}

fn convert(time: Time, source: &Zone, target: &Zone, clock: &Clock) -> Option<Reply> {
    // No date means today in the source zone, which is what "12 pm utc" says.
    let day = clock.now.to_zoned(source.tz.clone()).date();
    // `to_zoned` is jiff's `compatible`: a DST gap shifts forward, a fold
    // takes the earlier instant.
    let from = source.tz.to_zoned(day.to_datetime(time)).ok()?;
    let to = from.with_time_zone(target.tz.clone());
    Some(Reply {
        display: title(&to, day, clock.hour24),
        from: format!("{} {}", clock_face(from.time(), clock.hour24), code(&from)),
        labels: [describe(source, &from), describe(target, &to)],
    })
}

/// `5:30 PM IST`, or `Thu 5:30 AM IST` when the day is not the one asked from.
fn title(at: &Zoned, from_day: Date, hour24: bool) -> String {
    let face = format!("{} {}", clock_face(at.time(), hour24), code(at));
    if at.date() == from_day {
        face
    } else {
        format!("{} {face}", weekday(at.weekday()))
    }
}

/// `Kolkata · India Standard Time, UTC+5:30 · Wed 7 Oct`. The card wraps it
/// to two lines at most.
fn describe(zone: &Zone, at: &Zoned) -> String {
    let mut parts = Vec::with_capacity(3);
    let offset = utc_offset(at.offset());
    // `Coordinated Universal Time, UTC+0` says one thing twice.
    let named = name_of(at).map(|n| match code(at).as_str() {
        "UTC" => n.to_string(),
        _ => format!("{n}, {offset}"),
    });
    if let Some(place) = &zone.place {
        parts.push(place.clone());
    }
    parts.push(named.unwrap_or(offset));
    parts.push(format!(
        "{} {} {}",
        weekday(at.weekday()),
        at.day(),
        MONTHS[at.month() as usize - 1]
    ));
    parts.join(" \u{b7} ")
}

/// tzdb's abbreviation when it is a word, else `UTC+4`: tzdb writes `+04`
/// for zones with no code in use, and inventing one would be wrong.
fn code(at: &Zoned) -> String {
    let info = at.time_zone().to_offset_info(at.timestamp());
    let abbr = info.abbreviation();
    if !abbr.is_empty() && abbr.chars().all(|c| c.is_ascii_alphabetic()) {
        abbr.to_string()
    } else {
        utc_offset(at.offset())
    }
}

fn utc_offset(offset: Offset) -> String {
    let secs = offset.seconds();
    let sign = if secs < 0 { '-' } else { '+' };
    let (h, m) = (secs.abs() / 3600, secs.abs() % 3600 / 60);
    if m == 0 {
        format!("UTC{sign}{h}")
    } else {
        format!("UTC{sign}{h}:{m:02}")
    }
}

fn clock_face(t: Time, hour24: bool) -> String {
    let (h, m) = (t.hour(), t.minute());
    if hour24 {
        return format!("{h:02}:{m:02}");
    }
    let meridiem = if h < 12 { "AM" } else { "PM" };
    let h12 = match h % 12 {
        0 => 12,
        h => h,
    };
    format!("{h12}:{m:02} {meridiem}")
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn weekday(w: Weekday) -> &'static str {
    match w {
        Weekday::Monday => "Mon",
        Weekday::Tuesday => "Tue",
        Weekday::Wednesday => "Wed",
        Weekday::Thursday => "Thu",
        Weekday::Friday => "Fri",
        Weekday::Saturday => "Sat",
        Weekday::Sunday => "Sun",
    }
}

/// Same rule as `units::split_keyword`: ` to ` before ` in `.
fn split_keyword(lower: &str) -> Option<(&str, &str)> {
    [" to ", " in "].iter().find_map(|k| {
        lower
            .find(k)
            .map(|at| (&lower[..at], &lower[at + k.len()..]))
    })
}

/// A time of day at the start of `s`, and what follows it.
///
/// Needs `am`/`pm` or a colon: `12 utc` is too close to arithmetic to claim.
fn time_of_day(s: &str) -> Option<(Time, &str)> {
    let s = s.trim_start();
    for (word, hour) in [("noon", 12), ("midnight", 0)] {
        if let Some(rest) = s.strip_prefix(word) {
            return Some((Time::new(hour, 0, 0, 0).ok()?, rest));
        }
    }

    let digits = |s: &str| s.bytes().take_while(u8::is_ascii_digit).count();
    let n = digits(s);
    if !(1..=2).contains(&n) {
        return None;
    }
    let mut hour: i8 = s[..n].parse().ok()?;
    let mut rest = &s[n..];
    let mut minute: i8 = 0;
    let colon = rest.starts_with(':');
    if colon {
        let m = digits(&rest[1..]);
        if m != 2 {
            return None;
        }
        minute = rest[1..3].parse().ok()?;
        rest = &rest[3..];
    }

    let spaced = rest.trim_start();
    let meridiem = ["a.m.", "p.m.", "am", "pm"]
        .into_iter()
        .find(|m| spaced.strip_prefix(m).is_some_and(boundary));
    match meridiem {
        Some(m) => {
            if !(1..=12).contains(&hour) {
                return None;
            }
            // 12 am is midnight, 12 pm is noon: the classic off-by-twelve.
            hour %= 12;
            if m.starts_with('p') {
                hour += 12;
            }
            rest = &spaced[m.len()..];
        }
        None if colon => {}
        None => return None,
    }
    if !boundary(rest) {
        return None;
    }
    Some((Time::new(hour, minute, 0, 0).ok()?, rest))
}

/// End of input or whitespace, so `12 pmx` is not `12 pm` and a zone `x`.
fn boundary(rest: &str) -> bool {
    rest.is_empty() || rest.starts_with(char::is_whitespace)
}

/// A zone as the user named it.
struct Zone {
    tz: TimeZone,
    /// City typed, last IANA segment, or `your time`. None for UTC and offsets.
    place: Option<String>,
}

impl Zone {
    fn local(clock: &Clock) -> Zone {
        Zone {
            tz: clock.local.clone(),
            place: Some("your time".to_string()),
        }
    }
}

/// Resolve a zone name: fixed UTC, offsets, codes, IANA ids, then cities.
fn zone(name: &str) -> Option<Zone> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    if let Some(tz) = fixed(name) {
        return Some(Zone { tz, place: None });
    }
    if let Some((_, iana)) = CODES.iter().find(|(c, _)| *c == name) {
        return iana_zone(iana);
    }
    if name.contains('/') {
        return iana_zone(name);
    }
    if let Some((_, iana)) = CITIES.iter().find(|(c, _)| *c == name) {
        let tz = TimeZone::get(iana).ok()?;
        return Some(Zone {
            tz,
            place: Some(title_case(name)),
        });
    }
    city(name)
}

fn iana_zone(iana: &str) -> Option<Zone> {
    let tz = TimeZone::get(iana).ok()?;
    let place = tz.iana_name().map(last_segment);
    Some(Zone { tz, place })
}

/// `utc`, `gmt`, `z`, and `utc+5:30` / `gmt-3`.
fn fixed(name: &str) -> Option<TimeZone> {
    if matches!(name, "utc" | "gmt" | "z") {
        return Some(TimeZone::UTC);
    }
    let rest = name
        .strip_prefix("utc")
        .or_else(|| name.strip_prefix("gmt"))?;
    let (sign, rest) = match rest.as_bytes().first()? {
        b'+' => (1, &rest[1..]),
        b'-' => (-1, &rest[1..]),
        _ => return None,
    };
    let (h, m) = rest.split_once(':').unwrap_or((rest, "0"));
    let (h, m): (i32, i32) = (h.parse().ok()?, m.parse().ok()?);
    if h > 14 || m >= 60 {
        return None;
    }
    let offset = Offset::from_seconds(sign * (h * 3600 + m * 60)).ok()?;
    Some(TimeZone::fixed(offset))
}

/// Any tzdb city, by the last segment of its id: `new york`, `tokyo`.
fn city(name: &str) -> Option<Zone> {
    let iana = jiff::tz::db()
        .available()
        .find(|id| last_segment(id.as_str()).eq_ignore_ascii_case(name))?;
    iana_zone(iana.as_str())
}

/// `America/New_York` reads `New York`.
fn last_segment(iana: &str) -> String {
    iana.rsplit('/').next().unwrap_or(iana).replace('_', " ")
}

fn title_case(name: &str) -> String {
    name.split(' ')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

/// Codes people type, each with one fixed reading (decided 2026-10-07 for the
/// three that collide: `ist` India, `cst` US Central, `bst` London). A code
/// names a zone, not an offset: `pst` in July is Pacific time, shown as PDT.
const CODES: &[(&str, &str)] = &[
    ("ist", "Asia/Kolkata"),
    ("pst", "America/Los_Angeles"),
    ("pdt", "America/Los_Angeles"),
    ("pt", "America/Los_Angeles"),
    ("mst", "America/Denver"),
    ("mdt", "America/Denver"),
    ("cst", "America/Chicago"),
    ("cdt", "America/Chicago"),
    ("est", "America/New_York"),
    ("edt", "America/New_York"),
    ("et", "America/New_York"),
    ("akst", "America/Anchorage"),
    ("hst", "Pacific/Honolulu"),
    ("bst", "Europe/London"),
    ("wet", "Europe/Lisbon"),
    ("cet", "Europe/Berlin"),
    ("cest", "Europe/Berlin"),
    ("eet", "Europe/Athens"),
    ("eest", "Europe/Athens"),
    ("msk", "Europe/Moscow"),
    ("pkt", "Asia/Karachi"),
    ("sgt", "Asia/Singapore"),
    ("hkt", "Asia/Hong_Kong"),
    ("jst", "Asia/Tokyo"),
    ("kst", "Asia/Seoul"),
    ("aest", "Australia/Sydney"),
    ("aedt", "Australia/Sydney"),
    ("nzst", "Pacific/Auckland"),
    ("nzdt", "Pacific/Auckland"),
];

/// Cities tzdb does not name, each to the zone it keeps.
const CITIES: &[(&str, &str)] = &[
    ("mumbai", "Asia/Kolkata"),
    ("delhi", "Asia/Kolkata"),
    ("new delhi", "Asia/Kolkata"),
    ("bangalore", "Asia/Kolkata"),
    ("bengaluru", "Asia/Kolkata"),
    ("chennai", "Asia/Kolkata"),
    ("hyderabad", "Asia/Kolkata"),
    ("san francisco", "America/Los_Angeles"),
    ("seattle", "America/Los_Angeles"),
    ("washington", "America/New_York"),
    ("boston", "America/New_York"),
    ("beijing", "Asia/Shanghai"),
    ("osaka", "Asia/Tokyo"),
];

/// The zone's name in words, keyed by zone **and** code: tzdb gives `IST` to
/// Kolkata, Dublin and Jerusalem. First match wins; a key ending in `/`
/// matches by prefix. A zone with no row shows its offset alone.
const NAMES: &[(&str, &str, &str)] = &[
    ("Asia/Kolkata", "IST", "India Standard Time"),
    ("Asia/Calcutta", "IST", "India Standard Time"),
    ("Europe/Dublin", "IST", "Irish Standard Time"),
    ("Asia/Jerusalem", "IST", "Israel Standard Time"),
    ("Asia/Jerusalem", "IDT", "Israel Daylight Time"),
    ("Asia/Shanghai", "CST", "China Standard Time"),
    ("Asia/Taipei", "CST", "Taipei Standard Time"),
    ("America/Havana", "CST", "Cuba Standard Time"),
    ("America/Havana", "CDT", "Cuba Daylight Time"),
    ("Europe/London", "BST", "British Summer Time"),
    ("", "GMT", "Greenwich Mean Time"),
    ("", "UTC", "Coordinated Universal Time"),
    ("America/", "PST", "Pacific Standard Time"),
    ("America/", "PDT", "Pacific Daylight Time"),
    ("America/", "MST", "Mountain Standard Time"),
    ("America/", "MDT", "Mountain Daylight Time"),
    ("America/", "CST", "Central Standard Time"),
    ("America/", "CDT", "Central Daylight Time"),
    ("America/", "EST", "Eastern Standard Time"),
    ("America/", "EDT", "Eastern Daylight Time"),
    ("America/", "AST", "Atlantic Standard Time"),
    ("America/", "ADT", "Atlantic Daylight Time"),
    ("America/", "AKST", "Alaska Standard Time"),
    ("America/", "AKDT", "Alaska Daylight Time"),
    ("Pacific/Honolulu", "HST", "Hawaii Standard Time"),
    ("Europe/", "WET", "Western European Time"),
    ("Europe/", "WEST", "Western European Summer Time"),
    ("Atlantic/", "WET", "Western European Time"),
    ("Atlantic/", "WEST", "Western European Summer Time"),
    ("", "CET", "Central European Time"),
    ("", "CEST", "Central European Summer Time"),
    ("", "EET", "Eastern European Time"),
    ("", "EEST", "Eastern European Summer Time"),
    ("Europe/", "MSK", "Moscow Time"),
    ("Asia/Karachi", "PKT", "Pakistan Standard Time"),
    ("Asia/Tokyo", "JST", "Japan Standard Time"),
    ("Asia/Seoul", "KST", "Korea Standard Time"),
    ("Asia/Hong_Kong", "HKT", "Hong Kong Time"),
    ("Asia/Jakarta", "WIB", "Western Indonesia Time"),
    ("Asia/Manila", "PST", "Philippine Standard Time"),
    ("Africa/", "SAST", "South Africa Standard Time"),
    ("Africa/", "CAT", "Central Africa Time"),
    ("Africa/", "EAT", "East Africa Time"),
    ("Africa/", "WAT", "West Africa Time"),
    ("Australia/", "AEST", "Australian Eastern Standard Time"),
    ("Australia/", "AEDT", "Australian Eastern Daylight Time"),
    ("Australia/", "ACST", "Australian Central Standard Time"),
    ("Australia/", "ACDT", "Australian Central Daylight Time"),
    ("Australia/", "AWST", "Australian Western Standard Time"),
    ("Pacific/Auckland", "NZST", "New Zealand Standard Time"),
    ("Pacific/Auckland", "NZDT", "New Zealand Daylight Time"),
];

fn name_of(at: &Zoned) -> Option<&'static str> {
    let code = code(at);
    let id = at.time_zone().iana_name().unwrap_or("");
    NAMES
        .iter()
        .find(|(zone, c, _)| {
            *c == code
                && (*zone == id || (zone.ends_with('/') || zone.is_empty()) && id.starts_with(zone))
        })
        .map(|(_, _, name)| *name)
}

/// The user's short-time format names hours with `H` when it is 24-hour.
#[cfg(windows)]
fn hour24() -> bool {
    use std::sync::OnceLock;
    use windows::Win32::Globalization::{GetLocaleInfoEx, LOCALE_SSHORTTIME};
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(|| {
        let mut buf = [0u16; 80];
        // SAFETY: buffer outlives the call; length passed is its own.
        let n = unsafe {
            GetLocaleInfoEx(
                windows::core::PCWSTR::null(),
                LOCALE_SSHORTTIME,
                Some(&mut buf),
            )
        };
        n > 0 && String::from_utf16_lossy(&buf[..n as usize]).contains('H')
    })
}

/// macOS reads its 24-hour setting at v0.12; 12-hour until then.
#[cfg(not(windows))]
fn hour24() -> bool {
    false
}
