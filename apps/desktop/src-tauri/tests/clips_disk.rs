//! `clips.db` on real disk (v0.5).
//!
//! The unit tests run in memory, which is exactly where the two claims this
//! feature makes cannot be checked: that what lands on disk is ciphertext, and
//! that a swept row is *gone* rather than unlinked. Both are answered here by
//! reading the bytes of the file, which is what the phase's exit criterion asks
//! for — "verified by opening the file, not by trusting the UI".

#![cfg(windows)]

mod common;

use common::TempDir;
use rusqlite::Connection;
use takyon_lib::clips::store::{ClipKind, ClipStore, Retention};

/// The literal a hex editor would be looking for. Long and unlikely, so a match
/// anywhere in the file is a real one rather than a coincidence.
const SECRET: &str = "correct-horse-battery-staple-9f3c1a";

/// Every byte of `clips.db` and its WAL sidecar.
///
/// The WAL matters as much as the database: it holds the live copy of a row
/// until checkpoint, so a test reading only the main file would pass while the
/// content sat in plaintext beside it (ADR-0008).
fn on_disk(dir: &std::path::Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    for name in ["clips.db", "clips.db-wal", "clips.db-shm"] {
        if let Ok(mut file) = std::fs::read(dir.join(name)) {
            bytes.append(&mut file);
        }
    }
    bytes
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// The ciphertext of the one row in the database.
fn ciphertext(dir: &std::path::Path) -> Vec<u8> {
    let conn = Connection::open(dir.join("clips.db")).expect("open clips.db");
    conn.query_row("SELECT ciphertext FROM clips", [], |r| {
        r.get::<_, Vec<u8>>(0)
    })
    .expect("one row")
}

/// ADR-0008's claim, against the file rather than against the API.
#[test]
fn v0_5_a_stored_clip_is_not_readable_in_the_file() {
    let dir = TempDir::new("clips-plaintext");
    let store = ClipStore::open(Some(dir.to_owned())).expect("open");
    store
        .insert(ClipKind::Text, Some("notepad.exe"), SECRET)
        .expect("insert");
    // Force the WAL out to disk, so this reads what a stolen copy would hold
    // rather than what is still in this process's page cache.
    drop(store);

    let bytes = on_disk(dir.path());
    assert!(!bytes.is_empty(), "clips.db was never written");
    assert!(
        !contains(&bytes, SECRET.as_bytes()),
        "the clipboard content is readable in clips.db"
    );
    // The accepted leak, asserted so it stays deliberate: metadata is plaintext.
    assert!(
        contains(&bytes, b"notepad.exe"),
        "source_exe was expected to be plaintext (ADR-0008)"
    );
}

/// The key is wrapped, and it survives a restart — a regenerated key would make
/// every stored clip undecryptable and present as history that emptied itself.
#[test]
fn v0_5_the_key_is_wrapped_on_disk_and_reused_across_opens() {
    let dir = TempDir::new("clips-key");
    let store = ClipStore::open(Some(dir.to_owned())).expect("open");
    let id = store.insert(ClipKind::Text, None, SECRET).expect("insert");
    drop(store);

    let key_file = takyon_lib::clips::key::key_file(dir.path());
    assert!(key_file.exists(), "no key was written to creds\\");
    let wrapped = std::fs::read(&key_file).expect("read key");
    assert!(wrapped.len() > 32, "a DPAPI blob is longer than the key");

    let reopened = ClipStore::open(Some(dir.to_owned())).expect("reopen");
    assert_eq!(
        reopened.content(id).as_deref(),
        Some(SECRET),
        "a clip stored before the restart is unreadable after it"
    );
}

/// "The row is deleted" and "the secret is gone" are different claims, and this
/// asserts the second one: after a sweep, the ciphertext is not in the file.
#[test]
fn v0_5_a_swept_clip_leaves_no_ciphertext_behind() {
    let dir = TempDir::new("clips-sweep");
    let store = ClipStore::open(Some(dir.to_owned())).expect("open");

    let now = 10_000_000;
    store
        .insert_at(
            ClipKind::Text,
            Some("notepad.exe"),
            SECRET,
            now - 40 * 86_400,
        )
        .expect("insert");
    let ciphertext = ciphertext(dir.path());
    assert!(ciphertext.len() > 16, "AES-GCM output is at least a tag");
    assert!(
        contains(&on_disk(dir.path()), &ciphertext),
        "the ciphertext should be findable before the sweep, or this test proves nothing"
    );

    assert_eq!(store.sweep_at(Retention::OneMonth, now), 1);
    drop(store);

    assert!(
        !contains(&on_disk(dir.path()), &ciphertext),
        "a swept clip's ciphertext is still recoverable from clips.db"
    );
}

/// The sweep destroys only what is past the window. A retention change that
/// emptied the whole history would be a far worse bug than one that kept too much.
#[test]
fn v0_5_a_sweep_keeps_everything_inside_the_window() {
    let dir = TempDir::new("clips-window");
    let store = ClipStore::open(Some(dir.to_owned())).expect("open");

    let now = 10_000_000;
    store
        .insert_at(ClipKind::Text, None, "kept", now - 3 * 86_400)
        .expect("insert");
    store
        .insert_at(ClipKind::Text, None, SECRET, now - 40 * 86_400)
        .expect("insert");

    assert_eq!(store.count_older_than(now - 30 * 86_400), 1);
    assert_eq!(store.sweep_at(Retention::OneMonth, now), 1);
    let left = store.recent(10);
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].preview, "kept");
}

/// An image whose pixels spell out a recognisable run, so a leak is findable.
fn screenshot() -> takyon_lib::clips::image::Rgba {
    let pixels = (0..64 * 64)
        .flat_map(|i| [(i % 256) as u8, 0x5a, (i / 64) as u8, 255])
        .collect();
    takyon_lib::clips::image::Rgba {
        width: 64,
        height: 64,
        pixels,
    }
}

/// Every blob file under `clips\`, by path.
fn blob_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(dir.join(takyon_lib::clips::blobs::DIR))
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default()
}

/// v0.14's exit criterion on disk: the blob is ciphertext, and deleting the
/// row takes the file with it.
#[test]
fn v0_14_an_image_blob_is_sealed_on_disk_and_gone_after_delete() {
    let dir = TempDir::new("clips-image");
    let store = ClipStore::open(Some(dir.to_owned())).expect("open");
    let id = store.insert_image(None, &screenshot()).expect("insert");
    let png = store.image(id).expect("decrypts");

    let files = blob_files(dir.path());
    assert_eq!(files.len(), 1, "one image, one blob file");
    let sealed = std::fs::read(&files[0]).expect("read blob");
    assert!(
        !contains(&sealed, &png[..32]),
        "the blob holds the PNG in clear"
    );
    assert!(
        !contains(&on_disk(dir.path()), b"IHDR"),
        "clips.db holds a PNG in clear"
    );

    assert_eq!(store.delete(id), 1);
    assert!(
        blob_files(dir.path()).is_empty(),
        "the blob outlived its row"
    );
}

/// A crash between a blob and its row leaves one without the other. Reopening
/// shreds the orphan file and drops the row whose file is gone.
#[test]
fn v0_14_reopening_reconciles_orphans_both_ways() {
    let dir = TempDir::new("clips-orphans");
    let store = ClipStore::open(Some(dir.to_owned())).expect("open");
    store.insert_image(None, &screenshot()).expect("insert");
    drop(store);

    let blobs = dir.path().join(takyon_lib::clips::blobs::DIR);
    for file in blob_files(dir.path()) {
        std::fs::remove_file(file).expect("remove");
    }
    std::fs::write(blobs.join(format!("{}.bin", "0".repeat(32))), b"orphan").expect("orphan");
    std::fs::write(blobs.join("half.tmp"), b"partial").expect("partial");

    let store = ClipStore::open(Some(dir.to_owned())).expect("reopen");
    assert!(
        store.is_empty(),
        "a row whose blob vanished survived reopening"
    );
    assert!(
        blob_files(dir.path()).is_empty(),
        "an orphan or partial file survived"
    );
}
