//! The real machine, from `cargo test`.
//!
//! Everything here touches something the unit tests cannot: COM, the shell, an
//! SQLite file on disk, or the wall clock. It is the layer between `--lib` and
//! `docs/verify/`, and it exists because most of what v0.3 built is not UI, so
//! no browser harness can reach it either (TBC-0007).
//!
//! Machine-dependent by construction. Assertions are therefore about shape and
//! ordering, never about which applications are installed.

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{real_apps, TempDir};
use takyon_lib::aliases::AliasStore;
use takyon_lib::collapse::CollapseStore;
use takyon_lib::entry::{EntryId, EntryKind, MAX_ENTRIES};
use takyon_lib::frecency::Frecency;
use takyon_lib::icons::IconStore;
use takyon_lib::query::{Pipeline, LOCK_DELAY_MS};
use takyon_lib::sources::apps::AppSource;
use takyon_lib::sources::recents::{recent_from, RecentsSource};

/// A Pipeline over the shared walk, with usage stored in `dir`.
fn pipeline_in(dir: &TempDir) -> Arc<Pipeline> {
    let (apps, icons) = real_apps();
    let frecency = Arc::new(Frecency::open(Some(dir.to_owned())).expect("frecency.db"));
    let collapse = Arc::new(CollapseStore::open(Some(dir.to_owned())).expect("collapse tables"));
    Arc::new(Pipeline::new(
        apps,
        Arc::new(RecentsSource::new()),
        icons,
        frecency,
        collapse,
    ))
}

/// A query that matches a lot, for tests needing several App rows.
const BROAD: &str = "e";

// ----------------------------------------------------------------- the walk

/// The four discovery paths, run for real.
///
/// No count is asserted, because the count is whatever is installed. What is
/// asserted is that COM produced something: an apartment or interface mistake
/// returns zero rather than an error, and looks like an empty machine.
#[test]
fn v0_3_the_real_walk_fills_the_palette() {
    let (apps, _) = real_apps();
    assert!(
        apps.len() > 100,
        "the COM walk found {} applications, so it failed silently",
        apps.len()
    );
    assert!(!apps.is_indexing(), "refresh must clear the indexing flag");

    let dir = TempDir::new("walk");
    let p = pipeline_in(&dir);

    let started = Instant::now();
    let r = p.query(BROAD, 1);
    let elapsed = started.elapsed();
    eprintln!(
        "  {} apps, query {BROAD:?} -> {} entries in {} us",
        apps.len(),
        r.entries.len(),
        elapsed.as_micros()
    );

    assert!(!r.entries.is_empty());
    assert!(r.entries.len() <= MAX_ENTRIES);
    // `bun run bench` owns the 20 ms budget. This only catches a collapse.
    assert!(elapsed < Duration::from_millis(200), "query took {elapsed:?}");
}

// -------------------------------------------------------------------- icons

/// `IShellItemImageFactory` through to `icons.bin` and back.
///
/// The one path where a Win32 exe and a UWP package yield an icon the same way,
/// so it has no pure-Rust equivalent. The round trip is task 0's second half:
/// v0.2 wrote a 12-byte header and re-extracted on every launch.
#[test]
fn v0_3_icons_extract_through_com_and_survive_a_restart() {
    let dir = TempDir::new("icons");
    let icons = Arc::new(IconStore::new(Some(dir.to_owned())));
    let apps = Arc::new(AppSource::new());
    apps.refresh(&icons);

    let frecency = Arc::new(Frecency::open(Some(dir.to_owned())).unwrap());
    let collapse = Arc::new(CollapseStore::open(None).unwrap());
    let p = Pipeline::new(
        apps,
        Arc::new(RecentsSource::new()),
        icons.clone(),
        frecency,
        collapse,
    );

    let mut keys = Vec::new();
    for entry in p.query(BROAD, 1).entries.iter().take(6) {
        let Some(icon) = &entry.icon else { continue };
        let Some(png) = icons.get(&icon.0) else { continue };
        assert_eq!(&png[..4], b"\x89PNG", "{} gave a non-PNG icon", entry.title);
        keys.push((icon.0.clone(), png));
    }
    assert!(!keys.is_empty(), "the shell yielded no icon at all");

    icons.flush().expect("flush icons.bin");
    let size = std::fs::metadata(dir.path().join("icons.bin"))
        .expect("icons.bin")
        .len();
    eprintln!("  {} icons, icons.bin = {size} bytes", keys.len());
    assert!(size > 12, "icons.bin is still just its header");

    // A second store over the same directory. Icons must come from the blob
    // rather than the shell, which is what makes a cold start cheap.
    let reopened = IconStore::new(Some(dir.to_owned()));
    for (key, expected) in &keys {
        assert_eq!(
            reopened.get(key).as_ref(),
            Some(expected),
            "icon {key} did not survive the round trip"
        );
    }
}

// ----------------------------------------------------------------- frecency

/// Verification step R5, against the real index rather than two fixtures.
///
/// The `--lib` version proves a second `Pipeline` reads what the first wrote.
/// This adds the part only the real walk has: the learned Entry climbs past a
/// thousand genuine competitors instead of one.
#[test]
fn v0_3_usage_learned_by_one_pipeline_ranks_the_next_one() {
    let dir = TempDir::new("frecency");

    let (chosen, cold_top) = {
        let p = pipeline_in(&dir);
        let entries = p.query(BROAD, 1).entries;
        assert!(entries.len() >= 2, "need two rows to move one past the other");
        // Not the top row: promoting something already first proves nothing.
        let chosen = entries[1].id.clone();
        for _ in 0..10 {
            p.frecency.record(&chosen, EntryKind::App).unwrap();
        }
        (chosen, entries[0].id.clone())
    };

    let fresh = pipeline_in(&dir);
    let top = fresh.query(BROAD, 1).entries[0].id.clone();
    eprintln!("  cold {} -> warm {}", cold_top.as_str(), top.as_str());
    assert_eq!(top, chosen, "a fresh Pipeline ignored what was learned");
}

// ------------------------------------------------------------- kind ordering

/// Task 4: applications above documents, never interleaved.
///
/// The `--lib` tests assert this over synthetic Entries. Here both Sources are
/// real and competing for one list, which is the arrangement the rule exists for
/// and the one v0.3's remaining Sources will make routine.
#[test]
fn v0_3_applications_outrank_documents_in_one_real_list() {
    let dir = TempDir::new("kinds");
    let (apps, icons) = real_apps();

    // The query is taken from an application this machine actually has, and the
    // documents are named after it. A broad query would fill all twelve rows
    // with applications and the ordering would never be exercised at all.
    let probe = pipeline_in(&dir);
    let title = probe.query(BROAD, 1).entries[0].title.clone();
    let word = title.split_whitespace().next().unwrap().to_lowercase();

    let recents = Arc::new(RecentsSource::new());
    recents.set_for_test(vec![
        recent_from(&std::path::PathBuf::from(format!(r"C:\docs\{word} plan.txt"))).unwrap(),
        recent_from(&std::path::PathBuf::from(format!(r"C:\docs\{word} notes.txt"))).unwrap(),
    ]);

    let frecency = Arc::new(Frecency::open(Some(dir.to_owned())).unwrap());
    let collapse = Arc::new(CollapseStore::open(None).unwrap());
    let p = Pipeline::new(apps, recents, icons, frecency, collapse);
    let entries = p.query(&word, 1).entries;
    let kinds: Vec<_> = entries.iter().map(|e| e.kind).collect();

    eprintln!("  {word:?} -> {kinds:?}");
    assert!(kinds.contains(&EntryKind::App), "no application matched {word:?}");
    assert!(
        kinds.contains(&EntryKind::File),
        "no document matched {word:?}, so nothing was ordered"
    );

    let last_app = kinds.iter().rposition(|k| *k == EntryKind::App).unwrap();
    let first_doc = kinds.iter().position(|k| *k != EntryKind::App).unwrap();
    assert!(
        last_app < first_doc,
        "a document appeared above an application: {kinds:?}"
    );
}

// ------------------------------------------------------------ stability lock

/// Verification step S1, without a person and without a slowed Source.
///
/// The `--lib` tests inject `now_ms`; this one sleeps, so it is the only thing
/// exercising the real clock inside `query`. A late answer that outranks the
/// settled top must append below it.
#[test]
fn v0_3_a_settled_top_holds_against_the_real_clock() {
    let dir = TempDir::new("stability");
    let p = pipeline_in(&dir);

    let entries = p.query(BROAD, 1).entries;
    assert!(entries.len() >= 2);
    let top = entries[0].id.clone();
    let rival = entries[1].id.clone();

    // Past LOCK_DELAY_MS in real time, which is what commits the top row.
    std::thread::sleep(Duration::from_millis(LOCK_DELAY_MS + 40));
    let settled = p.query(BROAD, 2).entries[0].id.clone();
    assert_eq!(settled, top, "the top changed before anything happened");

    for _ in 0..20 {
        p.frecency.record(&rival, EntryKind::App).unwrap();
    }

    let after = p.query(BROAD, 3);
    assert_eq!(after.entries[0].id, top, "the locked top was displaced");
    assert!(
        after.entries.iter().any(|e| e.id == rival),
        "the better answer was dropped instead of appended"
    );
}

// ------------------------------------------------------------------- aliases

/// Verification steps A1 and A2, which have never been run.
///
/// There is no editor until v0.6, so the script asks for a hand-written INSERT
/// and a restart. `AliasStore` plus `apply_aliases` is the same thing without
/// either, against the real application list.
#[test]
fn v0_3_an_alias_puts_its_target_first_in_the_real_list() {
    let dir = TempDir::new("aliases");
    let (apps, icons) = real_apps();
    let frecency = Arc::new(Frecency::open(Some(dir.to_owned())).unwrap());
    let p = Pipeline::new(
        apps.clone(),
        Arc::new(RecentsSource::new()),
        icons,
        frecency,
        Arc::new(CollapseStore::open(None).unwrap()),
    );

    // A real id the Palette already reaches, aliased to a string matching
    // nothing. The real id matters: an alias pointing at nothing proves nothing.
    let target = p.query(BROAD, 1).entries[0].id.clone();
    let alias = "zzqx";
    assert!(
        p.query(alias, 2).entries.is_empty(),
        "{alias} must match nothing before the alias exists"
    );

    let store = AliasStore::open(Some(dir.to_owned())).expect("settings.db");
    store.set(alias, &target).expect("insert alias");
    apps.apply_aliases(&store);

    let entries = p.query(alias, 3).entries;
    assert_eq!(entries.first().map(|e| &e.id), Some(&target), "alias missed");
    eprintln!("  {alias} -> {}", entries[0].title);

    // A2: removing it puts the Entry back where matching alone had it.
    store.remove(alias).expect("remove alias");
    apps.apply_aliases(&store);
    assert!(
        p.query(alias, 4).entries.is_empty(),
        "the alias outlived its row in the table"
    );
}

// ----------------------------------------------------------------- identity

/// Ids key Frecency, so a collision is a corrupt history rather than a
/// duplicate row (task 0, tbd v0.2 §9).
#[test]
fn v0_3_no_two_palette_rows_share_an_entry_id() {
    let dir = TempDir::new("ids");
    let p = pipeline_in(&dir);
    for q in ["e", "co", "s", "de", "po"] {
        let entries = p.query(q, 1).entries;
        let mut seen = std::collections::HashSet::new();
        for e in &entries {
            assert!(
                seen.insert(e.id.clone()),
                "{q:?} returned {} twice",
                e.id.as_str()
            );
            assert!(!e.id.as_str().is_empty());
        }
    }
}

/// Every id the Palette shows round-trips through the action menu, which is how
/// `Ctrl+K` and Enter find their target again.
///
/// Path ids lowercase so one exe reached two ways is one history. Prefixed ids
/// are the shell's own strings and stay verbatim.
#[test]
fn v0_3_every_id_the_palette_shows_resolves_to_actions() {
    let dir = TempDir::new("idshape");
    let p = pipeline_in(&dir);
    for e in p.query(BROAD, 1).entries.iter() {
        let id = e.id.as_str();
        let namespaced = id.starts_with("aumid:") || id.starts_with("steam:");
        if !namespaced {
            assert_eq!(id, id.to_lowercase(), "{id} is not canonicalised");
        }
        assert!(
            !p.actions_for(&EntryId(id.to_string())).is_empty(),
            "{id} resolves to no actions"
        );
    }
}

// ------------------------------------------------------------------ identity

/// Every candidate the icon signal produces on this machine, and what stops it.
///
/// `#[ignore]`d because it reads the real `icons.bin` and depends on what is
/// installed. It is the measurement TBC-0008 asks for before trusting anything.
#[test]
#[ignore = "measures the host machine; run explicitly with --ignored"]
fn v0_3_measure_the_real_icon_pairs() {
    use takyon_lib::collapse::pairs_by_icon;

    let (apps, _) = real_apps();
    let Some(data) = takyon_lib::identity::data_dir() else {
        eprintln!("  no data directory");
        return;
    };
    let icons = IconStore::new(Some(data));
    let bytes = icons.extracted();
    eprintln!("  icons.bin holds {} icons", bytes.len());

    let with_icons: Vec<(EntryId, Vec<u8>)> = apps
        .icon_keys()
        .into_iter()
        .filter_map(|(id, key)| bytes.get(&key).map(|b| (id, b.clone())))
        .collect();
    eprintln!("  {} of {} Entries have one", with_icons.len(), apps.len());

    let pairs = pairs_by_icon(&with_icons);
    eprintln!("  {} candidate pairs after the generic-icon rule:", pairs.len());
    for (a, b) in &pairs {
        eprintln!("    {}\n      {}", a.as_str(), b.as_str());
    }
}

/// The safety property, on the real machine: icons alone can hide nothing.
///
/// Seven pairs share icon bytes here and only one is a genuine duplicate. What
/// stops the other six is that neither half has ever been seen starting the
/// other's executable, so nothing is collapsed until a launch says so.
#[test]
fn v0_3_matching_icons_alone_never_hide_a_row() {
    let dir = TempDir::new("collapse-safety");
    let (apps, icons) = real_apps();
    let before = apps.len();

    let store = CollapseStore::open(Some(dir.to_owned())).unwrap();
    let frecency = Frecency::open(Some(dir.to_owned())).unwrap();
    let decided = takyon_lib::collapse::learn(&apps, &icons, &store, &frecency, Some(dir.path()));

    assert!(decided.is_empty(), "collapsed {decided:?} with no launch evidence");
    assert_eq!(apps.len(), before, "a row disappeared on the icon signal alone");
    assert!(store.active().is_empty());
}

/// A corroborated duplicate does collapse, so the safety test above is not
/// passing simply because nothing ever collapses.
///
/// The evidence is injected rather than launched: a test that starts real
/// applications is not a test.
#[test]
fn v0_3_a_corroborated_duplicate_does_collapse() {
    let dir = TempDir::new("collapse-acts");
    let image = r"c:\windows\system32\control.exe";
    let winner = EntryId(image.to_string());
    let loser = EntryId("aumid:Microsoft.Windows.AdministrativeTools".into());

    let store = CollapseStore::open(Some(dir.to_owned())).unwrap();
    let frecency = Frecency::open(Some(dir.to_owned())).unwrap();
    for id in [&winner, &loser] {
        for _ in 0..2 {
            store.observe(id, std::path::Path::new(image)).unwrap();
        }
    }

    let found = store.collapses(&[(loser.clone(), winner.clone())]);
    assert_eq!(found.len(), 1, "a corroborated pair did not collapse");
    assert_eq!(found[0].winner, winner, "the AUMID beat the real path");
    assert_eq!(found[0].loser, loser);

    // And the decision is durable, which is what makes suppression survive a walk.
    assert_eq!(store.apply(&found, &frecency).len(), 1);
    assert_eq!(store.active().len(), 1);
    assert!(store.apply(&found, &frecency).is_empty());
}
