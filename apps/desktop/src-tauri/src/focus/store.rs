//! `focus.db`: sessions, their events, and custom Focus categories.
//!
//! A running session is the row with `ended_at IS NULL`, at most one. Startup
//! reads it and resumes enforcement; that row is what makes Strict mean
//! anything across a crash or reboot (ADR-0036).

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::categories::{Category, Item};
use super::session::{Clock, Strictness};

/// What one session blocks. Stored as JSON with `kind` reserved for the Allow
/// list (plan § Allow list).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockList {
    /// Focus category ids: built-in (`social`) or custom (`custom:3`).
    pub categories: Vec<String>,
    /// Item keys: `site:`, `exe:`, `package:`, `entry:`.
    pub items: Vec<String>,
}

/// A session as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub id: i64,
    pub goal: String,
    pub strictness: Strictness,
    pub block_list: BlockList,
    pub clock: Clock,
}

/// How a session ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    /// Ran its full length.
    Expired,
    /// Completed by the user before the clock ran out.
    Completed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    Blocked,
    Snoozed,
    Paused,
    Resumed,
    /// Could not be blocked (elevated, unsupported browser).
    Refused,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finished {
    pub session: Session,
    pub ended_at: u64,
    pub outcome: Outcome,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub session_id: i64,
    pub at: u64,
    pub kind: EventKind,
    pub target: Option<String>,
}

#[derive(Debug)]
pub enum StoreError {
    Db(rusqlite::Error),
    /// A session is already running; finish it first.
    AlreadyRunning,
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Db(e)
    }
}

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    /// Open `focus.db` in `dir`, or in memory when there is nowhere to write.
    pub fn open(dir: Option<PathBuf>) -> rusqlite::Result<Self> {
        let conn = match dir {
            Some(dir) => {
                std::fs::create_dir_all(&dir).ok();
                Connection::open(dir.join("focus.db"))?
            }
            None => Connection::open_in_memory()?,
        };
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "foreign_keys", "ON");
        conn.execute_batch(SCHEMA)?;
        Ok(Store {
            conn: Mutex::new(conn),
        })
    }

    fn lock(&self) -> rusqlite::Result<MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|_| rusqlite::Error::InvalidQuery)
    }

    /// Record a new running session. Refused while another runs.
    pub fn start(
        &self,
        goal: &str,
        strictness: Strictness,
        list: &BlockList,
        clock: &Clock,
    ) -> Result<i64, StoreError> {
        let conn = self.lock()?;
        let running: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE ended_at IS NULL",
            [],
            |r| r.get(0),
        )?;
        if running > 0 {
            return Err(StoreError::AlreadyRunning);
        }
        conn.execute(
            "INSERT INTO sessions (goal, strictness, block_list, started_at, planned_ms, clock)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                goal,
                json(&strictness),
                json(list),
                clock.started_at() as i64,
                clock.planned_ms() as i64,
                json(clock),
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Persist a changed clock: Pause, resume, sleep, Edit.
    pub fn save_clock(&self, id: i64, clock: &Clock) -> rusqlite::Result<()> {
        self.lock()?.execute(
            "UPDATE sessions SET clock = ?2, planned_ms = ?3 WHERE id = ?1",
            params![id, json(clock), clock.planned_ms() as i64],
        )?;
        Ok(())
    }

    pub fn finish(&self, id: i64, now: u64, outcome: Outcome) -> rusqlite::Result<()> {
        self.lock()?.execute(
            "UPDATE sessions SET ended_at = ?2, outcome = ?3 WHERE id = ?1",
            params![id, now as i64, json(&outcome)],
        )?;
        Ok(())
    }

    pub fn record(
        &self,
        id: i64,
        at: u64,
        kind: EventKind,
        target: Option<&str>,
    ) -> rusqlite::Result<()> {
        self.lock()?.execute(
            "INSERT INTO session_events (session_id, at, kind, target) VALUES (?1, ?2, ?3, ?4)",
            params![id, at as i64, json(&kind), target],
        )?;
        Ok(())
    }

    /// Finished sessions that ended at or after `since`, oldest first.
    pub fn history(&self, since: u64) -> Vec<Finished> {
        let Ok(conn) = self.lock() else {
            return Vec::new();
        };
        let rows = conn
            .prepare(&format!("{SELECT_SESSION}, ended_at, outcome FROM sessions WHERE ended_at >= ?1 ORDER BY ended_at"))
            .and_then(|mut stmt| {
                stmt.query_map([since as i64], |r| {
                    Ok((session_row(r)?, r.get::<_, i64>(5)?, r.get::<_, String>(6)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()
            });
        rows.unwrap_or_default()
            .into_iter()
            .filter_map(|(session, ended_at, outcome)| {
                Some(Finished {
                    session: session?,
                    ended_at: ended_at as u64,
                    outcome: serde_json::from_str(&outcome).ok()?,
                })
            })
            .collect()
    }

    /// Events at or after `since`, in order.
    pub fn events(&self, since: u64) -> Vec<Event> {
        let Ok(conn) = self.lock() else {
            return Vec::new();
        };
        let rows = conn
            .prepare("SELECT session_id, at, kind, target FROM session_events WHERE at >= ?1 ORDER BY at, rowid")
            .and_then(|mut stmt| {
                stmt.query_map([since as i64], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()
            });
        rows.unwrap_or_default()
            .into_iter()
            .filter_map(|(session_id, at, kind, target)| {
                Some(Event {
                    session_id,
                    at: at as u64,
                    kind: serde_json::from_str(&kind).ok()?,
                    target,
                })
            })
            .collect()
    }

    /// Delete finished sessions and their events. A running one stays.
    pub fn clear_history(&self) -> rusqlite::Result<()> {
        let conn = self.lock()?;
        conn.execute("DELETE FROM sessions WHERE ended_at IS NOT NULL", [])?;
        let _ = conn.pragma_update(None, "wal_checkpoint", "TRUNCATE");
        Ok(())
    }

    /// Custom Focus categories, oldest first.
    pub fn custom_categories(&self) -> Vec<Category> {
        let Ok(conn) = self.lock() else {
            return Vec::new();
        };
        let heads = conn
            .prepare("SELECT id, name, icon, copied_from FROM categories ORDER BY id")
            .and_then(|mut stmt| {
                stmt.query_map([], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })?
                .collect::<rusqlite::Result<Vec<(i64, String, String, Option<String>)>>>()
            })
            .unwrap_or_default();
        heads
            .into_iter()
            .map(|(id, name, icon, copied_from)| Category {
                id: format!("{CUSTOM}{id}"),
                name,
                icon,
                dynamic: None,
                copied_from,
                items: items(&conn, id),
            })
            .collect()
    }

    /// Create (empty `id`) or replace a custom category; items replaced whole.
    pub fn save_category(&self, category: &Category) -> rusqlite::Result<Category> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        let id = match category
            .id
            .strip_prefix(CUSTOM)
            .and_then(|n| n.parse::<i64>().ok())
        {
            Some(id) => {
                tx.execute(
                    "UPDATE categories SET name = ?2, icon = ?3, copied_from = ?4 WHERE id = ?1",
                    params![id, category.name, category.icon, category.copied_from],
                )?;
                tx.execute("DELETE FROM category_items WHERE category_id = ?1", [id])?;
                id
            }
            None => {
                tx.execute(
                    "INSERT INTO categories (name, icon, copied_from) VALUES (?1, ?2, ?3)",
                    params![category.name, category.icon, category.copied_from],
                )?;
                tx.last_insert_rowid()
            }
        };
        for item in &category.items {
            tx.execute(
                "INSERT OR REPLACE INTO category_items (category_id, key, label) VALUES (?1, ?2, ?3)",
                params![id, item.key, item.label],
            )?;
        }
        tx.commit()?;
        Ok(Category {
            id: format!("{CUSTOM}{id}"),
            dynamic: None,
            ..category.clone()
        })
    }

    pub fn delete_category(&self, id: &str) -> rusqlite::Result<()> {
        if let Some(n) = id.strip_prefix(CUSTOM).and_then(|n| n.parse::<i64>().ok()) {
            self.lock()?
                .execute("DELETE FROM categories WHERE id = ?1", [n])?;
        }
        Ok(())
    }

    /// The session with no end, if any.
    pub fn running(&self) -> Option<Session> {
        let conn = self.lock().ok()?;
        conn.query_row(
            &format!("{SELECT_SESSION} FROM sessions WHERE ended_at IS NULL"),
            [],
            session_row,
        )
        .ok()
        .flatten()
    }
}

/// Prefix that marks a Focus category id as custom.
pub const CUSTOM: &str = "custom:";

/// A custom category's items, in insertion order.
fn items(conn: &Connection, id: i64) -> Vec<Item> {
    conn.prepare("SELECT key, label FROM category_items WHERE category_id = ?1 ORDER BY rowid")
        .and_then(|mut stmt| {
            stmt.query_map([id], |r| {
                Ok(Item {
                    key: r.get(0)?,
                    label: r.get(1)?,
                })
            })?
            .collect()
        })
        .unwrap_or_default()
}

const SELECT_SESSION: &str = "SELECT id, goal, strictness, block_list, clock";

/// A [`SELECT_SESSION`] row. Inner None = a row this build cannot read.
fn session_row(r: &rusqlite::Row) -> rusqlite::Result<Option<Session>> {
    let text = |i: usize| r.get::<_, String>(i);
    let (strictness, list, clock) = (text(2)?, text(3)?, text(4)?);
    Ok((|| {
        Some(Session {
            id: r.get(0).ok()?,
            goal: text(1).ok()?,
            strictness: serde_json::from_str(&strictness).ok()?,
            block_list: serde_json::from_str(&list).ok()?,
            clock: serde_json::from_str(&clock).ok()?,
        })
    })())
}

fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// IMPLEMENTATION_PLAN §4. `clock` is the whole [`Clock`] as JSON;
/// `started_at` and `planned_ms` are copied out for the stats queries.
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sessions (
  id          INTEGER PRIMARY KEY,
  goal        TEXT    NOT NULL,
  strictness  TEXT    NOT NULL,
  block_list  TEXT    NOT NULL,
  started_at  INTEGER NOT NULL,
  planned_ms  INTEGER NOT NULL,
  clock       TEXT    NOT NULL,
  ended_at    INTEGER,
  outcome     TEXT
);
CREATE INDEX IF NOT EXISTS sessions_running ON sessions (ended_at);
CREATE TABLE IF NOT EXISTS session_events (
  session_id  INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  at          INTEGER NOT NULL,
  kind        TEXT    NOT NULL,
  target      TEXT
);
CREATE TABLE IF NOT EXISTS categories (
  id          INTEGER PRIMARY KEY,
  name        TEXT    NOT NULL,
  icon        TEXT    NOT NULL,
  copied_from TEXT
);
CREATE TABLE IF NOT EXISTS category_items (
  category_id INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
  key         TEXT    NOT NULL,
  label       TEXT    NOT NULL,
  PRIMARY KEY (category_id, key)
);
";
