//! The web-search key, wrapped with Windows DPAPI.
//!
//! Same treatment as the clipboard key (ADR-0008), for a stronger reason: this
//! one is a bearer token for someone else's paid account. It lives at
//! `creds\web.key.dpapi` and never in `settings.db`, which is plain SQLite
//! anyone can open.
//!
//! The key is never sent to the frontend. Settings asks whether one is stored
//! and sees a hint — last four characters — so a wrong paste is visible without
//! the value crossing IPC.

use std::path::{Path, PathBuf};

/// The key file, relative to the data directory.
pub const KEY_PATH: &[&str] = &["creds", "web.key.dpapi"];

/// Characters of the key shown back in Settings. Enough to tell two keys apart,
/// too few to use.
const HINT_LEN: usize = 4;

/// Entropy for this key alone, separate from the clipboard's (ADR-0008).
///
/// A blob wrapped for one secret must not unwrap in the other's code path.
/// Frozen, and still spelled `brave` after ADR-0021 moved the provider to Exa:
/// entropy is an input to decryption, not a label.
const ENTROPY: &[u8] = b"com.v3sper.takyon/brave.key/v1";

/// The pre-ADR-0020 entropy. [`load`] rewraps a key found under it, same as the
/// clipboard key does — no machine is known to hold one, but a key silently
/// dropped reads as `!s` forgetting a key it never lost.
const LEGACY_ENTROPY: &[u8] = b"com.v3sper.launcher/brave.key/v1";

/// Whether a key is safe to send as a header value.
///
/// CR and LF split an HTTP header, so a key carrying either could append headers
/// of its own to every request. Refused rather than stripped, so what is stored
/// is what was pasted.
fn is_sendable(key: &str) -> bool {
    !key.is_empty() && key.len() <= 512 && !key.chars().any(char::is_control)
}

/// Store a key, replacing whatever was there. Blank deletes it.
///
/// A key carrying control characters is refused: it would be a header injection
/// on every request, and no provider issues one that looks like that.
pub fn store(dir: &Path, key: &str) -> std::io::Result<()> {
    let path = key_file(dir);
    let key = key.trim();
    if key.is_empty() {
        return clear(dir);
    }
    if !is_sendable(key) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "That does not look like a key. Paste the value from the provider's console.",
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &path,
        crate::clips::key::protect_with(key.as_bytes(), ENTROPY)?,
    )
}

/// The stored key, or `None` when none is held.
///
/// A blob that will not unwrap is `None` rather than an error: it means the file
/// was copied from another account, and the fix is pasting the key again.
pub fn load(dir: &Path) -> Option<String> {
    let wrapped = std::fs::read(key_file(dir)).ok()?;
    let plain = match crate::clips::key::unprotect_with(&wrapped, ENTROPY) {
        Ok(plain) => plain,
        Err(_) => {
            // ADR-0020 rotation. Rewrapped on the way past, so this costs one
            // failed DPAPI call once rather than on every read.
            let plain = crate::clips::key::unprotect_with(&wrapped, LEGACY_ENTROPY).ok()?;
            if let Ok(rewrapped) = crate::clips::key::protect_with(&plain, ENTROPY) {
                let _ = std::fs::write(key_file(dir), rewrapped);
            }
            plain
        }
    };
    let key = String::from_utf8(plain).ok()?;
    let key = key.trim().to_string();
    // Checked on the way out as well as in: a file written by hand, or by an
    // older build, must not become a header.
    is_sendable(&key).then_some(key)
}

/// Delete the key. Silent when there was none.
pub fn clear(dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(key_file(dir)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Whether a key is stored, without unwrapping it.
pub fn present(dir: &Path) -> bool {
    load(dir).is_some()
}

/// The last few characters, for showing a stored key back without sending it.
pub fn hint(dir: &Path) -> Option<String> {
    let key = load(dir)?;
    let tail: String = key
        .chars()
        .rev()
        .take(HINT_LEN)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    Some(format!("…{tail}"))
}

pub fn key_file(dir: &Path) -> PathBuf {
    let mut path = dir.to_path_buf();
    for part in KEY_PATH {
        path.push(part);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of its own per test. Shared one and these clobber each
    /// other: `cargo test` runs them in parallel and each one writes the key.
    fn temp() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("takyon-key-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn v0_9_a_key_round_trips_through_dpapi() {
        let dir = temp();
        assert_eq!(load(&dir), None);
        store(&dir, "BSA-secret-token").expect("store");
        assert_eq!(load(&dir).as_deref(), Some("BSA-secret-token"));
        assert!(present(&dir));
    }

    /// The point of the file: what lands on disk must not be the key.
    #[test]
    fn v0_9_the_key_is_not_on_disk_in_plaintext() {
        let dir = temp();
        store(&dir, "BSA-secret-token").expect("store");
        let raw = std::fs::read(key_file(&dir)).expect("the blob");
        assert!(
            !raw.windows(16).any(|w| w == b"BSA-secret-token"),
            "the key appears verbatim inside its own DPAPI blob"
        );
    }

    /// Blank means "remove it", so clearing the box in Settings is how a key is
    /// taken off the machine.
    #[test]
    fn v0_9_storing_a_blank_key_deletes_it() {
        let dir = temp();
        store(&dir, "BSA-secret-token").expect("store");
        store(&dir, "   ").expect("blank");
        assert_eq!(load(&dir), None);
        assert!(!present(&dir));
    }

    /// Clearing a key that was never stored must be silent — Settings offers the
    /// button whether or not one is held.
    #[test]
    fn v0_9_clearing_nothing_is_not_an_error() {
        assert!(clear(&temp()).is_ok());
    }

    /// Surrounding whitespace comes with every paste from a web page and would
    /// otherwise be sent as part of the token.
    #[test]
    fn v0_9_a_pasted_key_is_trimmed() {
        let dir = temp();
        store(&dir, "  BSA-token\n").expect("store");
        assert_eq!(load(&dir).as_deref(), Some("BSA-token"));
    }

    /// The hint tells two keys apart and cannot be used as one.
    #[test]
    fn v0_9_the_hint_shows_only_the_tail() {
        let dir = temp();
        store(&dir, "BSA-abcdefgh1234").expect("store");
        let hint = hint(&dir).expect("a hint");
        assert_eq!(hint, "…1234");
        assert!(!hint.contains("abcdefgh"));
    }

    /// CR or LF in a key would append headers of its own to every request. The
    /// paste is refused rather than cleaned, so nothing unexpected is stored.
    #[test]
    fn v0_9_a_key_carrying_a_control_character_is_refused() {
        let dir = temp();
        for bad in [
            "BSA\r\nX-Evil: 1",
            "BSA\nmore",
            "BSA\u{0}null",
            "BSA\u{7f}del",
            "BSA\tTab",
        ] {
            assert!(store(&dir, bad).is_err(), "{bad:?} was stored");
        }
        assert!(!present(&dir));
        // A key of ordinary shape still stores.
        assert!(store(&dir, "BSA-abc_123-XYZ").is_ok());
    }

    /// A key long enough to be a paste of something else is not a key.
    #[test]
    fn v0_9_an_absurdly_long_key_is_refused() {
        assert!(store(&temp(), &"A".repeat(4096)).is_err());
    }

    /// A blob from another account will not unwrap. That reads as "no key",
    /// whose fix is pasting one, rather than as a failure with no action in it.
    #[test]
    fn v0_9_an_unreadable_blob_reads_as_no_key() {
        let dir = temp();
        let path = key_file(&dir);
        std::fs::create_dir_all(path.parent().unwrap()).expect("creds dir");
        std::fs::write(&path, b"not a DPAPI blob").expect("write");
        assert_eq!(load(&dir), None);
        assert!(!present(&dir));
    }
}
