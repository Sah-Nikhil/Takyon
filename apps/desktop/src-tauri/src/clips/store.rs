//! `clips.db` — the clipboard history, encrypted per row (IMPLEMENTATION_PLAN §4).
//!
//! Content is AES-256-GCM with a fresh 12-byte nonce per row; the key is DPAPI
//! wrapped and lives in `key.rs`. `created_at`, `source_exe` and `len` stay
//! plaintext, which ADR-0008 accepts and explains.
//!
//! **Deleting is not the same as destroying.** SQLite leaves freed pages intact
//! and WAL keeps live content until checkpoint, so every path that removes rows
//! goes through [`ClipStore::purge`]: `PRAGMA secure_delete = ON` is set at open,
//! and the delete is followed by `wal_checkpoint(TRUNCATE)`.

use std::path::PathBuf;
use std::sync::Mutex;

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, Nonce};
use rusqlite::{params, Connection, OptionalExtension};

use super::key::{self, ClipKey, KEY_LEN};

/// How much of a clip the Palette ever draws. Longer content is stored whole and
/// truncated on the way out, so a 4 MB paste never crosses IPC to draw one row.
pub const PREVIEW_CHARS: usize = 160;

/// How many rows a `!v` search decrypts. Search has to decrypt to match, so this
/// is what bounds the work: newest first, because that is what anyone is looking
/// for. Nothing older is unreachable — it is reachable by being more specific.
pub const SEARCH_WINDOW: usize = 5_000;

/// How many rows the history surface asks for at once.
///
/// Bigger than MAX_ENTRIES because this is a scrolling pane rather than a
/// twelve-row list, and small enough that one page is a handful of decryptions.
pub const PAGE: usize = 200;

/// How long history is kept. Fixed list, per ADR-0006 — not a free-text duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retention {
    Forever,
    SixMonths,
    OneMonth,
    OneWeek,
    OneDay,
}

impl Retention {
    /// The stored spelling. Frozen: renaming one silently resets a saved setting.
    pub fn as_str(self) -> &'static str {
        match self {
            Retention::Forever => "forever",
            Retention::SixMonths => "6-months",
            Retention::OneMonth => "1-month",
            Retention::OneWeek => "1-week",
            Retention::OneDay => "1-day",
        }
    }

    /// Parse a stored spelling, falling back to the default rather than failing.
    /// An unreadable setting must not turn retention off.
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "forever" => Retention::Forever,
            "6-months" => Retention::SixMonths,
            "1-month" => Retention::OneMonth,
            "1-week" => Retention::OneWeek,
            "1-day" => Retention::OneDay,
            _ => Retention::default(),
        }
    }

    /// Seconds kept, or `None` for forever.
    pub fn seconds(self) -> Option<i64> {
        match self {
            Retention::Forever => None,
            Retention::SixMonths => Some(182 * 86_400),
            Retention::OneMonth => Some(30 * 86_400),
            Retention::OneWeek => Some(7 * 86_400),
            Retention::OneDay => Some(86_400),
        }
    }

    /// Every option, in the order Settings lists them (v0.6).
    pub fn all() -> [Retention; 5] {
        [
            Retention::Forever,
            Retention::SixMonths,
            Retention::OneMonth,
            Retention::OneWeek,
            Retention::OneDay,
        ]
    }
}

impl Default for Retention {
    /// A month. Long enough that history is useful, short enough that a password
    /// copied once does not sit in the file for years. The user can pick either
    /// extreme; this is only what happens before anyone chooses.
    fn default() -> Self {
        Retention::OneMonth
    }
}

/// What a clip holds. Text only at v0.5 — images and file lists arrive behind the
/// same rules, which is why this is a kind rather than an assumption.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipKind {
    Text,
}

impl ClipKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ClipKind::Text => "text",
        }
    }
}

/// One row as the UI needs it: metadata plus a truncated, decrypted preview.
///
/// Full content is fetched by id at paste time, never carried in a list — a
/// search response would otherwise ship every matching secret to the webview.
#[derive(Clone, Debug)]
pub struct Clip {
    pub id: i64,
    pub created_at: i64,
    pub kind: ClipKind,
    pub source_exe: Option<String>,
    pub len: usize,
    pub preview: String,
}

/// The clipboard history database.
///
/// One connection behind a `Mutex`, like `Frecency`: writes happen once per copy
/// and reads once per `!v` keystroke.
pub struct ClipStore {
    conn: Mutex<Connection>,
    cipher: Aes256Gcm,
}

impl ClipStore {
    /// Open `clips.db` in `dir`, or an in-memory database with an ephemeral key
    /// when `dir` is `None` — the seam the tests use, and the fallback when there
    /// is nowhere to write.
    pub fn open(dir: Option<PathBuf>) -> std::io::Result<Self> {
        let (conn, key) = match dir {
            Some(dir) => {
                std::fs::create_dir_all(&dir)?;
                let key = key::load_or_create(&dir)?;
                let conn = Connection::open(dir.join("clips.db")).map_err(sql_err)?;
                (conn, key)
            }
            None => (
                Connection::open_in_memory().map_err(sql_err)?,
                ClipKey::generate(),
            ),
        };
        Self::with_connection(conn, &key)
    }

    /// The half that needs no filesystem, so a test can hand in its own key.
    pub fn with_connection(conn: Connection, key: &ClipKey) -> std::io::Result<Self> {
        // Set every open, not once at creation: `secure_delete` is a connection
        // pragma, so a connection that forgets it leaves recoverable ciphertext in
        // freed pages for the whole session (ADR-0008).
        conn.pragma_update(None, "secure_delete", "ON")
            .map_err(sql_err)?;
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        conn.execute(
            "CREATE TABLE IF NOT EXISTS clips (
                 id         INTEGER PRIMARY KEY,
                 created_at INTEGER NOT NULL,
                 kind       TEXT    NOT NULL,
                 source_exe TEXT,
                 len        INTEGER NOT NULL,
                 nonce      BLOB    NOT NULL,
                 ciphertext BLOB    NOT NULL
             )",
            [],
        )
        .map_err(sql_err)?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS clips_created_at ON clips (created_at DESC)",
            [],
        )
        .map_err(sql_err)?;

        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key.bytes()));
        Ok(ClipStore {
            conn: Mutex::new(conn),
            cipher,
        })
    }

    /// Store one capture, returning its row id.
    ///
    /// A repeat of the newest clip moves that row's timestamp instead of adding a
    /// second one. Copying the same thing twice is routine and two identical rows
    /// are never what anyone wanted.
    pub fn insert_at(
        &self,
        kind: ClipKind,
        source_exe: Option<&str>,
        content: &str,
        now: i64,
    ) -> std::io::Result<i64> {
        if let Some(id) = self.touch_if_repeat(content, now) {
            return Ok(id);
        }

        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = self
            .cipher
            .encrypt(&nonce, content.as_bytes())
            .map_err(|_| std::io::Error::other("clipboard content could not be encrypted"))?;

        let conn = self.conn.lock().expect("clips mutex");
        conn.execute(
            "INSERT INTO clips (created_at, kind, source_exe, len, nonce, ciphertext)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                now,
                kind.as_str(),
                source_exe,
                content.chars().count() as i64,
                nonce.as_slice(),
                ciphertext
            ],
        )
        .map_err(sql_err)?;
        Ok(conn.last_insert_rowid())
    }

    pub fn insert(
        &self,
        kind: ClipKind,
        source_exe: Option<&str>,
        content: &str,
    ) -> std::io::Result<i64> {
        self.insert_at(kind, source_exe, content, unix_now())
    }

    /// The newest clips, most recent first.
    pub fn recent(&self, limit: usize) -> Vec<Clip> {
        self.rows(limit).into_iter().map(|(clip, _)| clip).collect()
    }

    /// Clips whose content contains `needle`, newest first.
    ///
    /// Matching is on the decrypted text, so it happens in Rust rather than in
    /// SQL — the column SQLite can see is ciphertext, and that is the point.
    pub fn search(&self, needle: &str, limit: usize) -> Vec<Clip> {
        let needle = needle.trim().to_lowercase();
        if needle.is_empty() {
            return self.recent(limit);
        }
        // Matched against the full decrypted text, not the preview: a needle past
        // PREVIEW_CHARS still has to find its clip.
        self.rows(SEARCH_WINDOW)
            .into_iter()
            .filter(|(_, text)| text.to_lowercase().contains(&needle))
            .map(|(clip, _)| clip)
            .take(limit)
            .collect()
    }

    /// The full text of one clip. What paste-back reads, and the only path that
    /// ever returns untruncated content.
    pub fn content(&self, id: i64) -> Option<String> {
        let conn = self.conn.lock().ok()?;
        let row: Option<(Vec<u8>, Vec<u8>)> = conn
            .query_row(
                "SELECT nonce, ciphertext FROM clips WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .ok()
            .flatten();
        let (nonce, ciphertext) = row?;
        self.decrypt(&nonce, &ciphertext)
    }

    /// How many rows are older than `cutoff`.
    ///
    /// Read before a retention change so the confirmation names the real number
    /// rather than "some items" (ROADMAP v0.6).
    pub fn count_older_than(&self, cutoff: i64) -> usize {
        let Ok(conn) = self.conn.lock() else {
            return 0;
        };
        conn.query_row(
            "SELECT COUNT(*) FROM clips WHERE created_at < ?1",
            [cutoff],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0) as usize
    }

    /// Delete everything older than the retention window. Returns rows removed.
    ///
    /// Runs at startup and on a timer. `Forever` deletes nothing and still
    /// returns, so the caller has no special case.
    pub fn sweep_at(&self, retention: Retention, now: i64) -> usize {
        let Some(seconds) = retention.seconds() else {
            return 0;
        };
        self.purge("DELETE FROM clips WHERE created_at < ?1", params![
            now - seconds
        ])
    }

    pub fn sweep(&self, retention: Retention) -> usize {
        self.sweep_at(retention, unix_now())
    }

    /// Remove one clip.
    pub fn delete(&self, id: i64) -> usize {
        self.purge("DELETE FROM clips WHERE id = ?1", params![id])
    }

    /// Remove everything.
    pub fn clear(&self) -> usize {
        self.purge("DELETE FROM clips", params![])
    }

    pub fn len(&self) -> usize {
        let Ok(conn) = self.conn.lock() else {
            return 0;
        };
        conn.query_row("SELECT COUNT(*) FROM clips", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0) as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The only path that removes rows.
    ///
    /// `secure_delete` zeroes the freed pages, and the checkpoint truncates the
    /// WAL that still holds the live copy. Skipping either leaves a deleted
    /// password readable in a hex editor (ADR-0008).
    fn purge(&self, sql: &str, args: &[&dyn rusqlite::ToSql]) -> usize {
        let Ok(conn) = self.conn.lock() else {
            return 0;
        };
        let removed = conn.execute(sql, args).unwrap_or(0);
        if removed > 0 {
            let _ = conn.pragma_update(None, "wal_checkpoint", "TRUNCATE");
        }
        removed
    }

    /// Bump the newest row's timestamp when the same text is copied again.
    fn touch_if_repeat(&self, content: &str, now: i64) -> Option<i64> {
        let newest = self.recent(1).into_iter().next()?;
        if self.content(newest.id)? != content {
            return None;
        }
        let conn = self.conn.lock().ok()?;
        conn.execute(
            "UPDATE clips SET created_at = ?1 WHERE id = ?2",
            params![now, newest.id],
        )
        .ok()?;
        Some(newest.id)
    }

    /// The newest `limit` rows, decrypted, each with its full text beside it.
    ///
    /// A row that will not decrypt is dropped rather than surfaced: it is either
    /// corrupt or from another key, and neither has anything to show.
    fn rows(&self, limit: usize) -> Vec<(Clip, String)> {
        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };
        let sql = "SELECT id, created_at, kind, source_exe, len, nonce, ciphertext
                   FROM clips ORDER BY created_at DESC, id DESC LIMIT ?1";
        let Ok(mut stmt) = conn.prepare(sql) else {
            return Vec::new();
        };
        let mapped = stmt.query_map([limit as i64], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Vec<u8>>(5)?,
                row.get::<_, Vec<u8>>(6)?,
            ))
        });
        let Ok(mapped) = mapped else {
            return Vec::new();
        };
        mapped
            .filter_map(|r| r.ok())
            .filter_map(|r| self.to_clip(r))
            .collect()
    }

    #[allow(clippy::type_complexity)]
    fn to_clip(
        &self,
        row: (i64, i64, String, Option<String>, i64, Vec<u8>, Vec<u8>),
    ) -> Option<(Clip, String)> {
        let (id, created_at, _kind, source_exe, len, nonce, ciphertext) = row;
        let text = self.decrypt(&nonce, &ciphertext)?;
        let clip = Clip {
            id,
            created_at,
            // One kind at v0.5. Read back and discarded rather than parsed, so
            // adding `image` is a match here and nothing else.
            kind: ClipKind::Text,
            source_exe,
            len: len as usize,
            preview: preview(&text),
        };
        Some((clip, text))
    }

    fn decrypt(&self, nonce: &[u8], ciphertext: &[u8]) -> Option<String> {
        let plain = self
            .cipher
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .ok()?;
        String::from_utf8(plain).ok()
    }
}

/// One line, capped. Newlines become spaces so a multi-line paste stays one row.
fn preview(text: &str) -> String {
    let flat = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>();
    let trimmed = flat.trim();
    if trimmed.chars().count() <= PREVIEW_CHARS {
        return trimmed.to_string();
    }
    trimmed.chars().take(PREVIEW_CHARS).collect()
}

fn sql_err(e: rusqlite::Error) -> std::io::Error {
    std::io::Error::other(format!("clips.db: {e}"))
}

/// Seconds since the epoch. Public so the retention commands can name the same
/// "now" the sweep uses.
pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The key length this file assumes, asserted rather than trusted.
const _: () = assert!(KEY_LEN == 32);

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> ClipStore {
        ClipStore::open(None).expect("in-memory store")
    }

    #[test]
    fn v0_5_a_clip_round_trips_through_encryption() {
        let s = store();
        let id = s.insert(ClipKind::Text, Some("notepad.exe"), "hello").unwrap();
        assert_eq!(s.content(id).as_deref(), Some("hello"));
        let clip = s.recent(1).remove(0);
        assert_eq!(clip.preview, "hello");
        assert_eq!(clip.source_exe.as_deref(), Some("notepad.exe"));
        assert_eq!(clip.len, 5);
    }

    /// ADR-0008's whole claim. The row SQLite holds must not be the text.
    #[test]
    fn v0_5_the_stored_row_does_not_contain_the_plaintext() {
        let s = store();
        s.insert(ClipKind::Text, None, "correct horse battery staple")
            .unwrap();
        let conn = s.conn.lock().unwrap();
        let ciphertext: Vec<u8> = conn
            .query_row("SELECT ciphertext FROM clips", [], |r| r.get(0))
            .unwrap();
        assert!(!contains(&ciphertext, b"correct horse"));
        assert!(!contains(&ciphertext, b"staple"));
    }

    /// Two identical clips must not share a nonce — GCM nonce reuse under one key
    /// leaks the plaintext XOR outright.
    #[test]
    fn v0_5_every_row_gets_its_own_nonce() {
        let s = store();
        s.insert(ClipKind::Text, None, "same").unwrap();
        s.insert(ClipKind::Text, Some("other.exe"), "different").unwrap();
        let conn = s.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT nonce FROM clips").unwrap();
        let nonces: Vec<Vec<u8>> = stmt
            .query_map([], |r| r.get::<_, Vec<u8>>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(nonces.len(), 2);
        assert_ne!(nonces[0], nonces[1]);
        assert!(nonces.iter().all(|n| n.len() == 12));
    }

    /// `secure_delete` is what makes a delete destroy rather than unlink. Off, and
    /// every retention sweep is a lie.
    #[test]
    fn v0_5_secure_delete_is_on_for_the_connection() {
        let s = store();
        let conn = s.conn.lock().unwrap();
        let on: i64 = conn
            .query_row("PRAGMA secure_delete", [], |r| r.get(0))
            .unwrap();
        assert_eq!(on, 1);
    }

    #[test]
    fn v0_5_copying_the_same_text_twice_moves_one_row_rather_than_adding_one() {
        let s = store();
        let first = s.insert_at(ClipKind::Text, None, "token", 1_000).unwrap();
        let again = s.insert_at(ClipKind::Text, None, "token", 2_000).unwrap();
        assert_eq!(first, again);
        assert_eq!(s.len(), 1);
        assert_eq!(s.recent(1)[0].created_at, 2_000);
    }

    /// Only a repeat of the *newest* clip collapses. Copying A, B, then A again is
    /// three events and A is genuinely back at the top.
    #[test]
    fn v0_5_a_repeat_of_an_older_clip_is_a_new_row() {
        let s = store();
        s.insert_at(ClipKind::Text, None, "a", 1_000).unwrap();
        s.insert_at(ClipKind::Text, None, "b", 2_000).unwrap();
        s.insert_at(ClipKind::Text, None, "a", 3_000).unwrap();
        assert_eq!(s.len(), 3);
    }

    #[test]
    fn v0_5_search_matches_decrypted_content_newest_first() {
        let s = store();
        s.insert_at(ClipKind::Text, None, "old meeting notes", 1_000).unwrap();
        s.insert_at(ClipKind::Text, None, "unrelated", 2_000).unwrap();
        s.insert_at(ClipKind::Text, None, "new MEETING agenda", 3_000).unwrap();

        let hits = s.search("meeting", 10);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].preview, "new MEETING agenda");
        assert_eq!(hits[1].preview, "old meeting notes");
        assert!(s.search("nothing here", 10).is_empty());
    }

    /// An empty `!v` query lists history rather than nothing — unlike Bangless,
    /// where empty means empty (ADR-0001). The Mode *is* the history view.
    #[test]
    fn v0_5_an_empty_search_lists_recent_clips() {
        let s = store();
        s.insert(ClipKind::Text, None, "a").unwrap();
        assert_eq!(s.search("   ", 10).len(), 1);
    }

    #[test]
    fn v0_5_the_sweep_deletes_older_than_the_window_and_keeps_the_rest() {
        let s = store();
        let now = 10_000_000;
        s.insert_at(ClipKind::Text, None, "ancient", now - 40 * 86_400).unwrap();
        s.insert_at(ClipKind::Text, None, "recent", now - 3 * 86_400).unwrap();

        assert_eq!(s.count_older_than(now - 30 * 86_400), 1);
        assert_eq!(s.sweep_at(Retention::OneMonth, now), 1);
        assert_eq!(s.len(), 1);
        assert_eq!(s.recent(1)[0].preview, "recent");
    }

    #[test]
    fn v0_5_forever_deletes_nothing() {
        let s = store();
        s.insert_at(ClipKind::Text, None, "kept", 0).unwrap();
        assert_eq!(s.sweep_at(Retention::Forever, 10_000_000), 0);
        assert_eq!(s.len(), 1);
    }

    /// The spellings are a stored setting. A rename here empties someone's choice
    /// back to the default without telling them.
    #[test]
    fn v0_5_retention_spellings_round_trip() {
        for r in Retention::all() {
            assert_eq!(Retention::parse(r.as_str()), r);
        }
        assert_eq!(Retention::parse("nonsense"), Retention::default());
        assert_eq!(Retention::parse("1-DAY"), Retention::OneDay);
    }

    #[test]
    fn v0_5_retention_windows_are_ordered_shortest_last() {
        let seconds: Vec<Option<i64>> = Retention::all().iter().map(|r| r.seconds()).collect();
        assert_eq!(seconds[0], None);
        for pair in seconds[1..].windows(2) {
            assert!(pair[0].unwrap() > pair[1].unwrap());
        }
    }

    #[test]
    fn v0_5_a_long_clip_is_stored_whole_and_previewed_short() {
        let s = store();
        let long = "x".repeat(PREVIEW_CHARS * 3);
        let id = s.insert(ClipKind::Text, None, &long).unwrap();
        assert_eq!(s.content(id).unwrap().len(), long.len());
        assert_eq!(s.recent(1)[0].preview.chars().count(), PREVIEW_CHARS);
        assert_eq!(s.recent(1)[0].len, long.len());
    }

    #[test]
    fn v0_5_a_multi_line_clip_previews_as_one_line() {
        let s = store();
        s.insert(ClipKind::Text, None, "  first\nsecond\ttab  ").unwrap();
        assert_eq!(s.recent(1)[0].preview, "first second tab");
    }

    #[test]
    fn v0_5_deleting_one_clip_leaves_the_others() {
        let s = store();
        let a = s.insert_at(ClipKind::Text, None, "a", 1).unwrap();
        s.insert_at(ClipKind::Text, None, "b", 2).unwrap();
        assert_eq!(s.delete(a), 1);
        assert_eq!(s.len(), 1);
        assert!(s.content(a).is_none());
        assert_eq!(s.clear(), 1);
        assert!(s.is_empty());
    }

    /// A different key must not read another key's rows. Guards against the
    /// obvious future bug: regenerating the key file and calling the result a fix.
    #[test]
    fn v0_5_content_written_under_one_key_is_unreadable_under_another() {
        let conn = Connection::open_in_memory().unwrap();
        let key = ClipKey::generate();
        let s = ClipStore::with_connection(conn, &key).unwrap();
        s.insert(ClipKind::Text, None, "secret").unwrap();

        let rows = {
            let conn = s.conn.lock().unwrap();
            conn.query_row("SELECT nonce, ciphertext FROM clips", [], |r| {
                Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?))
            })
            .unwrap()
        };
        let other = ClipStore::with_connection(Connection::open_in_memory().unwrap(), &ClipKey::generate()).unwrap();
        assert!(other.decrypt(&rows.0, &rows.1).is_none());
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }
}
