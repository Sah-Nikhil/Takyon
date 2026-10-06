//! Focus on real disk and real processes (v0.16).
//!
//! `focus.db` must outlive Takyon: a Strict session survives Task Manager and
//! a reboot only because the running row is read back at startup. Asserted
//! here by closing and reopening the store, not by trusting one connection.

#![cfg(windows)]

mod common;

use common::TempDir;
use takyon_lib::focus::categories::{Category, Item};
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
