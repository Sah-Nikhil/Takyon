//! `clips.db` — the clipboard history, encrypted per row (IMPLEMENTATION_PLAN §4).
//!
//! Content is AES-256-GCM with a fresh 12-byte nonce per row; the key is DPAPI
//! wrapped and lives in `key.rs`. `created_at`, `source_exe` and `len` stay
//! plaintext, which ADR-0008 accepts and explains.
//!
//! **Deleting is not the same as destroying.** SQLite leaves freed pages intact
//! and WAL keeps live content until checkpoint, so every path that removes rows
//! goes through [`ClipStore::purge`]: `PRAGMA secure_delete = ON` is set at open,
//! and the delete is followed by `wal_checkpoint(TRUNCATE)`. Image rows (v0.14)
//! keep an encrypted thumbnail in `ciphertext` and the full PNG in a blob file
//! (`blobs.rs`), which `purge` destroys with the row.

use std::path::PathBuf;
use std::sync::Mutex;

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, Nonce};
use rusqlite::{params, Connection, OptionalExtension};

use super::blobs::Blobs;
use super::image::{self, Rgba};
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

/// What a clip holds. Text since v0.5, images since v0.14; file lists are the
/// kind still to come (`docs/plans/v0.14-clipboard-kinds.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipKind {
    Text,
    Image,
}

impl ClipKind {
    /// The stored spelling. Frozen, like `Retention::as_str`.
    pub fn as_str(self) -> &'static str {
        match self {
            ClipKind::Text => "text",
            ClipKind::Image => "image",
        }
    }

    /// Unknown spellings are `None`, and the row is skipped rather than drawn
    /// as a kind it is not.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "text" => Some(ClipKind::Text),
            "image" => Some(ClipKind::Image),
            _ => None,
        }
    }
}

/// A clip's full content, as paste-back needs it.
pub enum Content {
    Text(String),
    /// PNG bytes.
    Image(Vec<u8>),
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
    /// Characters for text, bytes of the stored PNG for an image.
    pub len: usize,
    pub preview: String,
    /// Pixel size, images only.
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// The clipboard history database.
///
/// One connection behind a `Mutex`, like `Frecency`: writes happen once per copy
/// and reads once per `!v` keystroke.
pub struct ClipStore {
    conn: Mutex<Connection>,
    cipher: Aes256Gcm,
    blobs: Blobs,
    /// Held from a blob's write to its row's insert, and through `reconcile`.
    /// Without it `clear` can see a fresh blob as an orphan and shred it.
    publish: Mutex<()>,
    /// The data directory, for [`ClipStore::disk_bytes`]. `None` in memory.
    dir: Option<PathBuf>,
}

impl ClipStore {
    /// Open `clips.db` in `dir`, or an in-memory database with an ephemeral key
    /// when `dir` is `None` — the seam the tests use, and the fallback when there
    /// is nowhere to write.
    pub fn open(dir: Option<PathBuf>) -> std::io::Result<Self> {
        let Some(dir) = dir else {
            let conn = Connection::open_in_memory().map_err(sql_err)?;
            return Self::with_connection(conn, &ClipKey::generate());
        };
        std::fs::create_dir_all(&dir)?;
        let key = key::load_or_create(&dir)?;
        let conn = Connection::open(dir.join("clips.db")).map_err(sql_err)?;
        let store = Self::build(conn, &key, Blobs::disk(&dir)?, Some(dir))?;
        // Once, before the watcher can write: a crash between a blob and its row
        // leaves one without the other, and both halves are reconciled here.
        store.blobs.sweep_partial();
        store.reconcile();
        Ok(store)
    }

    /// The half that needs no filesystem, so a test can hand in its own key.
    pub fn with_connection(conn: Connection, key: &ClipKey) -> std::io::Result<Self> {
        Self::build(conn, key, Blobs::memory(), None)
    }

    fn build(
        conn: Connection,
        key: &ClipKey,
        blobs: Blobs,
        dir: Option<PathBuf>,
    ) -> std::io::Result<Self> {
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
        migrate(&conn)?;

        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key.bytes()));
        Ok(ClipStore {
            conn: Mutex::new(conn),
            cipher,
            blobs,
            publish: Mutex::new(()),
            dir,
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

        let (nonce, ciphertext) = self.seal(content.as_bytes())?;

        let conn = self.conn.lock().expect("clips mutex");
        conn.execute(
            "INSERT INTO clips (created_at, kind, source_exe, len, nonce, ciphertext)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                now,
                kind.as_str(),
                source_exe,
                content.chars().count() as i64,
                nonce,
                ciphertext
            ],
        )
        .map_err(sql_err)?;
        Ok(conn.last_insert_rowid())
    }

    /// Store one image capture, returning its row id.
    ///
    /// Full PNG sealed into a blob file, thumbnail sealed into the row. Over
    /// [`image::MAX_PNG_BYTES`] is refused whole. A repeat of the newest image
    /// moves its timestamp, as text does.
    pub fn insert_image_at(
        &self,
        source_exe: Option<&str>,
        pixels: &Rgba,
        now: i64,
    ) -> std::io::Result<i64> {
        let png = image::encode_png(pixels)
            .ok_or_else(|| std::io::Error::other("image could not be encoded"))?;
        if png.len() > image::MAX_PNG_BYTES {
            return Err(std::io::Error::other(format!(
                "image is {} bytes, over the {} cap; skipped",
                png.len(),
                image::MAX_PNG_BYTES
            )));
        }
        if let Some(id) = self.touch_if_repeat_image(&png, pixels, now) {
            return Ok(id);
        }
        let thumb = image::encode_png(&image::thumbnail(pixels, image::THUMB_EDGE))
            .ok_or_else(|| std::io::Error::other("thumbnail could not be encoded"))?;

        let (blob_nonce, blob_ct) = self.seal(&png)?;
        let (nonce, ciphertext) = self.seal(&thumb)?;
        let _publish = self.publish.lock().unwrap_or_else(|e| e.into_inner());
        let name = self.blobs.write(&[blob_nonce, blob_ct].concat())?;

        let conn = self.conn.lock().expect("clips mutex");
        let inserted = conn.execute(
            "INSERT INTO clips (created_at, kind, source_exe, len, nonce, ciphertext,
                                blob, width, height)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                now,
                ClipKind::Image.as_str(),
                source_exe,
                png.len() as i64,
                nonce,
                ciphertext,
                name,
                pixels.width,
                pixels.height
            ],
        );
        if let Err(e) = inserted {
            // No row will ever name it, so it goes now rather than at next start.
            self.blobs.destroy(&name);
            return Err(sql_err(e));
        }
        Ok(conn.last_insert_rowid())
    }

    pub fn insert_image(&self, source_exe: Option<&str>, pixels: &Rgba) -> std::io::Result<i64> {
        self.insert_image_at(source_exe, pixels, unix_now())
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

    /// The full text of a text clip. What paste-back reads, and the only path
    /// that ever returns untruncated text. `None` for an image.
    pub fn content(&self, id: i64) -> Option<String> {
        let (nonce, ciphertext) = self.sealed_row(id, ClipKind::Text)?;
        self.decrypt(&nonce, &ciphertext)
    }

    /// Full content of any kind, for paste-back and Copy.
    pub fn fetch(&self, id: i64) -> Option<Content> {
        match self.content(id) {
            Some(text) => Some(Content::Text(text)),
            None => self.image(id).map(Content::Image),
        }
    }

    /// The full PNG of an image clip, decrypted from its blob. Never cached.
    pub fn image(&self, id: i64) -> Option<Vec<u8>> {
        let name: String = {
            let conn = self.conn.lock().ok()?;
            conn.query_row(
                "SELECT blob FROM clips WHERE id = ?1 AND kind = 'image'",
                [id],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten()?
        };
        self.open_blob(&name)
    }

    /// The thumbnail PNG of an image clip, from its row.
    pub fn thumbnail(&self, id: i64) -> Option<Vec<u8>> {
        let (nonce, ciphertext) = self.sealed_row(id, ClipKind::Image)?;
        self.decrypt_bytes(&nonce, &ciphertext)
    }

    /// What history occupies on disk: `clips.db`, its sidecars and every blob.
    /// Zero in memory. Settings shows it so pruning has a number behind it.
    pub fn disk_bytes(&self) -> u64 {
        let Some(dir) = &self.dir else {
            return 0;
        };
        let db: u64 = ["clips.db", "clips.db-wal", "clips.db-shm"]
            .iter()
            .filter_map(|n| std::fs::metadata(dir.join(n)).ok())
            .map(|m| m.len())
            .sum();
        db + self.blobs.bytes()
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
        self.purge("created_at < ?1", params![now - seconds])
    }

    pub fn sweep(&self, retention: Retention) -> usize {
        self.sweep_at(retention, unix_now())
    }

    /// Remove one clip.
    pub fn delete(&self, id: i64) -> usize {
        self.purge("id = ?1", params![id])
    }

    /// Remove everything, then any blob no row named.
    pub fn clear(&self) -> usize {
        let removed = self.purge("1", params![]);
        self.reconcile();
        removed
    }

    /// Make rows and blob files agree: shred files no row names, delete rows
    /// whose file is gone. Startup and `clear`; a crash between the two writes
    /// is the only way they disagree. Either inventory failing stops it: read
    /// as empty, it would delete everything.
    pub fn reconcile(&self) {
        let _publish = self.publish.lock().unwrap_or_else(|e| e.into_inner());
        let named: rusqlite::Result<Vec<String>> = match self.conn.lock() {
            Ok(conn) => conn
                .prepare("SELECT blob FROM clips WHERE blob IS NOT NULL")
                .and_then(|mut s| s.query_map([], |r| r.get::<_, String>(0))?.collect()),
            Err(_) => return,
        };
        let (named, present) = match (named, self.blobs.names()) {
            (Ok(named), Ok(present)) => (named, present),
            (Err(e), _) => return eprintln!("[takyon] clip reconcile skipped, rows: {e}"),
            (_, Err(e)) => return eprintln!("[takyon] clip reconcile skipped, files: {e}"),
        };
        for name in present.iter().filter(|n| !named.contains(n)) {
            self.blobs.destroy(name);
        }
        for name in named.iter().filter(|n| !present.contains(n)) {
            self.purge("blob = ?1", params![name]);
        }
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

    /// The only path that removes rows. `filter` is a `WHERE` clause.
    ///
    /// `secure_delete` zeroes freed pages, the checkpoint truncates the WAL that
    /// holds the live copy, and each row's blob is shredded after. Skipping any
    /// leaves a deleted secret readable in a hex editor (ADR-0008).
    fn purge(&self, filter: &str, args: &[&dyn rusqlite::ToSql]) -> usize {
        let Ok(conn) = self.conn.lock() else {
            return 0;
        };
        let blobs: Vec<String> = conn
            .prepare(&format!(
                "SELECT blob FROM clips WHERE ({filter}) AND blob IS NOT NULL"
            ))
            .and_then(|mut s| s.query_map(args, |r| r.get::<_, String>(0))?.collect())
            .unwrap_or_default();
        let removed = conn
            .execute(&format!("DELETE FROM clips WHERE {filter}"), args)
            .unwrap_or(0);
        if removed > 0 {
            let _ = conn.pragma_update(None, "wal_checkpoint", "TRUNCATE");
        }
        // Rows first: a crash here leaves an orphan file the startup sweep
        // shreds, never a row pointing at nothing.
        for name in &blobs {
            self.blobs.destroy(name);
        }
        removed
    }

    /// The `nonce, ciphertext` pair of row `id`, only if it is of `kind`.
    fn sealed_row(&self, id: i64, kind: ClipKind) -> Option<(Vec<u8>, Vec<u8>)> {
        let conn = self.conn.lock().ok()?;
        conn.query_row(
            "SELECT nonce, ciphertext FROM clips WHERE id = ?1 AND kind = ?2",
            params![id, kind.as_str()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .ok()
        .flatten()
    }

    /// Bump the newest row's timestamp when the same image is copied again.
    ///
    /// Compared on the full PNG, never the thumbnail: two screenshots differing
    /// by a clock digit share a thumbnail, and merging them loses the second.
    fn touch_if_repeat_image(&self, png: &[u8], pixels: &Rgba, now: i64) -> Option<i64> {
        let newest = self.recent(1).into_iter().next()?;
        let same_shape = newest.kind == ClipKind::Image
            && newest.width == Some(pixels.width)
            && newest.height == Some(pixels.height)
            && newest.len == png.len();
        if !same_shape || self.image(newest.id)? != png {
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

    /// Fresh nonce, then AES-GCM. Returns `(nonce, ciphertext)`.
    fn seal(&self, plain: &[u8]) -> std::io::Result<(Vec<u8>, Vec<u8>)> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plain)
            .map_err(|_| std::io::Error::other("clipboard content could not be encrypted"))?;
        Ok((nonce.to_vec(), ciphertext))
    }

    /// A blob file is `nonce || ciphertext`.
    fn open_blob(&self, name: &str) -> Option<Vec<u8>> {
        let sealed = self.blobs.read(name)?;
        if sealed.len() < 12 {
            return None;
        }
        let (nonce, ciphertext) = sealed.split_at(12);
        self.decrypt_bytes(nonce, ciphertext)
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
        let sql = "SELECT id, created_at, kind, source_exe, len, nonce, ciphertext,
                          width, height
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
                row.get::<_, Option<u32>>(7)?,
                row.get::<_, Option<u32>>(8)?,
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

    /// An image row's text is empty: nothing to match, and its thumbnail is not
    /// decrypted for a list that only needs its size.
    #[allow(clippy::type_complexity)]
    fn to_clip(
        &self,
        row: (
            i64,
            i64,
            String,
            Option<String>,
            i64,
            Vec<u8>,
            Vec<u8>,
            Option<u32>,
            Option<u32>,
        ),
    ) -> Option<(Clip, String)> {
        let (id, created_at, kind, source_exe, len, nonce, ciphertext, width, height) = row;
        let kind = ClipKind::parse(&kind)?;
        let (text, preview) = match kind {
            ClipKind::Text => {
                let text = self.decrypt(&nonce, &ciphertext)?;
                let shown = preview(&text);
                (text, shown)
            }
            ClipKind::Image => (
                String::new(),
                format!("Image {} \u{d7} {}", width?, height?),
            ),
        };
        let clip = Clip {
            id,
            created_at,
            kind,
            source_exe,
            len: len as usize,
            preview,
            width,
            height,
        };
        Some((clip, text))
    }

    fn decrypt(&self, nonce: &[u8], ciphertext: &[u8]) -> Option<String> {
        String::from_utf8(self.decrypt_bytes(nonce, ciphertext)?).ok()
    }

    fn decrypt_bytes(&self, nonce: &[u8], ciphertext: &[u8]) -> Option<Vec<u8>> {
        if nonce.len() != 12 {
            return None;
        }
        self.cipher
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .ok()
    }
}

/// v0.14's columns, added in place. `ADD COLUMN` is cheap and keeps every
/// existing row, which are all text and need none of them.
fn migrate(conn: &Connection) -> std::io::Result<()> {
    let have: Vec<String> = conn
        .prepare("PRAGMA table_info(clips)")
        .and_then(|mut s| s.query_map([], |r| r.get::<_, String>(1))?.collect())
        .map_err(sql_err)?;
    for (name, ty) in [
        ("blob", "TEXT"),
        ("width", "INTEGER"),
        ("height", "INTEGER"),
    ] {
        if !have.iter().any(|c| c == name) {
            conn.execute(&format!("ALTER TABLE clips ADD COLUMN {name} {ty}"), [])
                .map_err(sql_err)?;
        }
    }
    Ok(())
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
        let id = s
            .insert(ClipKind::Text, Some("notepad.exe"), "hello")
            .unwrap();
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
        s.insert(ClipKind::Text, Some("other.exe"), "different")
            .unwrap();
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
        s.insert_at(ClipKind::Text, None, "old meeting notes", 1_000)
            .unwrap();
        s.insert_at(ClipKind::Text, None, "unrelated", 2_000)
            .unwrap();
        s.insert_at(ClipKind::Text, None, "new MEETING agenda", 3_000)
            .unwrap();

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
        s.insert_at(ClipKind::Text, None, "ancient", now - 40 * 86_400)
            .unwrap();
        s.insert_at(ClipKind::Text, None, "recent", now - 3 * 86_400)
            .unwrap();

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
        s.insert(ClipKind::Text, None, "  first\nsecond\ttab  ")
            .unwrap();
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
        let other =
            ClipStore::with_connection(Connection::open_in_memory().unwrap(), &ClipKey::generate())
                .unwrap();
        assert!(other.decrypt(&rows.0, &rows.1).is_none());
    }

    fn pixels(width: u32, height: u32, seed: u8) -> Rgba {
        let pixels = (0..width * height)
            .flat_map(|i| [seed, (i % 251) as u8, (i / 7 % 251) as u8, 255])
            .collect();
        Rgba {
            width,
            height,
            pixels,
        }
    }

    #[test]
    fn v0_14_an_image_round_trips_with_a_thumbnail_and_a_size() {
        let s = store();
        let shot = pixels(600, 300, 1);
        let id = s.insert_image(Some("SnippingTool.exe"), &shot).unwrap();

        let clip = s.recent(1).remove(0);
        assert_eq!(clip.kind, ClipKind::Image);
        assert_eq!((clip.width, clip.height), (Some(600), Some(300)));
        assert_eq!(clip.preview, "Image 600 \u{d7} 300");
        let full = image::decode_png(&s.image(id).unwrap()).unwrap();
        assert_eq!(full, shot);
        let thumb = image::decode_png(&s.thumbnail(id).unwrap()).unwrap();
        assert_eq!((thumb.width, thumb.height), (256, 128));
        assert!(s.content(id).is_none(), "an image has no text");
        assert!(matches!(s.fetch(id), Some(Content::Image(_))));
    }

    /// Neither half of an image is plaintext: not the row, not the blob.
    #[test]
    fn v0_14_neither_the_row_nor_the_blob_holds_the_png() {
        let s = store();
        let id = s.insert_image(None, &pixels(40, 40, 9)).unwrap();
        let png = s.image(id).unwrap();
        let sealed = match &s.blobs {
            Blobs::Memory(map) => map.lock().unwrap().values().next().unwrap().clone(),
            Blobs::Disk(_) => unreachable!(),
        };
        assert!(
            !contains(&sealed, &png[..16]),
            "the blob is the PNG in clear"
        );
        let row: Vec<u8> = s
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT ciphertext FROM clips", [], |r| r.get(0))
            .unwrap();
        assert!(
            !contains(&row, b"IHDR"),
            "the thumbnail is the PNG in clear"
        );
    }

    /// Every destroying path takes the blob with it, or the directory grows
    /// forever while `clips.db` shrinks.
    #[test]
    fn v0_14_delete_sweep_and_clear_each_destroy_the_blob() {
        let s = store();
        let a = s.insert_image_at(None, &pixels(8, 8, 1), 1).unwrap();
        s.insert_image_at(None, &pixels(8, 8, 2), 10_000_000)
            .unwrap();
        s.insert_image_at(None, &pixels(8, 8, 3), 10_000_001)
            .unwrap();
        assert_eq!(s.blobs.names().unwrap().len(), 3);

        assert_eq!(s.delete(a), 1);
        assert_eq!(s.blobs.names().unwrap().len(), 2);
        assert_eq!(s.sweep_at(Retention::OneDay, 10_000_002 + 86_400), 2);
        assert!(s.blobs.names().unwrap().is_empty());

        s.insert_image(None, &pixels(8, 8, 4)).unwrap();
        s.insert(ClipKind::Text, None, "text too").unwrap();
        assert_eq!(s.clear(), 2);
        assert!(s.blobs.names().unwrap().is_empty());
    }

    #[test]
    fn v0_14_reconcile_removes_orphan_files_and_rows_with_no_file() {
        let s = store();
        let id = s.insert_image(None, &pixels(4, 4, 1)).unwrap();
        let orphan = s.blobs.write(b"no row names this").unwrap();
        s.reconcile();
        assert!(s.blobs.read(&orphan).is_none());
        assert_eq!(s.len(), 1);

        for name in s.blobs.names().unwrap() {
            s.blobs.destroy(&name);
        }
        s.reconcile();
        assert!(s.image(id).is_none());
        assert!(s.is_empty(), "a row whose blob is gone is gone too");
    }

    /// An unreadable blob directory reads as an error, not as "no files", or
    /// reconcile would delete every image row (review finding, v0.14).
    #[test]
    fn v0_14_reconcile_does_nothing_when_the_files_cannot_be_listed() {
        // A file where the directory should be: `read_dir` fails on it every
        // time, whatever else is in %TEMP%.
        let not_a_dir =
            std::env::temp_dir().join(format!("takyon-clips-file-{}", std::process::id()));
        std::fs::write(&not_a_dir, b"").unwrap();
        let s = ClipStore::build(
            Connection::open_in_memory().unwrap(),
            &ClipKey::generate(),
            Blobs::Disk(not_a_dir.clone()),
            None,
        )
        .unwrap();
        s.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO clips (created_at, kind, len, nonce, ciphertext, blob, width, height)
                 VALUES (1, 'image', 1, x'00', x'00', ?1, 1, 1)",
                ["0".repeat(32)],
            )
            .unwrap();
        s.reconcile();
        assert_eq!(s.len(), 1, "a listing error deleted the row");
        let _ = std::fs::remove_file(&not_a_dir);
    }

    #[test]
    fn v0_14_copying_the_same_image_twice_moves_one_row() {
        let s = store();
        let first = s.insert_image_at(None, &pixels(30, 20, 5), 1_000).unwrap();
        let again = s.insert_image_at(None, &pixels(30, 20, 5), 2_000).unwrap();
        assert_eq!(first, again);
        assert_eq!(s.len(), 1);
        assert_eq!(s.blobs.names().unwrap().len(), 1);
        assert_eq!(s.recent(1)[0].created_at, 2_000);
    }

    /// Same size, one pixel different: two screenshots, two rows.
    #[test]
    fn v0_14_a_near_identical_image_is_a_new_row() {
        let s = store();
        let a = pixels(300, 300, 5);
        let mut b = a.clone();
        b.pixels[4 * 1000] ^= 1;
        s.insert_image_at(None, &a, 1_000).unwrap();
        s.insert_image_at(None, &b, 2_000).unwrap();
        assert_eq!(s.len(), 2);
    }

    /// A text search never matches an image row; an empty one lists it.
    #[test]
    fn v0_14_text_search_skips_images_and_the_empty_search_lists_them() {
        let s = store();
        s.insert_image_at(None, &pixels(4, 4, 1), 1).unwrap();
        s.insert_at(ClipKind::Text, None, "Image notes", 2).unwrap();
        assert_eq!(s.search("image", 10).len(), 1);
        assert_eq!(s.search("", 10).len(), 2);
    }

    /// A v0.5 database has none of v0.14's columns. Opening it must add them
    /// and keep every row.
    #[test]
    fn v0_14_a_v0_5_database_migrates_in_place() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE clips (id INTEGER PRIMARY KEY, created_at INTEGER NOT NULL,
             kind TEXT NOT NULL, source_exe TEXT, len INTEGER NOT NULL,
             nonce BLOB NOT NULL, ciphertext BLOB NOT NULL)",
            [],
        )
        .unwrap();
        let s = ClipStore::with_connection(conn, &ClipKey::generate()).unwrap();
        s.insert(ClipKind::Text, None, "old shape").unwrap();
        s.insert_image(None, &pixels(2, 2, 1)).unwrap();
        assert_eq!(s.len(), 2);
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }
}
