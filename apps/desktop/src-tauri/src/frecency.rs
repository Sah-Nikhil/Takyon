//! Frecency: how often and how recently an Entry was chosen (CONTEXT.md).
//!
//! `weight = Σ 0.5^(age_days / 30)` — IMPLEMENTATION_PLAN §3. Stored already
//! decayed with a `decayed_at` stamp and re-decayed lazily on read, so there is
//! no background job and no clock-skew bug: a clock that jumps backwards makes
//! the elapsed time negative, which is clamped rather than inflating a score.
//!
//! Keyed by `EntryId`, which is why v0.3 task 0 had to land first — nine
//! applications sharing one id would have shared one history.

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension, Result};

use crate::entry::{EntryId, EntryKind};

/// Days for a weight to halve. A guess, per the plan — tune from real use.
pub const HALF_LIFE_DAYS: f64 = 30.0;

/// Carry a stored weight forward to now.
///
/// Negative elapsed time is clamped to zero. A clock that jumps backwards would
/// otherwise raise `0.5` to a negative power and *grow* the weight, which is a
/// score nobody earned and nothing later would correct.
pub fn decay(weight: f64, elapsed_days: f64) -> f64 {
    weight * 0.5_f64.powf(elapsed_days.max(0.0) / HALF_LIFE_DAYS)
}

/// The usage database. Schema is IMPLEMENTATION_PLAN §4's `usage` table.
///
/// One connection behind a `Mutex`: writes happen once per launch and reads once
/// per keystroke against a table with one row per application, so contention is
/// not the constraint. A pool would be machinery for a load that does not exist.
pub struct Frecency {
    conn: Mutex<Connection>,
}

impl Frecency {
    /// Open `frecency.db` in `dir`, or an in-memory database when `dir` is
    /// `None` — the seam the tests use, and the fallback when there is no data
    /// directory to write to.
    pub fn open(dir: Option<PathBuf>) -> Result<Self> {
        let conn = match dir {
            Some(dir) => {
                std::fs::create_dir_all(&dir).ok();
                Connection::open(dir.join("frecency.db"))?
            }
            None => Connection::open_in_memory()?,
        };
        // WAL so a write during a keystroke does not block the read serving it.
        // Ignored rather than checked: an in-memory database refuses it, and a
        // journal mode is a performance choice, not a correctness one.
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        conn.execute(
            "CREATE TABLE IF NOT EXISTS usage (
                 entry_id   TEXT PRIMARY KEY,
                 kind       TEXT    NOT NULL,
                 count      INTEGER NOT NULL,
                 last_used  INTEGER NOT NULL,
                 score      REAL    NOT NULL,
                 decayed_at INTEGER NOT NULL
             )",
            [],
        )?;
        Ok(Frecency {
            conn: Mutex::new(conn),
        })
    }

    /// Record one activation at `now` (unix seconds).
    ///
    /// The stored score is decayed forward to `now` *before* the new unit is
    /// added, which is the whole job of `decayed_at`: without it an launch from a
    /// year ago would still be worth a full unit.
    pub fn record_at(&self, id: &EntryId, kind: EntryKind, now: i64) -> Result<()> {
        let conn = self.conn.lock().expect("frecency mutex");
        let previous: Option<(f64, i64)> = conn
            .query_row(
                "SELECT score, decayed_at FROM usage WHERE entry_id = ?1",
                [id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        let score = match previous {
            Some((score, decayed_at)) => decay(score, days_between(decayed_at, now)) + 1.0,
            None => 1.0,
        };
        conn.execute(
            "INSERT INTO usage (entry_id, kind, count, last_used, score, decayed_at)
             VALUES (?1, ?2, 1, ?3, ?4, ?3)
             ON CONFLICT(entry_id) DO UPDATE SET
                 kind = ?2, count = count + 1, last_used = ?3, score = ?4, decayed_at = ?3",
            params![id.as_str(), kind_name(kind), now, score],
        )?;
        Ok(())
    }

    /// The decayed weight of one Entry at `now`. Zero for anything never chosen.
    pub fn weight_at(&self, id: &EntryId, now: i64) -> f64 {
        let Ok(conn) = self.conn.lock() else {
            return 0.0;
        };
        let row: Option<(f64, i64)> = conn
            .query_row(
                "SELECT score, decayed_at FROM usage WHERE entry_id = ?1",
                [id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .unwrap_or(None);
        match row {
            Some((score, decayed_at)) => decay(score, days_between(decayed_at, now)),
            None => 0.0,
        }
    }

    /// Record one activation now.
    pub fn record(&self, id: &EntryId, kind: EntryKind) -> Result<()> {
        self.record_at(id, kind, unix_now())
    }

    /// Weight now. What the ranker calls, once per candidate Entry.
    pub fn weight(&self, id: &EntryId) -> f64 {
        self.weight_at(id, unix_now())
    }
}

fn days_between(from: i64, to: i64) -> f64 {
    (to - from) as f64 / 86_400.0
}

/// Seconds since the epoch, or 0 if the clock is before it.
fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Stored so a later phase can read usage per kind. Spelled here rather than via
/// `Serialize`, so the wire format and the database schema can move apart.
fn kind_name(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::App => "app",
        EntryKind::File => "file",
        EntryKind::Folder => "folder",
        EntryKind::Clip => "clip",
        EntryKind::Calc => "calc",
        EntryKind::Recent => "recent",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The half-life, as the two numbers anyone can check by hand.
    #[test]
    fn v0_3_a_weight_halves_every_thirty_days() {
        assert!((decay(1.0, 0.0) - 1.0).abs() < 1e-9);
        assert!((decay(1.0, HALF_LIFE_DAYS) - 0.5).abs() < 1e-9);
        assert!((decay(1.0, HALF_LIFE_DAYS * 2.0) - 0.25).abs() < 1e-9);
    }

    /// A clock that went backwards must not inflate a score. Negative elapsed
    /// time is treated as none rather than as growth.
    #[test]
    fn v0_3_a_backwards_clock_does_not_grow_a_weight() {
        assert!((decay(1.0, -90.0) - 1.0).abs() < 1e-9);
    }

    const DAY: i64 = 86_400;

    fn id(s: &str) -> EntryId {
        EntryId(s.into())
    }

    /// Nothing chosen, nothing learned. An unknown Entry must weigh zero rather
    /// than some floor, or every Source's output gets the same nudge.
    #[test]
    fn v0_3_an_unrecorded_entry_weighs_nothing() {
        let f = Frecency::open(None).unwrap();
        assert_eq!(f.weight_at(&id("c:\\a.exe"), 0), 0.0);
    }

    /// One launch, one unit. Two launches at the same moment, two units — the
    /// "how often" half, before any decay applies.
    #[test]
    fn v0_3_each_launch_adds_a_unit() {
        let f = Frecency::open(None).unwrap();
        let vsc = id("c:\\vsc\\code.exe");
        f.record_at(&vsc, EntryKind::App, 0).unwrap();
        assert!((f.weight_at(&vsc, 0) - 1.0).abs() < 1e-9);
        f.record_at(&vsc, EntryKind::App, 0).unwrap();
        assert!((f.weight_at(&vsc, 0) - 2.0).abs() < 1e-9);
    }

    /// The "how recently" half. One launch a half-life ago is worth half of one
    /// launch now, and the store applies that on read without a background job.
    #[test]
    fn v0_3_a_weight_is_decayed_lazily_on_read() {
        let f = Frecency::open(None).unwrap();
        let slack = id("c:\\slack\\slack.exe");
        f.record_at(&slack, EntryKind::App, 0).unwrap();
        let thirty_days = 30 * DAY;
        assert!((f.weight_at(&slack, thirty_days) - 0.5).abs() < 1e-6);
        assert!((f.weight_at(&slack, 60 * DAY) - 0.25).abs() < 1e-6);
    }

    /// An old launch plus a new one is not two units. The stored value is decayed
    /// forward *before* the new unit lands, which is what `decayed_at` is for.
    #[test]
    fn v0_3_a_new_launch_lands_on_top_of_a_decayed_one() {
        let f = Frecency::open(None).unwrap();
        let e = id("c:\\a.exe");
        f.record_at(&e, EntryKind::App, 0).unwrap();
        f.record_at(&e, EntryKind::App, 30 * DAY).unwrap();
        // 1.0 halved to 0.5, then +1.0.
        assert!((f.weight_at(&e, 30 * DAY) - 1.5).abs() < 1e-6);
    }

    /// Usage outlives the process, or the ranker relearns from nothing every
    /// login and the whole phase is theatre.
    #[test]
    fn v0_3_usage_survives_a_restart() {
        let dir = std::env::temp_dir().join("takyon-frecency-restart");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let e = id("c:\\a.exe");
        {
            let f = Frecency::open(Some(dir.clone())).unwrap();
            f.record_at(&e, EntryKind::App, 0).unwrap();
        }
        let reopened = Frecency::open(Some(dir.clone())).unwrap();
        assert!((reopened.weight_at(&e, 0) - 1.0).abs() < 1e-9);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Two Entries, two histories. The id is the key, so task 0's fix is what
    /// keeps nine `cmd.exe` prompts from sharing one of these.
    #[test]
    fn v0_3_two_ids_keep_separate_histories() {
        let f = Frecency::open(None).unwrap();
        let a = id("c:\\windows\\system32\\cmd.exe");
        let b = id("c:\\windows\\system32\\cmd.exe|/k vsdevcmd.bat");
        f.record_at(&a, EntryKind::App, 0).unwrap();
        assert!((f.weight_at(&a, 0) - 1.0).abs() < 1e-9);
        assert_eq!(f.weight_at(&b, 0), 0.0);
    }
}
