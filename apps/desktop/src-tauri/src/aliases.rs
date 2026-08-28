//! User-defined aliases: `ps` → Photoshop (IMPLEMENTATION_PLAN §4).
//!
//! Owns the `aliases` table in `settings.db`. That database gains `settings`,
//! `roots`, `exclusions` and `blocklist` at v0.6, each with its own module beside
//! this one — one file, several owners, which SQLite is fine with and which keeps
//! each table's rules next to the code that enforces them.
//!
//! The alias is stored lowercased because `Query::needle` is lowercased, and a
//! `ps` that only matched a capital `PS` would look broken rather than
//! case-sensitive. The target is an `EntryId`, so an alias follows ADR-0014's
//! durability rule and breaks in exactly the same cases Frecency does.

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::{params, Connection, Result};

use crate::entry::EntryId;

/// The `aliases` table.
pub struct AliasStore {
    conn: Mutex<Connection>,
}

impl AliasStore {
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
            "CREATE TABLE IF NOT EXISTS aliases (
                 alias  TEXT PRIMARY KEY,
                 target TEXT NOT NULL
             )",
            [],
        )?;
        Ok(AliasStore {
            conn: Mutex::new(conn),
        })
    }

    /// Point an alias at an Entry, replacing whatever it pointed at before.
    pub fn set(&self, alias: &str, target: &EntryId) -> Result<()> {
        let conn = self.conn.lock().expect("alias mutex");
        conn.execute(
            "INSERT INTO aliases (alias, target) VALUES (?1, ?2)
             ON CONFLICT(alias) DO UPDATE SET target = ?2",
            params![alias.trim().to_lowercase(), target.as_str()],
        )?;
        Ok(())
    }

    pub fn remove(&self, alias: &str) -> Result<()> {
        let conn = self.conn.lock().expect("alias mutex");
        conn.execute(
            "DELETE FROM aliases WHERE alias = ?1",
            [alias.trim().to_lowercase()],
        )?;
        Ok(())
    }

    /// Every alias, grouped by the Entry it points at.
    ///
    /// Grouped rather than flat because that is the shape the application list
    /// wants: one lookup per app when aliases are applied, not one per alias.
    pub fn by_target(&self) -> std::collections::HashMap<EntryId, Vec<String>> {
        let mut out: std::collections::HashMap<EntryId, Vec<String>> = Default::default();
        let Ok(conn) = self.conn.lock() else {
            return out;
        };
        let Ok(mut stmt) = conn.prepare("SELECT alias, target FROM aliases") else {
            return out;
        };
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        });
        if let Ok(rows) = rows {
            for (alias, target) in rows.flatten() {
                out.entry(EntryId(target)).or_default().push(alias);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> EntryId {
        EntryId(s.into())
    }

    #[test]
    fn v0_3_an_alias_round_trips_to_its_target() {
        let store = AliasStore::open(None).unwrap();
        let ps = id(r"c:\ps\photoshop.exe");
        store.set("ps", &ps).unwrap();

        let by_target = store.by_target();
        assert_eq!(by_target.get(&ps).map(|v| v.as_slice()), Some(&["ps".to_string()][..]));
    }

    /// The needle is lowercased before matching, so a stored `PS` would never
    /// fire. Normalised on the way in rather than on every read.
    #[test]
    fn v0_3_an_alias_is_stored_lowercased_and_trimmed() {
        let store = AliasStore::open(None).unwrap();
        let ps = id(r"c:\ps\photoshop.exe");
        store.set("  PS  ", &ps).unwrap();
        assert_eq!(store.by_target()[&ps], vec!["ps".to_string()]);
    }

    /// One alias, one target. Re-pointing it must replace rather than accumulate,
    /// or `ps` would silently mean two applications.
    #[test]
    fn v0_3_repointing_an_alias_replaces_it() {
        let store = AliasStore::open(None).unwrap();
        let (old, new) = (id(r"c:\a.exe"), id(r"c:\b.exe"));
        store.set("ps", &old).unwrap();
        store.set("ps", &new).unwrap();

        let by_target = store.by_target();
        assert!(!by_target.contains_key(&old));
        assert_eq!(by_target[&new], vec!["ps".to_string()]);
    }

    #[test]
    fn v0_3_an_alias_can_be_removed() {
        let store = AliasStore::open(None).unwrap();
        let ps = id(r"c:\ps\photoshop.exe");
        store.set("ps", &ps).unwrap();
        store.remove("PS").unwrap();
        assert!(store.by_target().is_empty());
    }

    /// One Entry may answer to several names.
    #[test]
    fn v0_3_one_entry_can_carry_several_aliases() {
        let store = AliasStore::open(None).unwrap();
        let ps = id(r"c:\ps\photoshop.exe");
        store.set("ps", &ps).unwrap();
        store.set("photo", &ps).unwrap();

        let mut names = store.by_target()[&ps].clone();
        names.sort();
        assert_eq!(names, vec!["photo".to_string(), "ps".to_string()]);
    }
}
