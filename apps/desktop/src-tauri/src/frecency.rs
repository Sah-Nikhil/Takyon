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

/// How many rows the Takyon-owned recents list keeps (TBC-0010).
///
/// A guess, in TBC-0009's company. Too small and yesterday's work is gone; too
/// large and it stops being "recent" and becomes a second, worse index.
pub const OPENED_CAP: i64 = 100;

/// One thing Takyon opened, for the Recents list it owns (TBC-0010).
#[derive(Clone, Debug)]
pub struct Opened {
    pub id: EntryId,
    pub path: PathBuf,
    /// As stored — `kind_name`'s spelling, not `EntryKind`. The table outlives
    /// any one build's enum.
    pub kind: String,
    pub opened_at: i64,
}

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
        // TBC-0010's table, beside the other learned usage data. Separate from
        // `usage` because they answer different questions and share only a file.
        conn.execute(
            "CREATE TABLE IF NOT EXISTS opened (
                 entry_id  TEXT PRIMARY KEY,
                 path      TEXT    NOT NULL,
                 kind      TEXT    NOT NULL,
                 opened_at INTEGER NOT NULL
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

    /// Remember that Takyon opened this, for the Recents list it owns (TBC-0010).
    ///
    /// **Not Frecency.** That answers "how often and how recently" and decays;
    /// this answers "what did I touch last", chronological and shallow. One
    /// database, two questions, nothing shared.
    pub fn record_opened_at(
        &self,
        id: &EntryId,
        path: &str,
        kind: EntryKind,
        now: i64,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("frecency mutex");
        conn.execute(
            "INSERT INTO opened (entry_id, path, kind, opened_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(entry_id) DO UPDATE SET path = ?2, kind = ?3, opened_at = ?4",
            params![id.as_str(), path, kind_name(kind), now],
        )?;
        // Capped by deleting the oldest beyond the cap, so the list stays
        // "recent" rather than becoming a second, worse index.
        conn.execute(
            "DELETE FROM opened WHERE entry_id NOT IN
                 (SELECT entry_id FROM opened ORDER BY opened_at DESC LIMIT ?1)",
            params![OPENED_CAP],
        )?;
        Ok(())
    }

    pub fn record_opened(&self, id: &EntryId, path: &str, kind: EntryKind) -> Result<()> {
        self.record_opened_at(id, path, kind, unix_now())
    }

    /// What Takyon opened, newest first, **existence-checked** (ADR-0013).
    ///
    /// People delete things, and a recents list of dead rows is the classic way
    /// this feature rots. Checked on read rather than swept: a path can vanish
    /// between two openings of the Palette.
    pub fn opened(&self, kinds: &[EntryKind], limit: usize) -> Vec<Opened> {
        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };
        let wanted: Vec<&str> = kinds.iter().map(|k| kind_name(*k)).collect();
        let Ok(mut stmt) =
            conn.prepare("SELECT entry_id, path, kind, opened_at FROM opened ORDER BY opened_at DESC")
        else {
            return Vec::new();
        };
        let rows = stmt.query_map([], |row| {
            Ok(Opened {
                id: EntryId(row.get(0)?),
                path: PathBuf::from(row.get::<_, String>(1)?),
                kind: row.get::<_, String>(2)?,
                opened_at: row.get(3)?,
            })
        });
        let Ok(rows) = rows else {
            return Vec::new();
        };
        rows.flatten()
            .filter(|row| wanted.is_empty() || wanted.contains(&row.kind.as_str()))
            .filter(|row| row.path.exists())
            .take(limit)
            .collect()
    }

    /// Forget everything Takyon opened. The Settings control behind TBC-0010's
    /// condition that a local history must be deletable in one action.
    pub fn clear_opened(&self) -> Result<usize> {
        let conn = self.conn.lock().expect("frecency mutex");
        conn.execute("DELETE FROM opened", [])
    }

    /// How many rows the list holds, dead ones included. What the Settings
    /// confirmation counts.
    pub fn opened_count(&self) -> usize {
        let Ok(conn) = self.conn.lock() else {
            return 0;
        };
        conn.query_row("SELECT COUNT(*) FROM opened", [], |row| {
            row.get::<_, i64>(0)
        })
        .map(|n| n as usize)
        .unwrap_or(0)
    }

    /// Weight now. What the ranker calls, once per candidate Entry.
    pub fn weight(&self, id: &EntryId) -> f64 {
        self.weight_at(id, unix_now())
    }

    /// The most-used Entry ids, heaviest first (v0.10).
    ///
    /// **Decayed on read**, never sorted by the stored score: `score` is only
    /// current as of `decayed_at`, so the column ranks fifty launches last year
    /// over five this week. Reading every row to sort a handful is deliberate.
    pub fn top_at(&self, limit: usize, now: i64) -> Vec<(EntryId, EntryKind)> {
        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };
        let Ok(mut stmt) = conn.prepare("SELECT entry_id, kind, score, decayed_at FROM usage")
        else {
            return Vec::new();
        };
        let rows = stmt.query_map([], |row| {
            Ok((
                EntryId(row.get(0)?),
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        });
        let Ok(rows) = rows else {
            return Vec::new();
        };
        let mut scored: Vec<(EntryId, EntryKind, f64)> = rows
            .flatten()
            .filter_map(|(id, kind, score, decayed_at)| {
                kind_of(&kind).map(|kind| (id, kind, decay(score, days_between(decayed_at, now))))
            })
            .collect();
        // `total_cmp`, not `partial_cmp().unwrap()`: a NaN from a corrupt row
        // would panic on the path that draws the Palette's first view.
        scored.sort_by(|a, b| b.2.total_cmp(&a.2));
        scored
            .into_iter()
            .take(limit)
            .map(|(id, kind, _)| (id, kind))
            .collect()
    }

    /// The same, now.
    pub fn top(&self, limit: usize) -> Vec<(EntryId, EntryKind)> {
        self.top_at(limit, unix_now())
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
        EntryKind::System => "system",
        EntryKind::SystemTask => "system-task",
        EntryKind::Command => "command",
    }
}

/// [`kind_name`] backwards, for reading a stored row (v0.10).
///
/// `None` rather than a default: an unrecognised name is a row written by a
/// build that knew a Kind this one does not, and guessing `App` for it would put
/// a wrong icon and a wrong action on the suggestion it produced.
fn kind_of(name: &str) -> Option<EntryKind> {
    Some(match name {
        "app" => EntryKind::App,
        "file" => EntryKind::File,
        "folder" => EntryKind::Folder,
        "clip" => EntryKind::Clip,
        "calc" => EntryKind::Calc,
        "recent" => EntryKind::Recent,
        "system" => EntryKind::System,
        "system-task" => EntryKind::SystemTask,
        "command" => EntryKind::Command,
        _ => return None,
    })
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

    /// A temp file that exists, so the existence check has something to pass.
    fn a_real_file(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("takyon-opened");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{label}-{}.txt", std::process::id()));
        std::fs::write(&path, "x").unwrap();
        path
    }

    /// TBC-0010's shape: chronological, newest first, and nothing to do with
    /// Frecency's ordering.
    #[test]
    fn v0_7_the_opened_list_is_chronological_newest_first() {
        let f = Frecency::open(None).unwrap();
        let (a, b) = (a_real_file("first"), a_real_file("second"));
        f.record_opened_at(&EntryId("a".into()), &a.to_string_lossy(), EntryKind::File, 100)
            .unwrap();
        f.record_opened_at(&EntryId("b".into()), &b.to_string_lossy(), EntryKind::File, 200)
            .unwrap();

        let rows = f.opened(&[], 10);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, EntryId("b".into()));
        assert_eq!(rows[1].id, EntryId("a".into()));
    }

    /// Re-opening moves a row to the top rather than adding a second one.
    #[test]
    fn v0_7_reopening_something_updates_its_row() {
        let f = Frecency::open(None).unwrap();
        let a = a_real_file("again");
        let id = EntryId("a".into());
        f.record_opened_at(&id, &a.to_string_lossy(), EntryKind::File, 100).unwrap();
        f.record_opened_at(&id, &a.to_string_lossy(), EntryKind::File, 300).unwrap();

        let rows = f.opened(&[], 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].opened_at, 300);
    }

    /// ADR-0013: a deleted file must not survive as a row. Checked on read,
    /// because a path can vanish between two openings of the Palette.
    #[test]
    fn v0_7_a_deleted_path_drops_out_of_the_opened_list() {
        let f = Frecency::open(None).unwrap();
        let gone = a_real_file("gone");
        f.record_opened_at(
            &EntryId("gone".into()),
            &gone.to_string_lossy(),
            EntryKind::File,
            100,
        )
        .unwrap();
        assert_eq!(f.opened(&[], 10).len(), 1);

        std::fs::remove_file(&gone).unwrap();
        assert!(f.opened(&[], 10).is_empty());
        // Still counted: the row is there, and the Settings confirmation names
        // what it will actually delete.
        assert_eq!(f.opened_count(), 1);
    }

    /// `!e` with no query shows files and folders, not the applications that
    /// share the table (task 10).
    #[test]
    fn v0_7_the_opened_list_filters_by_kind() {
        let f = Frecency::open(None).unwrap();
        let (file, app) = (a_real_file("doc"), a_real_file("app"));
        f.record_opened_at(&EntryId("f".into()), &file.to_string_lossy(), EntryKind::File, 100)
            .unwrap();
        f.record_opened_at(&EntryId("a".into()), &app.to_string_lossy(), EntryKind::App, 200)
            .unwrap();

        let files = f.opened(&[EntryKind::File, EntryKind::Folder], 10);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].id, EntryId("f".into()));
    }

    /// Capped, oldest evicted. Without it the list stops being recent and
    /// becomes a second, worse index.
    #[test]
    fn v0_7_the_opened_list_is_capped() {
        let f = Frecency::open(None).unwrap();
        let path = a_real_file("capped");
        for n in 0..(OPENED_CAP + 20) {
            f.record_opened_at(
                &EntryId(format!("id-{n}")),
                &path.to_string_lossy(),
                EntryKind::File,
                n,
            )
            .unwrap();
        }
        assert_eq!(f.opened_count(), OPENED_CAP as usize);
        // The survivors are the newest, not an arbitrary hundred.
        let rows = f.opened(&[], OPENED_CAP as usize);
        assert_eq!(rows[0].id, EntryId(format!("id-{}", OPENED_CAP + 19)));
    }

    /// A local history with no visible off switch is fine until somebody asks
    /// about it (TBC-0010). One call empties it.
    #[test]
    fn v0_7_the_opened_list_can_be_cleared_in_one_call() {
        let f = Frecency::open(None).unwrap();
        let path = a_real_file("clearable");
        f.record_opened_at(&EntryId("x".into()), &path.to_string_lossy(), EntryKind::File, 1)
            .unwrap();
        assert_eq!(f.clear_opened().unwrap(), 1);
        assert_eq!(f.opened_count(), 0);
    }

    /// The two tables answer different questions and must not disturb each
    /// other: clearing history is not forgetting what is used.
    #[test]
    fn v0_7_clearing_recents_leaves_frecency_alone() {
        let f = Frecency::open(None).unwrap();
        let path = a_real_file("both");
        let id = EntryId("shared".into());
        f.record_at(&id, EntryKind::File, 100).unwrap();
        f.record_opened_at(&id, &path.to_string_lossy(), EntryKind::File, 100).unwrap();

        f.clear_opened().unwrap();
        assert!(f.weight_at(&id, 100) > 0.0, "Frecency lost its row");
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
