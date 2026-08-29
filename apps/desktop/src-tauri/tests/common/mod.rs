//! Shared fixtures for the integration tests.
//!
//! Two things live here because getting either wrong is expensive. The COM walk
//! costs ~450 ms and every test in a binary wants the same one, so it is taken
//! once. And every test writes to a real directory, so it gets a guard that
//! removes it again even when the test panics.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use takyon_lib::icons::IconStore;
use takyon_lib::sources::apps::AppSource;

/// A temp directory that deletes itself.
///
/// Named after the caller and the process, so two test binaries running at once
/// cannot collide. Deletion is retried: `icons.bin` is memory-mapped, and a map
/// that has not been dropped yet holds the file open on Windows.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(label: &str) -> Self {
        let dir = std::env::temp_dir()
            .join("takyon-tests")
            .join(format!("{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        TempDir(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn to_owned(&self) -> PathBuf {
        self.0.clone()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        for _ in 0..5 {
            if std::fs::remove_dir_all(&self.0).is_ok() || !self.0.exists() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        eprintln!("[tests] could not remove {}", self.0.display());
    }
}

/// The machine's real applications, walked once per test binary.
///
/// Every caller shares it, so the ~450 ms COM walk is paid once. The `IconStore`
/// comes back too because discovery registers icon sources into it, and a store
/// that did not see the walk can extract nothing.
pub fn real_apps() -> (Arc<AppSource>, Arc<IconStore>) {
    static WALK: OnceLock<(Arc<AppSource>, Arc<IconStore>)> = OnceLock::new();
    WALK.get_or_init(|| {
        // No directory: this store never reads or writes a blob, so the shared
        // walk cannot fight a test that wants its own icons.bin.
        let icons = Arc::new(IconStore::new(None));
        let apps = Arc::new(AppSource::new());
        apps.refresh(&icons);
        (apps, icons)
    })
    .clone()
}
