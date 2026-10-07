//! Focus on real disk and real processes (v0.16).
//!
//! `focus.db` must outlive Takyon: a Strict session survives Task Manager and
//! a reboot only because the running row is read back at startup. Asserted
//! here by closing and reopening the store, not by trusting one connection.

#![cfg(windows)]

mod common;

use common::TempDir;
use takyon_lib::focus::categories::{Category, Edit, Item};
use takyon_lib::focus::session::{Clock, Strictness};
use takyon_lib::focus::store::{BlockList, EventKind, Outcome, Store, StoreError};

const MIN: u64 = 60_000;
const T0: u64 = 1_760_000_000_000;

fn list() -> BlockList {
    BlockList {
        categories: vec!["social".into()],
        items: vec!["exe:discord.exe".into(), "site:youtube.com".into()],
    }
}

#[test]
fn v0_16_a_running_session_is_read_back_after_reopening() {
    let dir = TempDir::new("focus-running");
    let id = {
        let store = Store::open(Some(dir.to_owned())).unwrap();
        store
            .start(
                "Write report",
                Strictness::Strict,
                &list(),
                &Clock::start(T0, 30 * MIN),
            )
            .unwrap()
    };
    let store = Store::open(Some(dir.to_owned())).unwrap();
    let running = store.running().expect("still running");
    assert_eq!(running.id, id);
    assert_eq!(running.goal, "Write report");
    assert_eq!(running.strictness, Strictness::Strict);
    assert_eq!(running.block_list, list());
    assert_eq!(running.clock, Clock::start(T0, 30 * MIN));
}

#[test]
fn v0_16_only_one_session_runs_at_a_time() {
    let dir = TempDir::new("focus-one");
    let store = Store::open(Some(dir.to_owned())).unwrap();
    store
        .start("a", Strictness::Soft, &list(), &Clock::start(T0, MIN))
        .unwrap();
    let second = store.start("b", Strictness::Soft, &list(), &Clock::start(T0, MIN));
    assert!(matches!(second, Err(StoreError::AlreadyRunning)));
}

#[test]
fn v0_16_a_paused_clock_is_saved_and_a_finished_session_stops_running() {
    let dir = TempDir::new("focus-finish");
    let store = Store::open(Some(dir.to_owned())).unwrap();
    let mut clock = Clock::start(T0, 30 * MIN);
    let id = store
        .start("Read", Strictness::Soft, &list(), &clock)
        .unwrap();
    clock.pause(T0 + 5 * MIN, Some(10 * MIN));
    store.save_clock(id, &clock).unwrap();
    assert_eq!(
        Store::open(Some(dir.to_owned()))
            .unwrap()
            .running()
            .unwrap()
            .clock,
        clock
    );

    store.finish(id, T0 + 40 * MIN, Outcome::Completed).unwrap();
    assert!(store.running().is_none());
    let history = store.history(T0 - 1);
    assert_eq!(history.len(), 1);
    assert_eq!(
        (
            history[0].session.id,
            history[0].ended_at,
            history[0].outcome
        ),
        (id, T0 + 40 * MIN, Outcome::Completed)
    );
    assert!(
        store.history(T0 + 41 * MIN).is_empty(),
        "history is by end time"
    );
    store
        .start(
            "Next",
            Strictness::Soft,
            &list(),
            &Clock::start(T0 + 50 * MIN, MIN),
        )
        .unwrap();
}

#[test]
fn v0_16_events_are_kept_in_order_and_cleared_with_history() {
    let dir = TempDir::new("focus-events");
    let store = Store::open(Some(dir.to_owned())).unwrap();
    let done = store
        .start("Done", Strictness::Soft, &list(), &Clock::start(T0, MIN))
        .unwrap();
    store
        .record(done, T0 + 1, EventKind::Blocked, Some("exe:discord.exe"))
        .unwrap();
    store
        .record(done, T0 + 2, EventKind::Snoozed, Some("exe:discord.exe"))
        .unwrap();
    store.finish(done, T0 + MIN, Outcome::Expired).unwrap();
    let live = store
        .start(
            "Live",
            Strictness::Soft,
            &list(),
            &Clock::start(T0 + 2 * MIN, MIN),
        )
        .unwrap();
    store
        .record(live, T0 + 2 * MIN + 1, EventKind::Paused, None)
        .unwrap();

    let events = store.events(T0);
    let kinds: Vec<_> = events
        .iter()
        .map(|e| (e.session_id, e.kind, e.target.as_deref()))
        .collect();
    assert_eq!(
        kinds,
        [
            (done, EventKind::Blocked, Some("exe:discord.exe")),
            (done, EventKind::Snoozed, Some("exe:discord.exe")),
            (live, EventKind::Paused, None),
        ]
    );

    store.clear_history().unwrap();
    assert!(store.history(0).is_empty());
    assert_eq!(
        store.events(0).len(),
        1,
        "the running session keeps its events"
    );
    assert_eq!(store.running().unwrap().id, live);
}

fn category(id: &str, keys: &[&str]) -> Category {
    Category {
        id: id.into(),
        name: "Mine".into(),
        icon: "Star".into(),
        dynamic: None,
        copied_from: Some("social".into()),
        items: keys
            .iter()
            .map(|k| Item {
                key: (*k).into(),
                label: k.to_string(),
            })
            .collect(),
        ..Default::default()
    }
}

#[test]
fn v0_16_custom_categories_are_created_edited_and_deleted() {
    let dir = TempDir::new("focus-categories");
    let store = Store::open(Some(dir.to_owned())).unwrap();
    let saved = store
        .save_category(&category("", &["site:x.com", "exe:discord.exe"]))
        .unwrap();
    assert!(saved.id.starts_with("custom:"), "{}", saved.id);

    let reopened = Store::open(Some(dir.to_owned())).unwrap();
    assert_eq!(reopened.custom_categories(), std::slice::from_ref(&saved));

    let mut edited = saved.clone();
    edited.name = "Renamed".into();
    edited.items.truncate(1);
    assert_eq!(reopened.save_category(&edited).unwrap(), edited);
    assert_eq!(reopened.custom_categories(), [edited.clone()]);

    reopened.delete_category(&edited.id).unwrap();
    assert!(reopened.custom_categories().is_empty());
}

fn edit(key: &str, label: &str, removed: bool) -> Edit {
    Edit {
        category: "social".into(),
        key: key.into(),
        label: label.into(),
        removed,
    }
}

/// Edits to a built-in survive a reopen; undoing one forgets it, and Reset
/// forgets everything.
#[test]
fn v0_16_edits_to_a_built_in_are_kept_restored_and_reset() {
    let dir = TempDir::new("focus-builtin-edits");
    let store = Store::open(Some(dir.to_owned())).unwrap();
    store
        .edit_builtin(&edit("exe:mastodon.exe", "Mastodon", false))
        .unwrap();
    store
        .edit_builtin(&edit("site:linkedin.com", "LinkedIn", true))
        .unwrap();
    let reopened = Store::open(Some(dir.to_owned())).unwrap();
    assert_eq!(
        reopened.builtin_edits(),
        [
            edit("exe:mastodon.exe", "Mastodon", false),
            edit("site:linkedin.com", "LinkedIn", true)
        ]
    );

    reopened
        .edit_builtin(&edit("site:linkedin.com", "LinkedIn", false))
        .unwrap();
    assert_eq!(
        reopened.builtin_edits(),
        [edit("exe:mastodon.exe", "Mastodon", false)]
    );
    reopened
        .edit_builtin(&edit("exe:mastodon.exe", "Mastodon", true))
        .unwrap();
    assert!(
        reopened.builtin_edits().is_empty(),
        "an added item just goes"
    );
    reopened
        .edit_builtin(&edit("site:x.com", "X", true))
        .unwrap();
    reopened.reset_builtin("social").unwrap();
    assert!(reopened.builtin_edits().is_empty());
}

// --- Processes ------------------------------------------------------------

use std::process::{Child, Command};
use std::time::{Duration, Instant};
use takyon_lib::focus::procs::{self, Quit};

/// Kills the child if a test panics before quitting it.
struct Spawned(Child);

impl Drop for Spawned {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

fn wait_for<F: FnMut() -> bool>(what: &str, timeout: Duration, mut ready: F) {
    let start = Instant::now();
    while !ready() {
        assert!(start.elapsed() < timeout, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Paths compared canonicalised: `%TEMP%` may be an 8.3 alias (CLAUDE.md).
#[test]
fn v0_16_the_snapshot_sees_this_test_with_its_image_path() {
    let me = std::process::id();
    let found = procs::snapshot()
        .into_iter()
        .find(|r| r.pid == me)
        .expect("own pid in snapshot");
    let canon = |p: &std::path::Path| std::fs::canonicalize(p).unwrap();
    assert_eq!(
        canon(&found.process.image),
        canon(&std::env::current_exe().unwrap())
    );
    assert_ne!(found.process.session_id, 0);
    assert_eq!(found.process.package_family, None);
}

/// A PowerShell WinForms window, up and visible. `on_close` runs in its
/// FormClosing handler.
fn form(on_close: &str) -> Spawned {
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; \
         $f = New-Object Windows.Forms.Form; $f.Text = 'takyon-focus-test'; \
         $f.add_FormClosing({{ {on_close} }}); [Windows.Forms.Application]::Run($f)"
    );
    let child = Spawned(
        Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", &script])
            .spawn()
            .unwrap(),
    );
    let pid = child.0.id();
    wait_for("the form", Duration::from_secs(30), || {
        procs::window_count(pid) > 0
    });
    child
}

/// A window that honours `WM_CLOSE` closes; nothing gets terminated.
#[test]
fn v0_16_a_windowed_app_is_asked_to_close_first() {
    let mut child = form("");
    assert_eq!(
        procs::quit(child.0.id(), Duration::from_secs(5)),
        Quit::Closed
    );
    assert!(child.0.try_wait().unwrap().is_some());
}

/// Asked, refused, terminated once the grace runs out.
#[test]
fn v0_16_a_window_refusing_to_close_is_terminated_after_the_grace() {
    let mut child = form("$_.Cancel = $true");
    let started = Instant::now();
    assert_eq!(
        procs::quit(child.0.id(), Duration::from_millis(800)),
        Quit::Terminated
    );
    assert!(started.elapsed() >= Duration::from_millis(800));
    wait_for("exit", Duration::from_secs(5), || {
        child.0.try_wait().unwrap().is_some()
    });
}

/// Nothing to ask, so no grace: `taskkill` without `/F` does the same.
#[test]
fn v0_16_a_windowless_process_is_terminated_at_once() {
    let mut child = Spawned(
        Command::new("ping.exe")
            .args(["-n", "120", "127.0.0.1"])
            .spawn()
            .unwrap(),
    );
    let started = Instant::now();
    assert_eq!(
        procs::quit(child.0.id(), Duration::from_secs(10)),
        Quit::Terminated
    );
    assert!(started.elapsed() < Duration::from_secs(5));
    wait_for("exit", Duration::from_secs(5), || {
        child.0.try_wait().unwrap().is_some()
    });
}

#[test]
fn v0_16_quitting_a_process_that_is_gone_says_so() {
    let mut child = Command::new("cmd.exe")
        .args(["/c", "exit"])
        .spawn()
        .unwrap();
    let pid = child.id();
    child.wait().unwrap();
    assert_eq!(procs::quit(pid, Duration::from_millis(200)), Quit::Gone);
}

/// The start form's elevation probe: our own child can be closed, a service
/// running as SYSTEM in session 0 cannot.
#[test]
fn v0_16_can_quit_tells_our_processes_from_ones_above_us() {
    let child = Spawned(
        Command::new("ping.exe")
            .args(["-n", "30", "127.0.0.1"])
            .spawn()
            .unwrap(),
    );
    assert!(procs::can_quit(child.0.id()));
    let services: Vec<u32> = procs::snapshot()
        .into_iter()
        .filter(|r| r.process.session_id == 0)
        .map(|r| r.pid)
        .collect();
    assert!(
        services.is_empty() || services.iter().any(|&pid| !procs::can_quit(pid)),
        "every session 0 process was quittable"
    );
}

/// Machine-dependent: shape only. Every entry an absolute `.exe` with a name.
#[test]
fn v0_16_registered_browsers_read_from_the_registry() {
    for (exe, name) in takyon_lib::focus::browser::registered() {
        assert!(exe.is_absolute(), "{exe:?}");
        assert!(
            exe.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("exe")),
            "{exe:?}"
        );
        assert!(!name.trim().is_empty(), "{exe:?}");
    }
}

/// Network: the first-launch and add-a-site fetch lands in the `!s` cache
/// (ADR-0037). `--ignored`, run by CI's daily `web-search` job.
#[test]
#[ignore = "fetches icons from youtube.com and wikipedia.org"]
fn v0_16_live_site_icons_land_in_the_shared_cache() {
    use takyon_lib::focus::icons::{fetch_missing, hosts_of};
    use takyon_lib::search::favicon::cached;
    let dir = TempDir::new("focus-icons");
    let hosts = hosts_of(["site:youtube.com", "site:wikipedia.org"]);
    assert_eq!(fetch_missing(dir.path(), &hosts), 2);
    for host in &hosts {
        assert!(cached(dir.path(), host).is_some(), "{host}");
    }
    assert_eq!(
        fetch_missing(dir.path(), &hosts),
        0,
        "cached hosts cost nothing"
    );
}
