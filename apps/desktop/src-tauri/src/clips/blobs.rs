//! Image clip bytes, one sealed file each, outside `clips.db` (v0.14).
//!
//! Files rather than BLOB rows: a 4K screenshot is megabytes, and inline it
//! takes `clips.db` into gigabytes and makes every sweep rewrite them. And
//! destruction becomes an explicit act on a named file: overwrite, sync, unlink.
//! The bytes arrive already sealed by `store.rs`; nothing here sees plaintext.
//!
//! Every path that destroys clipboard data ends in [`Blobs::destroy`], so the
//! rule ADR-0008 cares about ("row deleted" is not "secret gone") is one function.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The directory under the data directory. Frozen: a rename orphans every blob.
pub const DIR: &str = "clips";

const EXT: &str = "bin";

/// Where sealed blobs live. `Memory` is the in-memory store's twin, for tests.
pub enum Blobs {
    Disk(PathBuf),
    Memory(Mutex<HashMap<String, Vec<u8>>>),
}

impl Blobs {
    pub fn disk(data_dir: &Path) -> std::io::Result<Self> {
        let dir = data_dir.join(DIR);
        std::fs::create_dir_all(&dir)?;
        Ok(Blobs::Disk(dir))
    }

    pub fn memory() -> Self {
        Blobs::Memory(Mutex::new(HashMap::new()))
    }

    /// Store sealed bytes under a fresh random name, returned.
    ///
    /// Written to `.tmp` and renamed, so a crash mid-write never leaves a
    /// half-file under a real name. The startup sweep removes stray `.tmp`s.
    pub fn write(&self, sealed: &[u8]) -> std::io::Result<String> {
        let name = fresh_name();
        match self {
            Blobs::Disk(dir) => {
                let tmp = dir.join(format!("{name}.tmp"));
                let mut file = std::fs::File::create(&tmp)?;
                file.write_all(sealed)?;
                file.sync_all()?;
                drop(file);
                std::fs::rename(&tmp, dir.join(format!("{name}.{EXT}")))?;
            }
            Blobs::Memory(map) => {
                lock(map).insert(name.clone(), sealed.to_vec());
            }
        }
        Ok(name)
    }

    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        if !valid(name) {
            return None;
        }
        match self {
            Blobs::Disk(dir) => std::fs::read(dir.join(format!("{name}.{EXT}"))).ok(),
            Blobs::Memory(map) => lock(map).get(name).cloned(),
        }
    }

    /// Overwrite with zeros, sync, then unlink. Best-effort: a file that will
    /// not open is unlinked anyway, and one that will not unlink is retried by
    /// the next startup sweep, which sees it as an orphan.
    pub fn destroy(&self, name: &str) {
        if !valid(name) {
            return;
        }
        match self {
            Blobs::Disk(dir) => shred(&dir.join(format!("{name}.{EXT}"))),
            Blobs::Memory(map) => {
                lock(map).remove(name);
            }
        }
    }

    /// Every blob name present. What the orphan sweep diffs against the rows.
    ///
    /// An error, never a short list: the sweep deletes whatever this omits, so
    /// an unreadable directory must stop it rather than read as empty.
    pub fn names(&self) -> std::io::Result<Vec<String>> {
        let Blobs::Disk(dir) = self else {
            let Blobs::Memory(map) = self else {
                unreachable!()
            };
            return Ok(lock(map).keys().cloned().collect());
        };
        let mut names = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == EXT) {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if valid(stem) {
                        names.push(stem.to_string());
                    }
                }
            }
        }
        Ok(names)
    }

    /// Shred anything a crash left mid-write. Run once, at startup.
    pub fn sweep_partial(&self) {
        let Blobs::Disk(dir) = self else {
            return;
        };
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "tmp") {
                shred(&path);
            }
        }
    }

    /// Bytes the blob files occupy. Settings shows it (v0.14 task 8).
    pub fn bytes(&self) -> u64 {
        match self {
            Blobs::Disk(dir) => std::fs::read_dir(dir)
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok()?.metadata().ok())
                .filter(|m| m.is_file())
                .map(|m| m.len())
                .sum(),
            Blobs::Memory(map) => lock(map).values().map(|v| v.len() as u64).sum(),
        }
    }
}

/// Overwrite then unlink. The content is ciphertext already; zeroing is so a
/// leaked key later still finds nothing in freed clusters on a spinning disk.
fn shred(path: &Path) {
    if let Ok(len) = std::fs::metadata(path).map(|m| m.len()) {
        if let Ok(mut file) = std::fs::OpenOptions::new().write(true).open(path) {
            let zeros = vec![0u8; 64 * 1024];
            let mut left = len;
            while left > 0 {
                let n = left.min(zeros.len() as u64) as usize;
                if file.write_all(&zeros[..n]).is_err() {
                    break;
                }
                left -= n as u64;
            }
            let _ = file.sync_all();
        }
    }
    let _ = std::fs::remove_file(path);
}

/// 32 hex characters from the OS CSPRNG. Not the row id: the blob is written
/// before its row exists, so a crash between the two leaves an orphan, not a
/// row pointing at nothing.
fn fresh_name() -> String {
    use aes_gcm::aead::rand_core::RngCore;
    let mut bytes = [0u8; 16];
    aes_gcm::aead::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Names come back out of `clips.db`, so they are checked before they become a
/// path: anything but 32 lowercase hex characters never reaches the filesystem.
fn valid(name: &str) -> bool {
    name.len() == 32
        && name
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_14_a_blob_round_trips_and_is_gone_after_destroy() {
        let blobs = Blobs::memory();
        let name = blobs.write(b"sealed").unwrap();
        assert_eq!(blobs.read(&name).as_deref(), Some(&b"sealed"[..]));
        assert_eq!(blobs.names().unwrap(), vec![name.clone()]);
        blobs.destroy(&name);
        assert!(blobs.read(&name).is_none());
        assert!(blobs.names().unwrap().is_empty());
    }

    /// A tampered row must not turn into a path outside `clips\`.
    #[test]
    fn v0_14_a_name_that_is_not_hex_never_reaches_the_filesystem() {
        assert!(!valid("..\\..\\creds\\clip.key"));
        assert!(!valid("ABCDEF0123456789ABCDEF0123456789"));
        assert!(!valid("abc"));
        assert!(valid(&fresh_name()));
    }

    #[test]
    fn v0_14_two_writes_get_two_names() {
        let blobs = Blobs::memory();
        assert_ne!(blobs.write(b"a").unwrap(), blobs.write(b"a").unwrap());
    }
}
