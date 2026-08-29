//! The Recents Source against a Recent folder we build ourselves.
//!
//! This machine has `Start_TrackDocs = 0`, so the shell writes no shortcuts and
//! the real folder is permanently empty (`docs/tbd/v0.3.md` §1). Reading them
//! from elsewhere would violate the intent behind that setting, so instead the
//! test points `%APPDATA%` at a temp tree and writes real `.lnk` files through
//! `IShellLinkW` — the shell's own writer, read back by the shell's own reader.
//!
//! Its own test binary because it mutates process environment, and a `Mutex`
//! within it because the tests inside still share one process.

mod common;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use common::TempDir;
use takyon_lib::entry::{EntryKind, Query, Source};
use takyon_lib::sources::recents::{recent_dir, RecentsSource};

use windows::core::{Interface, HSTRING};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

/// `%APPDATA%` is process-wide, so only one test may own it at a time.
static APPDATA: Mutex<()> = Mutex::new(());

/// A redirected `%APPDATA%` with an empty Recent folder inside it.
///
/// Restores the old value on drop, and the [`TempDir`] it holds removes the tree
/// — including the shortcuts written into it.
struct FakeRecent {
    _guard: MutexGuard<'static, ()>,
    _dir: TempDir,
    previous: Option<std::ffi::OsString>,
    recent: PathBuf,
}

impl FakeRecent {
    fn new(label: &str) -> Self {
        let guard = APPDATA.lock().unwrap_or_else(|e| e.into_inner());
        let dir = TempDir::new(label);
        let previous = std::env::var_os("APPDATA");
        std::env::set_var("APPDATA", dir.path());

        let recent = recent_dir().expect("APPDATA is set");
        std::fs::create_dir_all(&recent).expect("Recent folder");
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok().ok();

        FakeRecent {
            _guard: guard,
            _dir: dir,
            previous,
            recent,
        }
    }

    /// Write a real shortcut into the Recent folder, the way the shell does.
    fn link_to(&self, name: &str, target: &Path) {
        let link: IShellLinkW =
            unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }.expect("ShellLink");
        unsafe { link.SetPath(&HSTRING::from(target.as_os_str())) }.expect("SetPath");
        let file: IPersistFile = link.cast().expect("IPersistFile");
        let path = self.recent.join(format!("{name}.lnk"));
        unsafe { file.Save(&HSTRING::from(path.as_os_str()), true) }.expect("Save");
    }

    fn write_file(&self, name: &str) -> PathBuf {
        let path = self.recent.parent().unwrap().join(name);
        std::fs::write(&path, b"fixture").expect("fixture file");
        path
    }

    fn make_dir(&self, name: &str) -> PathBuf {
        let path = self.recent.parent().unwrap().join(name);
        std::fs::create_dir_all(&path).expect("fixture dir");
        path
    }
}

impl Drop for FakeRecent {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(v) => std::env::set_var("APPDATA", v),
            None => std::env::remove_var("APPDATA"),
        }
    }
}

/// Verification step N1, on a machine where the shell will never write one.
///
/// The whole path: shell writes the shortcut, `lnk::read` reads it back through
/// COM, `recent_from` turns it into an Entry, the Source ranks it. Until now
/// nothing in the suite executed any of it.
#[test]
fn v0_3_a_shortcut_in_the_recent_folder_becomes_a_document_entry() {
    let fake = FakeRecent::new("recents-file");
    let doc = fake.write_file("quarterly report.txt");
    fake.link_to("quarterly report", &doc);

    let source = RecentsSource::new();
    source.refresh();
    assert_eq!(source.len(), 1, "the shortcut was not read back");

    let entries = source.query(&Query::new("quarterly"), std::time::Duration::from_millis(20));
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].title, "quarterly report.txt");
    assert_eq!(entries[0].kind, EntryKind::File);
    assert_eq!(entries[0].id.as_str(), doc.to_string_lossy().to_lowercase());
}

/// `docs/tbd/v0.3.md` §2, demonstrated rather than deduced from reading code.
///
/// `lnk::read` drops any target that is not a file, so `recent_from`'s Folder
/// branch is unreachable from `discover`. Recorded as a gap owned by v0.7; this
/// is the test that will go red the day it is closed.
#[test]
fn v0_3_a_recently_opened_folder_never_reaches_the_palette() {
    let fake = FakeRecent::new("recents-folder");
    let folder = fake.make_dir("Project Notes");
    fake.link_to("Project Notes", &folder);

    let source = RecentsSource::new();
    source.refresh();
    assert_eq!(
        source.len(),
        0,
        "folders now arrive: close tbd v0.3 §2 and delete this test"
    );
}

/// A target that has been deleted must not produce a row.
///
/// ADR-0013's existence check, on the Source that reaches the most volatile
/// paths — the shell keeps a shortcut long after the document is gone.
#[test]
fn v0_3_a_recent_whose_file_was_deleted_is_dropped() {
    let fake = FakeRecent::new("recents-stale");
    let doc = fake.write_file("deleted later.txt");
    fake.link_to("deleted later", &doc);
    std::fs::remove_file(&doc).expect("remove the target");

    let source = RecentsSource::new();
    source.refresh();
    assert_eq!(source.len(), 0, "a dead shortcut produced a row");
}

/// The Recent folder holds more than shortcuts, and none of the rest is a row.
#[test]
fn v0_3_the_recent_folder_is_read_for_shortcuts_only() {
    let fake = FakeRecent::new("recents-noise");
    let doc = fake.write_file("real thing.txt");
    fake.link_to("real thing", &doc);
    std::fs::write(fake.recent.join("desktop.ini"), b"[.ShellClassInfo]").unwrap();
    std::fs::create_dir_all(fake.recent.join("AutomaticDestinations")).unwrap();
    std::fs::write(
        fake.recent.join("AutomaticDestinations").join("x.automaticDestinations-ms"),
        b"\x00\x01binary",
    )
    .unwrap();

    let source = RecentsSource::new();
    source.refresh();
    assert_eq!(source.len(), 1, "something other than the shortcut arrived");
}

/// Verification step N4, on real shortcuts rather than `set_for_test` fixtures.
#[test]
fn v0_3_a_recent_never_outranks_an_application_that_matches_as_well() {
    let fake = FakeRecent::new("recents-order");
    let doc = fake.write_file("notepad.txt");
    fake.link_to("notepad", &doc);

    let recents = Arc::new(RecentsSource::new());
    recents.refresh();
    assert_eq!(recents.len(), 1);

    // No shared walk here: it reads `%APPDATA%` for the Start Menu, which this
    // test has redirected. One `AppSource` of its own, inside the redirect.
    let dir = TempDir::new("recents-order-data");
    let icons = Arc::new(takyon_lib::icons::IconStore::new(None));
    let apps = Arc::new(takyon_lib::sources::apps::AppSource::new());
    apps.refresh(&icons);
    let frecency =
        Arc::new(takyon_lib::frecency::Frecency::open(Some(dir.to_owned())).unwrap());

    let p = takyon_lib::query::Pipeline::new(apps, recents, icons, frecency);
    let kinds: Vec<_> = p.query("notepad", 1).entries.iter().map(|e| e.kind).collect();
    eprintln!("  notepad -> {kinds:?}");

    assert!(kinds.contains(&EntryKind::File), "the recent did not match");
    if let Some(first_doc) = kinds.iter().position(|k| *k == EntryKind::File) {
        assert!(
            kinds[..first_doc].iter().all(|k| *k == EntryKind::App),
            "a document sorted above an application: {kinds:?}"
        );
    }
}
