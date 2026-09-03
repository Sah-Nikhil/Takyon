//! The `settings` table in `settings.db` (IMPLEMENTATION_PLAN §4).
//!
//! Opened at v0.5 for one key, `clips.retention`, and it could not wait for
//! v0.6: the retention sweep runs at startup, before any window exists to push a
//! preference in. A default held only in the frontend would mean every launch
//! sweeping with the default and deleting history the user chose to keep.
//!
//! Values are strings, typed by the caller. JSON arrives with v0.6's settings
//! window, which is where a value stops being one word.

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension, Result};

/// How long clipboard history is kept. `store::Retention` spells the values.
pub const CLIPS_RETENTION: &str = "clips.retention";

/// Whether `!v` reaches clipboard history. `"1"` or `"0"`.
///
/// The Bang is a shortcut, not the door: the **Clipboard History** command is
/// always in the Bangless list, so turning this off hides an accelerator rather
/// than a feature. Default on.
pub const CLIPS_BANG: &str = "clips.bang";

/// The `settings` table.
pub struct Prefs {
    conn: Mutex<Connection>,
}

impl Prefs {
    /// Open `settings.db` in `dir`, or in memory when there is nowhere to write.
    pub fn open(dir: Option<PathBuf>) -> Result<Self> {
        let conn = match dir {
            Some(dir) => {
                std::fs::create_dir_all(&dir).ok();
                Connection::open(dir.join("settings.db"))?
            }
            None => Connection::open_in_memory()?,
        };
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                 key   TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             )",
            [],
        )?;
        Ok(Prefs {
            conn: Mutex::new(conn),
        })
    }

    /// The stored value, or `None` if nothing has been written.
    pub fn get(&self, key: &str) -> Option<String> {
        let conn = self.conn.lock().ok()?;
        conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get::<_, String>(0)
        })
        .optional()
        .ok()
        .flatten()
    }

    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().expect("prefs mutex");
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, value],
        )?;
        Ok(())
    }
}

/// Read a boolean preference, defaulting when unset or unparseable.
///
/// Unparseable defaults rather than failing: a hand-edited `settings.db` should
/// not be able to turn a feature off by holding a typo.
pub fn flag(prefs: &Prefs, key: &str, default: bool) -> bool {
    match prefs.get(key).as_deref().map(str::trim) {
        Some("1") | Some("true") | Some("on") => true,
        Some("0") | Some("false") | Some("off") => false,
        _ => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_5_a_written_value_reads_back() {
        let p = Prefs::open(None).unwrap();
        assert!(p.get(CLIPS_RETENTION).is_none());
        p.set(CLIPS_RETENTION, "forever").unwrap();
        assert_eq!(p.get(CLIPS_RETENTION).as_deref(), Some("forever"));
        p.set(CLIPS_RETENTION, "1-day").unwrap();
        assert_eq!(p.get(CLIPS_RETENTION).as_deref(), Some("1-day"));
    }

    /// The startup sweep reads this before anything can write it. An absent key
    /// must be absent, never an empty string that parses back to a short window.
    #[test]
    fn v0_5_an_unset_key_is_none_rather_than_empty() {
        let p = Prefs::open(None).unwrap();
        assert_eq!(p.get("nothing.here"), None);
    }

    #[test]
    fn v0_5_a_flag_defaults_when_unset_or_unreadable() {
        let p = Prefs::open(None).unwrap();
        assert!(flag(&p, CLIPS_BANG, true));
        assert!(!flag(&p, CLIPS_BANG, false));

        p.set(CLIPS_BANG, "0").unwrap();
        assert!(!flag(&p, CLIPS_BANG, true));
        p.set(CLIPS_BANG, "1").unwrap();
        assert!(flag(&p, CLIPS_BANG, false));

        // A typo must not silently turn something off.
        p.set(CLIPS_BANG, "yes please").unwrap();
        assert!(flag(&p, CLIPS_BANG, true));
    }
}
