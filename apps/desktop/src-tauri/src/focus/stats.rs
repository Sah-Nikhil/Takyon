//! Focus stats for Settings: focused time, per day and per Focus category,
//! and what got blocked most. Pure over `store` rows; days are local, from an
//! offset the frontend sends.

use serde::Serialize;

use super::session::Strictness;
use super::store::{Event, EventKind, Finished, Outcome};

const DAY_MS: i64 = 86_400_000;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub focused_ms: u64,
    pub sessions: u32,
    /// Ran their full length.
    pub full_length: u32,
    pub ended_early: u32,
    pub strict: u32,
    pub blocks: u32,
    pub snoozes: u32,
    /// One entry per day in the range, oldest first, zeros included.
    pub days: Vec<Day>,
    /// Focus categories by focused time with them blocked, most first.
    pub categories: Vec<Share>,
    /// Block list keys by times blocked, most first, at most ten.
    pub blocked: Vec<Count>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Day {
    /// Local midnight, ms since the epoch.
    pub start: i64,
    pub focused_ms: u64,
    pub sessions: u32,
    pub blocks: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Share {
    pub id: String,
    pub focused_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Count {
    pub key: String,
    pub count: u32,
}

/// Local midnight of the day holding `ms`, for a zone `offset_min` east of UTC.
pub fn local_midnight(ms: i64, offset_min: i32) -> i64 {
    let offset = i64::from(offset_min) * 60_000;
    (ms + offset).div_euclid(DAY_MS) * DAY_MS - offset
}

/// Stats over the `days` local days ending today.
pub fn compute(
    history: &[Finished],
    events: &[Event],
    now: u64,
    days: u32,
    offset_min: i32,
) -> Stats {
    let today = local_midnight(now as i64, offset_min);
    let first = today - i64::from(days.max(1) - 1) * DAY_MS;
    let mut stats = Stats {
        days: (0..days.max(1))
            .map(|i| Day {
                start: first + i64::from(i) * DAY_MS,
                ..Day::default()
            })
            .collect(),
        ..Stats::default()
    };
    let day_of = |ms: u64| {
        let start = local_midnight(ms as i64, offset_min);
        (start >= first).then(|| ((start - first) / DAY_MS) as usize)
    };
    let mut in_range = std::collections::HashSet::new();
    for f in history {
        let Some(day) = day_of(f.ended_at) else {
            continue;
        };
        in_range.insert(f.session.id);
        let focused = f.session.clock.focused_ms(f.ended_at);
        stats.focused_ms += focused;
        stats.sessions += 1;
        match f.outcome {
            Outcome::Expired => stats.full_length += 1,
            Outcome::Completed => stats.ended_early += 1,
        }
        if f.session.strictness == Strictness::Strict {
            stats.strict += 1;
        }
        if let Some(d) = stats.days.get_mut(day) {
            d.focused_ms += focused;
            d.sessions += 1;
        }
        for id in &f.session.block_list.categories {
            match stats.categories.iter_mut().find(|s| &s.id == id) {
                Some(share) => share.focused_ms += focused,
                None => stats.categories.push(Share {
                    id: id.clone(),
                    focused_ms: focused,
                }),
            }
        }
    }
    let mut counts: Vec<Count> = Vec::new();
    for e in events.iter().filter(|e| in_range.contains(&e.session_id)) {
        match e.kind {
            EventKind::Blocked => {
                stats.blocks += 1;
                if let Some(d) = day_of(e.at).and_then(|i| stats.days.get_mut(i)) {
                    d.blocks += 1;
                }
                let key = e.target.clone().unwrap_or_default();
                match counts.iter_mut().find(|c| c.key == key) {
                    Some(c) => c.count += 1,
                    None => counts.push(Count { key, count: 1 }),
                }
            }
            EventKind::Snoozed => stats.snoozes += 1,
            _ => {}
        }
    }
    stats
        .categories
        .sort_by(|a, b| b.focused_ms.cmp(&a.focused_ms).then(a.id.cmp(&b.id)));
    counts.sort_by(|a, b| b.count.cmp(&a.count).then(a.key.cmp(&b.key)));
    counts.truncate(10);
    stats.blocked = counts;
    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::focus::session::Clock;
    use crate::focus::store::{BlockList, Session};

    const MIN: u64 = 60_000;
    /// 2026-10-06 12:00 UTC.
    const NOON: u64 = 1_791_288_000_000;

    fn finished(
        id: i64,
        start: u64,
        length: u64,
        ended: u64,
        outcome: Outcome,
        cats: &[&str],
    ) -> Finished {
        Finished {
            session: Session {
                id,
                goal: "g".into(),
                strictness: if id % 2 == 0 {
                    Strictness::Strict
                } else {
                    Strictness::Soft
                },
                block_list: BlockList {
                    categories: cats.iter().map(|c| c.to_string()).collect(),
                    items: vec![],
                },
                clock: Clock::start(start, length),
            },
            ended_at: ended,
            outcome,
        }
    }

    fn event(session_id: i64, at: u64, kind: EventKind, target: &str) -> Event {
        Event {
            session_id,
            at,
            kind,
            target: Some(target.into()),
        }
    }

    #[test]
    fn v0_16_local_midnight_follows_the_offset() {
        // 12:00 UTC is 17:30 in India (+330): midnight there is 18:30 UTC the day before.
        assert_eq!(
            local_midnight(NOON as i64, 330),
            NOON as i64 - 12 * 3_600_000 - 330 * 60_000
        );
        assert_eq!(local_midnight(NOON as i64, 0), NOON as i64 - 12 * 3_600_000);
        // 12:00 UTC is 05:00 in UTC-7: midnight there is 07:00 UTC the same day.
        assert_eq!(
            local_midnight(NOON as i64, -420),
            NOON as i64 - 5 * 3_600_000
        );
    }

    #[test]
    fn v0_16_totals_count_focused_time_not_planned_time() {
        let history = [
            finished(
                1,
                NOON - 120 * MIN,
                60 * MIN,
                NOON - 60 * MIN,
                Outcome::Expired,
                &["social"],
            ),
            finished(
                2,
                NOON - 50 * MIN,
                45 * MIN,
                NOON - 30 * MIN,
                Outcome::Completed,
                &["social", "video"],
            ),
        ];
        let events = [
            event(1, NOON - 100 * MIN, EventKind::Blocked, "exe:discord.exe"),
            event(1, NOON - 99 * MIN, EventKind::Blocked, "exe:discord.exe"),
            event(2, NOON - 40 * MIN, EventKind::Blocked, "site:youtube.com"),
            event(2, NOON - 39 * MIN, EventKind::Snoozed, "site:youtube.com"),
        ];
        let stats = compute(&history, &events, NOON, 7, 0);
        assert_eq!(stats.focused_ms, 80 * MIN);
        assert_eq!(
            (
                stats.sessions,
                stats.full_length,
                stats.ended_early,
                stats.strict
            ),
            (2, 1, 1, 1)
        );
        assert_eq!((stats.blocks, stats.snoozes), (3, 1));
        assert_eq!(
            stats.categories,
            [
                Share {
                    id: "social".into(),
                    focused_ms: 80 * MIN
                },
                Share {
                    id: "video".into(),
                    focused_ms: 20 * MIN
                }
            ]
        );
        assert_eq!(
            stats.blocked[0],
            Count {
                key: "exe:discord.exe".into(),
                count: 2
            }
        );
    }

    #[test]
    fn v0_16_every_day_in_the_range_is_present_and_sessions_land_on_their_end_day() {
        let yesterday = NOON - 24 * 60 * MIN;
        let history = [finished(
            1,
            yesterday,
            30 * MIN,
            yesterday + 30 * MIN,
            Outcome::Expired,
            &[],
        )];
        let stats = compute(&history, &[], NOON, 7, 0);
        assert_eq!(stats.days.len(), 7);
        assert_eq!(stats.days[6].start, local_midnight(NOON as i64, 0));
        assert_eq!(stats.days[5].focused_ms, 30 * MIN);
        assert_eq!(stats.days[6].focused_ms, 0);
    }

    #[test]
    fn v0_16_sessions_outside_the_range_are_ignored() {
        let old = NOON - 10 * 24 * 60 * MIN;
        let history = [finished(
            1,
            old,
            30 * MIN,
            old + 30 * MIN,
            Outcome::Expired,
            &[],
        )];
        assert_eq!(compute(&history, &[], NOON, 7, 0).sessions, 0);
    }
}
