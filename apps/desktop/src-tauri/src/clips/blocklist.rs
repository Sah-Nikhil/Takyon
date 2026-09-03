//! The capture blocklist — the `blocklist` table in `settings.db` (§4).
//!
//! Second of the two exclusion mechanisms ADR-0006 requires. The first,
//! `ExcludeClipboardContentFromMonitorProcessing`, is honoured in `watch.rs` and
//! is not enough on its own: an application only sets it if its authors thought
//! to. This is the user's answer for the ones that did not.
//!
//! Stored lowercased and compared by file name, so `notepad.exe` blocks it
//! wherever it was launched from. The editor is v0.6; until then a row is one
//! `INSERT` by hand, exactly like `aliases`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

use rusqlite::{Connection, Result};

/// The `blocklist` table, with the answer cached in memory.
///
/// Cached because it is read on the clipboard path, where a SQLite round trip per
/// copy buys nothing: the table is a handful of rows and only this process writes
/// it.
pub struct Blocklist {
    conn: Mutex<Connection>,
    blocked: RwLock<HashSet<String>>,
}

impl Blocklist {
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
            "CREATE TABLE IF NOT EXISTS blocklist (exe TEXT PRIMARY KEY)",
            [],
        )?;
        let list = Blocklist {
            conn: Mutex::new(conn),
            blocked: RwLock::new(HashSet::new()),
        };
        list.reload();
        Ok(list)
    }

    /// Whether a copy from this executable is captured.
    ///
    /// Takes a full path or a bare name; an unknown source is *not* blocked. That
    /// direction is deliberate: failing closed would silently stop capturing
    /// whenever the owning window could not be identified.
    pub fn blocks(&self, exe: Option<&str>) -> bool {
        let Some(exe) = exe else {
            return false;
        };
        let Ok(blocked) = self.blocked.read() else {
            return false;
        };
        blocked.contains(&file_name(exe))
    }

    pub fn add(&self, exe: &str) -> Result<()> {
        {
            let conn = self.conn.lock().expect("blocklist mutex");
            conn.execute(
                "INSERT OR IGNORE INTO blocklist (exe) VALUES (?1)",
                [file_name(exe)],
            )?;
        }
        self.reload();
        Ok(())
    }

    pub fn remove(&self, exe: &str) -> Result<()> {
        {
            let conn = self.conn.lock().expect("blocklist mutex");
            conn.execute("DELETE FROM blocklist WHERE exe = ?1", [file_name(exe)])?;
        }
        self.reload();
        Ok(())
    }

    /// Every blocked executable, sorted. What the v0.6 editor lists.
    pub fn all(&self) -> Vec<String> {
        let Ok(blocked) = self.blocked.read() else {
            return Vec::new();
        };
        let mut out: Vec<String> = blocked.iter().cloned().collect();
        out.sort();
        out
    }

    /// Re-read the table into the cache. Called after every write.
    fn reload(&self) {
        let Ok(conn) = self.conn.lock() else {
            return;
        };
        let Ok(mut stmt) = conn.prepare("SELECT exe FROM blocklist") else {
            return;
        };
        let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) else {
            return;
        };
        let fresh: HashSet<String> = rows.filter_map(|r| r.ok()).collect();
        if let Ok(mut blocked) = self.blocked.write() {
            *blocked = fresh;
        }
    }
}

/// Bare file name, lowercased. `C:\Windows\notepad.exe` and `NOTEPAD.EXE` are one
/// entry, or blocking an app depends on where the user copied its path from.
fn file_name(exe: &str) -> String {
    Path::new(exe.trim())
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| exe.trim().to_string())
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> Blocklist {
        Blocklist::open(None).expect("in-memory blocklist")
    }

    #[test]
    fn v0_5_a_blocked_exe_is_matched_by_name_wherever_it_lives() {
        let list = list();
        list.add("notepad.exe").unwrap();
        assert!(list.blocks(Some(r"C:\Windows\System32\notepad.exe")));
        assert!(list.blocks(Some(r"D:\portable\NOTEPAD.EXE")));
        assert!(!list.blocks(Some(r"C:\Windows\System32\notepad2.exe")));
    }

    /// Adding a full path must block the same executable as adding its name. The
    /// v0.6 editor will offer whatever the OS handed it.
    #[test]
    fn v0_5_adding_a_full_path_blocks_the_same_thing_as_adding_a_name() {
        let list = list();
        list.add(r"C:\Program Files\Bitwarden\Bitwarden.exe").unwrap();
        assert!(list.blocks(Some("bitwarden.exe")));
        assert_eq!(list.all(), vec!["bitwarden.exe"]);
    }

    /// Fails open, not closed. An unidentifiable window must not stop capture for
    /// everything — the exclusion format is what protects that case.
    #[test]
    fn v0_5_an_unknown_source_is_not_blocked() {
        let list = list();
        list.add("notepad.exe").unwrap();
        assert!(!list.blocks(None));
    }

    #[test]
    fn v0_5_removing_an_entry_lets_capture_resume() {
        let list = list();
        list.add("notepad.exe").unwrap();
        list.remove(r"C:\Windows\notepad.exe").unwrap();
        assert!(!list.blocks(Some("notepad.exe")));
        assert!(list.all().is_empty());
    }

    #[test]
    fn v0_5_adding_the_same_exe_twice_is_one_entry() {
        let list = list();
        list.add("code.exe").unwrap();
        list.add("Code.exe").unwrap();
        assert_eq!(list.all().len(), 1);
    }
}
