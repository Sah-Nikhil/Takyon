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

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{real_apps, TempDir};
use takyon_lib::aliases::AliasStore;
use takyon_lib::entry::{EntryId, EntryKind, GameLauncher, LaunchTarget, MAX_ENTRIES};
use takyon_lib::frecency::Frecency;
use takyon_lib::icons::IconStore;
use takyon_lib::query::{Pipeline, LOCK_DELAY_MS};
use takyon_lib::sources::apps::games::{self, epic::EpicLibrary, GameLibrary};
use takyon_lib::sources::apps::AppSource;
use takyon_lib::sources::recents::{recent_from, RecentsSource};
use takyon_lib::sources::system::SystemSource;

/// A Pipeline over the shared walk, with usage stored in `dir`.
fn pipeline_in(dir: &TempDir) -> Arc<Pipeline> {
    let (apps, icons) = real_apps();
    let frecency = Arc::new(Frecency::open(Some(dir.to_owned())).expect("frecency.db"));
    Arc::new(Pipeline::new(
        apps,
        Arc::new(RecentsSource::new()),
        Arc::new(SystemSource::new()),
        icons,
        frecency,
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
    let p = Pipeline::new(apps, Arc::new(RecentsSource::new()), Arc::new(SystemSource::new()), icons.clone(), frecency);

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
    let p = Pipeline::new(apps, recents, Arc::new(SystemSource::new()), icons, frecency);
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
        Arc::new(SystemSource::new()),
        icons,
        frecency,
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
        let namespaced = ["aumid:", "steam:", "epic:"].iter().any(|p| id.starts_with(p));
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




/// What the Palette actually holds for one query: ids, icon keys, icon sharing.
///
/// `#[ignore]`d — it reads the real walk and the real `icons.bin`, so it can
/// only report. Written to explain duplicate rows without guessing at them.
#[test]
#[ignore = "measures the host machine; run explicitly with --ignored"]
fn v0_3_measure_what_a_query_returns() {
    let (apps, _) = real_apps();
    let Some(data) = takyon_lib::identity::data_dir() else {
        return;
    };
    let store = IconStore::new(Some(data));
    let bytes = store.extracted();

    // How many Entries share each icon, so the generic-icon rule is visible.
    let mut shares: std::collections::HashMap<Vec<u8>, usize> = std::collections::HashMap::new();
    for (_, key) in apps.icon_keys() {
        if let Some(b) = bytes.get(&key) {
            *shares.entry(b.clone()).or_default() += 1;
        }
    }

    let by_id: std::collections::HashMap<EntryId, String> = apps.icon_keys().into_iter().collect();
    let dir = TempDir::new("measure-query");
    let p = pipeline_in(&dir);

    for q in ["explorer", "node", "administrative"] {
        eprintln!("\n  {q:?}");
        for e in p.query(q, 1).entries.iter() {
            let icon = by_id.get(&e.id).and_then(|k| bytes.get(k));
            let shared = icon.and_then(|b| shares.get(b)).copied().unwrap_or(0);
            eprintln!(
                "    {:<38} shares icon with {} entries{}",
                e.title,
                shared.saturating_sub(1),
                if icon.is_none() { "  (icon not cached)" } else { "" }
            );
            eprintln!("      id {}", e.id.as_str());
        }
    }
}

/// Is a given id in the walked list at all, whatever a query does with it?
#[test]
#[ignore = "measures the host machine; run explicitly with --ignored"]
fn v0_3_measure_whether_an_aumid_survives_discovery() {
    let (apps, _) = real_apps();
    for id in [
        "aumid:Microsoft.Windows.AdministrativeTools",
        "aumid:Microsoft.Windows.Explorer",
        r"c:\windows\explorer.exe",
    ] {
        let found = apps.find(&EntryId(id.to_string()));
        eprintln!(
            "  {:<48} {}",
            id,
            match &found {
                Some(a) => format!("in the list as {:?}", a.title),
                None => "NOT in the list".to_string(),
            }
        );
    }
    let aumids = apps
        .icon_keys()
        .into_iter()
        .filter(|(id, _)| id.as_str().starts_with("aumid:"))
        .count();
    eprintln!("  {aumids} AUMID Entries survived discovery in total");
}


/// What a version column would cost and cover on this machine.
#[test]
#[ignore = "measures the host machine; run explicitly with --ignored"]
fn v0_3_measure_executable_versions() {
    let (apps, _) = real_apps();
    let paths: Vec<String> = apps
        .icon_keys()
        .into_iter()
        .map(|(id, _)| id.as_str().split('|').next().unwrap_or_default().to_string())
        .filter(|p| p.ends_with(".exe"))
        .collect();

    let started = Instant::now();
    let versions: Vec<Option<String>> = paths
        .iter()
        .map(|p| takyon_lib::version::of(std::path::Path::new(p)))
        .collect();
    let elapsed = started.elapsed();

    let have = versions.iter().filter(|v| v.is_some()).count();
    eprintln!(
        "  {} executables, {} carry a version ({}%), read in {} ms",
        paths.len(),
        have,
        have * 100 / paths.len().max(1),
        elapsed.as_millis()
    );

    for needle in ["node.exe", "powershell.exe", "explorer.exe", "odbcad32.exe"] {
        eprintln!("  {needle}:");
        for (p, v) in paths.iter().zip(&versions) {
            if p.ends_with(needle) {
                eprintln!("    {:<52} {}", p, v.clone().unwrap_or("-".into()));
            }
        }
    }
}

/// How many executables actually collide by filename? That is the set a version
/// column would have to read, and the cost of the feature.
#[test]
#[ignore = "measures the host machine; run explicitly with --ignored"]
fn v0_3_measure_executables_sharing_a_filename() {
    let (apps, _) = real_apps();
    let paths: Vec<String> = apps
        .icon_keys()
        .into_iter()
        .map(|(id, _)| id.as_str().split('|').next().unwrap_or_default().to_string())
        .filter(|p| p.ends_with(".exe"))
        .collect();

    let mut by_name: std::collections::HashMap<&str, Vec<&String>> =
        std::collections::HashMap::new();
    for p in &paths {
        let name = p.rsplit('\\').next().unwrap_or(p);
        by_name.entry(name).or_default().push(p);
    }
    // Distinct paths only: one binary listed twice is not a collision.
    let colliding: Vec<(&str, Vec<&String>)> = by_name
        .into_iter()
        .map(|(n, mut v)| {
            v.sort();
            v.dedup();
            (n, v)
        })
        .filter(|(_, v)| v.len() > 1)
        .collect();

    let files: usize = colliding.iter().map(|(_, v)| v.len()).sum();
    let started = Instant::now();
    let mut differing = 0usize;
    for (name, group) in &colliding {
        let versions: Vec<Option<String>> = group
            .iter()
            .map(|p| takyon_lib::version::of(std::path::Path::new(p)))
            .collect();
        let distinct: std::collections::HashSet<_> = versions.iter().collect();
        if distinct.len() > 1 {
            differing += 1;
            eprintln!("    {name}");
            for (p, v) in group.iter().zip(&versions) {
                eprintln!("      {:<52} {}", p, v.clone().unwrap_or("-".into()));
            }
        }
    }
    eprintln!(
        "\n  {} names collide over {} files, read in {} ms",
        colliding.len(),
        files,
        started.elapsed().as_millis()
    );
    eprintln!("  {differing} of those are told apart by their version");
}

/// Versions land on the rows that need them and nowhere else.
#[test]
fn v0_3_only_ambiguous_executables_carry_a_version() {
    let dir = TempDir::new("versions");
    let p = pipeline_in(&dir);
    let (apps, _) = real_apps();

    let mut with_version = 0usize;
    for q in ["node", "chrome", "powershell", "explorer", "notepad"] {
        for e in p.query(q, 1).entries.iter() {
            if let Some(v) = &e.version {
                with_version += 1;
                eprintln!("  {:<32} {v}", e.title);
                assert!(!v.is_empty(), "an empty version is worse than none");
            }
        }
    }
    assert!(with_version > 0, "nothing carried a version at all");
    // The cost control: a version everywhere would mean reading 1233 files.
    let total = apps.len();
    assert!(
        with_version < total / 10,
        "{with_version} of {total} carry a version, which is not a collision set"
    );
}

/// Why did each row match? The rung, computed before Frecency touches it.
///
/// Scores in a `QueryResult` are already lifted by usage and already reduced by
/// the length penalty, which overlaps the tier bands — reading a rung off them
/// is guesswork. This re-scores each Entry's own `Haystack` instead.
#[test]
#[ignore = "measures the host machine; run explicitly with --ignored"]
fn v0_3_measure_why_rows_match() {
    use takyon_lib::entry::Query;
    use takyon_lib::rank::{
        self, TIER_ACRONYM, TIER_ALIAS_EXACT, TIER_EXACT_NAME, TIER_EXE_PREFIX, TIER_NAME_PREFIX,
        TIER_WORD_PREFIX,
    };

    // Base tiers only, so a penalised score still lands in its own band.
    let rung = |s: f32| {
        for (base, name) in [
            (TIER_ALIAS_EXACT, "alias"),
            (TIER_EXACT_NAME, "exact name"),
            (TIER_NAME_PREFIX, "name prefix"),
            (TIER_WORD_PREFIX, "word prefix"),
            (TIER_EXE_PREFIX, "EXE STEM"),
            (TIER_ACRONYM, "acronym"),
        ] {
            if s > base - 25.0 {
                return name;
            }
        }
        "other"
    };

    let dir = TempDir::new("why");
    let p = pipeline_in(&dir);
    let (apps, _) = real_apps();

    for q in ["chrome", "code", "photo", "term"] {
        eprintln!("
  {q:?}");
        for e in p.query(q, 1).entries.iter().take(6) {
            let Some(app) = apps.find(&e.id) else { continue };
            let base = rank::score(&Query::new(q), &app.hay).unwrap_or(0.0);
            eprintln!(
                "    {:<34} {:>11} {:>6.0}   stem {:?}",
                e.title,
                rung(base),
                base,
                app.hay.exe_stem.as_deref().unwrap_or("-")
            );
        }
    }
}

/// Which bare-PATH executables live under the Windows dir, and would any of
/// them vanish entirely if the PATH walk skipped that directory?
#[test]
#[ignore = "measures the host machine; run explicitly with --ignored"]
fn v0_3_measure_windows_dir_path_exes() {
    use takyon_lib::sources::apps::path;

    let sysroot = std::env::var("SystemRoot")
        .unwrap_or_else(|_| r"C:\Windows".into())
        .to_lowercase();

    // Everything the other three Sources already know, by lowercased title/stem.
    let (apps, _) = real_apps();
    let dir = TempDir::new("winexe");
    let p = pipeline_in(&dir);

    let mut under = 0usize;
    let mut unique = 0usize;
    for exe in path::discover() {
        let full = exe.path.to_string_lossy().to_lowercase();
        if !full.starts_with(&sysroot) {
            continue;
        }
        under += 1;
        // Is this stem reachable any other way? Query it and see if a non-PATH
        // row answers.
        let others = p
            .query(&exe.stem, 1)
            .entries
            .into_iter()
            .any(|e| {
                e.title.eq_ignore_ascii_case(&exe.stem)
                    && e.id.as_str() != full
            });
        let tag = if others { "" } else { "  <-- ONLY here" };
        if !others {
            unique += 1;
        }
        eprintln!("  {:<24} {}{}", exe.stem, full, tag);
        let _ = &apps;
    }
    eprintln!("
  {under} bare-PATH exes under {sysroot}; {unique} reachable no other way");
}

/// After the fix: `explorer` returns File Explorer, and no bare `explorer` row.
#[test]
fn v0_3_explorer_is_one_row_not_two() {
    let dir = TempDir::new("explorer-fix");
    let p = pipeline_in(&dir);
    let titles: Vec<(String, String)> = p
        .query("explorer", 1)
        .entries
        .into_iter()
        .map(|e| (e.title, e.id.as_str().to_string()))
        .collect();
    for (t, id) in &titles {
        eprintln!("  {t:<34} {id}");
    }
    // The bare PATH exe (id is exactly the plain path, no args) must be gone.
    assert!(
        !titles.iter().any(|(_, id)| id == r"c:\windows\explorer.exe"),
        "the bare explorer.exe row is still here"
    );
    // File Explorer (the shell app) must survive.
    assert!(
        titles.iter().any(|(t, _)| t.eq_ignore_ascii_case("File Explorer")),
        "File Explorer disappeared"
    );
}

/// The SDK shortcut left `explorer`'s results but must stay findable by name.
#[test]
fn v0_3_the_sdk_shortcut_is_still_reachable_by_its_name() {
    let dir = TempDir::new("sdk");
    let p = pipeline_in(&dir);
    let hit = p
        .query("development", 1)
        .entries
        .into_iter()
        .any(|e| e.title.to_lowercase().contains("software development kit"));
    assert!(hit, "the SDK shortcut vanished entirely");
}

/// What titles actually mention "development"? Diagnostic.
#[test]
#[ignore = "measures the host machine"]
fn v0_3_measure_sdk_titles() {
    let dir = TempDir::new("sdk2");
    let p = pipeline_in(&dir);
    for q in ["software", "development", "kit", "windows software"] {
        eprintln!("  {q:?}");
        for e in p.query(q, 1).entries.iter().take(6) {
            eprintln!("    {}", e.title);
        }
    }
}

// ----------------------------------------------------------- system entries

/// The All Tasks folder, walked for real (task 8).
///
/// COM produced something or it did not: an apartment or interface mistake shows
/// up here as an empty list, not a crash. No count is asserted — the tasks are
/// whatever this Windows build ships.
#[test]
fn v0_3_the_control_panel_walk_yields_named_tasks() {
    let tasks = takyon_lib::sources::system::control_panel_tasks();
    assert!(
        !tasks.is_empty(),
        "the All Tasks folder yielded nothing — COM or the folder GUID is wrong"
    );
    for t in &tasks {
        assert!(!t.title.is_empty(), "a task came back with no name");
        assert!(
            matches!(t.target, LaunchTarget::ShellItem(_)),
            "{} is not launchable by shell item",
            t.title
        );
        assert!(t.id.as_str().starts_with("system:"));
    }
}

/// The launch path's first half, proven without opening a window.
///
/// Every captured PIDL must bind back to a live shell item through exactly the
/// call `launch::shell_execute_idlist` makes, or the row is a dead button. A
/// sample: binding all ~200 is slow, and one failure class is enough.
#[test]
fn v0_3_control_panel_pidls_bind_for_launch() {
    let tasks = takyon_lib::sources::system::control_panel_tasks();
    for t in tasks.iter().take(10) {
        let LaunchTarget::ShellItem(pidl) = &t.target else {
            continue;
        };
        assert!(
            takyon_lib::launch::shell_item_is_bindable(pidl),
            "{:?} did not bind to a shell item",
            t.title
        );
    }
}

/// A full refresh carries both halves: the curated settings and the walked tasks.
#[test]
fn v0_3_a_refreshed_system_source_holds_settings_and_tasks() {
    use takyon_lib::sources::system::{settings_catalog, SystemSource};

    let source = SystemSource::new();
    source.refresh();
    assert!(source.is_ready());
    assert!(
        source.len() > settings_catalog().len(),
        "refresh added no control-panel tasks on top of the settings table"
    );

    // The settings half is machine-independent: bluetooth always reaches its page.
    let bt = EntryId("ms-settings:bluetooth".into());
    assert!(source.find(&bt).is_some(), "the settings half went missing");
}

#[test]
#[ignore = "measures the host machine"]
fn v0_3_measure_control_panel_tasks() {
    let tasks = takyon_lib::sources::system::control_panel_tasks();
    eprintln!("  {} tasks", tasks.len());
    for t in tasks.iter().take(8) {
        if let LaunchTarget::ShellItem(pidl) = &t.target {
            let ok = takyon_lib::launch::shell_item_is_bindable(pidl);
            eprintln!(
                "  [{}] {} ({} pidl bytes)",
                if ok { "ok" } else { "XX" },
                t.title,
                pidl.len()
            );
        }
    }
}

// ------------------------------------------------------------------ epic games

/// A stale manifest must not become an Entry.
///
/// This is the whole reason task 9 reads the disk rather than trusting the
/// directory: Epic leaves the `.item` file behind when a game is uninstalled, and
/// every one of the seven on this machine is stale. Raycast lists all seven.
#[test]
fn v0_3_an_epic_game_whose_executable_is_gone_is_dropped() {
    let dir = TempDir::new("epic");
    let installed = dir.path().join("FallGuys");
    std::fs::create_dir_all(&installed).expect("install dir");
    std::fs::write(installed.join("RunFallGuys.exe"), b"MZ").expect("executable");

    let manifest = |app_name: &str, name: &str, location: &Path, exe: &str| {
        let json = serde_json::json!({
            "AppName": app_name,
            "DisplayName": name,
            "InstallLocation": location.to_string_lossy(),
            "LaunchExecutable": exe,
        });
        std::fs::write(
            dir.path().join(format!("{app_name}.item")),
            serde_json::to_vec(&json).expect("manifest json"),
        )
        .expect("write manifest");
    };
    manifest("live", "Fall Guys", &installed, "RunFallGuys.exe");
    manifest("stale", "Dying Light", &dir.path().join("DyingLight"), "DyingLightGame.exe");
    manifest("dlc", "Dying Light The Following", &installed, "");

    let games = EpicLibrary::at(dir.path()).games();
    let names: Vec<&str> = games.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, vec!["Fall Guys"], "kept a game that cannot start");
    assert_eq!(games[0].id, "live");
    assert_eq!(games[0].launcher, GameLauncher::Epic);
}

/// The launch contract for a game Entry: launcher id in, no file out.
///
/// The absent path is the assertion that matters. It is what stops the action
/// menu offering reveal, copy path or elevate on something that has no file, and
/// what keeps the EntryId off a path that changes when the library moves drive.
#[test]
fn v0_3_a_game_entry_carries_a_launcher_id_and_no_file() {
    let target = LaunchTarget::Game {
        launcher: GameLauncher::Epic,
        id: "0a2d9f6403244d12969e11da6713137b".into(),
    };
    assert_eq!(
        EntryId::for_launch(&target).as_str(),
        "epic:0a2d9f6403244d12969e11da6713137b"
    );
    assert!(takyon_lib::launch::path_of(&target).is_none());
    assert!(takyon_lib::launch::run_as_admin(&target).is_err());
    assert!(takyon_lib::launch::reveal(&target).is_err());
}

#[test]
#[ignore = "measures the host machine"]
fn v0_3_measure_game_libraries() {
    for library in games::all() {
        let found = library.games();
        eprintln!("  {} — {} games", library.launcher().label(), found.len());
        for g in found.iter().take(8) {
            eprintln!("    {} ({})", g.name, library.launcher().uri(&g.id));
        }
    }
}

/// Where a settings page and an application land for the same few letters.
///
/// Written to chase a live report: typing `dis` for Discord selected the Display
/// settings page instead. Prints the rung and the pipeline score of every row so
/// the cause is read off rather than guessed at.
#[test]
#[ignore = "measures the host machine; run explicitly with --ignored"]
fn v0_3_measure_how_apps_and_settings_compete() {
    let dir = TempDir::new("compete");
    let (apps, icons) = real_apps();
    let system = Arc::new(SystemSource::new());
    system.refresh();
    let frecency = Arc::new(Frecency::open(Some(dir.to_owned())).expect("frecency.db"));
    let p = Arc::new(Pipeline::new(
        apps.clone(),
        Arc::new(RecentsSource::new()),
        system.clone(),
        icons,
        frecency,
    ));

    for q in ["dis", "disk", "blu", "wifi", "display", "keyboard", "not"] {
        eprintln!("\n  {q:?}");
        for e in p.query(q, 1).entries.iter().take(6) {
            let hay = apps
                .find(&e.id)
                .map(|a| a.hay)
                .or_else(|| system.find(&e.id).map(|s| s.hay));
            let base = hay
                .as_ref()
                .and_then(|h| takyon_lib::rank::score(&takyon_lib::entry::Query::new(q), h))
                .unwrap_or(0.0);
            eprintln!(
                "    {:<38} {:?}  base {:>6.1}  score {:>7.1}",
                e.title, e.kind, base, e.score
            );
        }
    }
}

// -------------------------------------------------------- desktop shortcuts

/// A Desktop shortcut adds a row or it adds nothing — never a second copy.
///
/// Task 10's whole rule is that Desktop loses every collision, so the EntryId
/// stays on the Start Menu copy. Asserted through the Palette rather than the
/// list, because one row per application is the property the user sees.
#[test]
fn v0_3_a_desktop_shortcut_never_puts_a_second_row_in_the_palette() {
    use takyon_lib::sources::apps::lnk;

    let dir = TempDir::new("desktop");
    let p = pipeline_in(&dir);

    for sc in lnk::discover_desktop() {
        let rows = p.query(&sc.name, 1).entries;
        let same: Vec<&str> = rows
            .iter()
            .filter(|e| e.title.eq_ignore_ascii_case(&sc.name))
            .map(|e| e.id.as_str())
            .collect();
        assert!(
            same.len() <= 1,
            "{} appears {} times: {:?}",
            sc.name,
            same.len(),
            same
        );
    }
}

/// Which Desktop shortcuts are duplicates and which is the one that is not.
#[test]
#[ignore = "measures the host machine"]
fn v0_3_measure_what_the_desktop_adds() {
    use takyon_lib::sources::apps::lnk;

    for root in lnk::desktop_roots() {
        eprintln!("  root {}", root.display());
    }
    let (apps, _) = real_apps();
    let dir = TempDir::new("deskmeasure");
    let desktop = lnk::discover_desktop();
    eprintln!("  {} desktop shortcuts", desktop.len());

    for sc in &desktop {
        let id = EntryId::for_launch(&LaunchTarget::Exe {
            path: sc.target.clone(),
            args: sc.args.clone(),
            working_dir: sc.working_dir.clone(),
        });
        // Three outcomes. `NEW` — the Desktop shortcut is the row. `same target`
        // — an earlier path produced the identical EntryId and `seen` dropped
        // this one. `same title` — the id is not a row at all, so it lost the
        // title check to something reached another way.
        let kept = apps.find(&id);
        let verdict = match &kept {
            Some(a) if a.icon_source.as_deref() == Some(sc.link.as_path()) => "NEW",
            Some(_) => "same target",
            None => "same title",
        };
        eprintln!("    [{verdict:>11}] {:<34} -> {}", sc.name, sc.target.display());
        if kept.is_none() {
            let p = pipeline_in(&dir);
            for e in p.query(&sc.name, 1).entries.iter().take(2) {
                eprintln!("                  won by: {} ({})", e.title, e.id.as_str());
            }
        }
    }
}

// ------------------------------------------------ the verify script, driven

/// Every step of `docs/verify/v0.3.md` that needs no window and no launch.
///
/// Machine-dependent by nature — it names Discord and Obsidian — so it is
/// `#[ignore]`d and prints a verdict per step. Frecency is a temp database, so
/// every app scores as if never launched: the conservative case.
#[test]
#[ignore = "drives the manual script against the host machine"]
fn v0_3_run_the_verify_steps_that_need_no_launch() {
    let dir = TempDir::new("verify");
    let (apps, icons) = real_apps();
    let system = Arc::new(SystemSource::new());
    system.refresh();
    let frecency = Arc::new(Frecency::open(Some(dir.to_owned())).expect("frecency.db"));
    let p = Arc::new(Pipeline::new(
        apps.clone(),
        Arc::new(RecentsSource::new()),
        system,
        icons,
        frecency,
    ));

    let mut failed = 0usize;
    let mut report = |step: &str, ok: bool, detail: String| {
        if !ok {
            failed += 1;
        }
        eprintln!("  [{}] {step:<5} {detail}", if ok { "pass" } else { "FAIL" });
    };

    let titles = |q: &str| -> Vec<String> {
        p.query(q, 1).entries.iter().map(|e| e.title.clone()).collect()
    };
    let top_is = |q: &str, want: &str| -> (bool, String) {
        let rows = titles(q);
        let got = rows.first().cloned().unwrap_or_else(|| "(nothing)".into());
        (got == want, format!("{q:?} -> {}", rows.join(" | ")))
    };

    // --- RK: the System weight and the keyword rung.
    for (step, q, want) in [
        ("RK1", "dis", "Discord"),
        ("RK2", "blu", "Bluetooth"),
        ("RK3", "display", "Display"),
        ("RK4", "disk", "Disk Cleanup"),
        ("RK7", "not", "Notepad"),
    ] {
        let (ok, detail) = top_is(q, want);
        report(step, ok, detail);
    }
    // RK6 asks only that an application leads, not which one.
    let kb = p.query("keyboard", 1).entries;
    let ok = kb.first().is_some_and(|e| e.kind == EntryKind::App);
    report("RK6", ok, format!("keyboard -> {:?}", titles("keyboard")));

    // --- DK: Desktop shortcuts add no duplicate row.
    for name in ["Obsidian", "Postman", "GitHub Desktop"] {
        let rows = titles(name);
        let n = rows.iter().filter(|t| t.eq_ignore_ascii_case(name)).count();
        report("DK1", n == 1, format!("{name} -> {n} row(s): {}", rows.join(" | ")));
    }

    // DK2 is the one with teeth: the Desktop copies point at a dead
    // `C:\Program Files\Roblox`, so the kept row must be the one that exists.
    for name in ["Roblox Player", "Roblox Studio"] {
        let rows = p.query(name, 1).entries;
        let hit = rows.iter().find(|e| e.title.eq_ignore_ascii_case(name));
        let detail = match hit.and_then(|e| apps.find(&e.id)) {
            Some(a) => match &a.target {
                LaunchTarget::Exe { path, .. } => {
                    format!("{name} -> {} (exists: {})", path.display(), path.is_file())
                }
                other => format!("{name} -> {other:?}"),
            },
            None => format!("{name} -> (no row)"),
        };
        let ok = detail.ends_with("exists: true)");
        report("DK2", ok, detail);
    }

    // --- EP3: a stale Epic manifest must never become a row.
    for game in ["fall guys", "dying light", "nba 2k21"] {
        let rows = titles(game);
        report("EP3", rows.is_empty(), format!("{game:?} -> {:?}", rows));
    }

    assert_eq!(failed, 0, "{failed} step(s) failed — see the log above");
}

// ------------------------------------------------------- winget (task 11)

/// Task 11, measure-first: would a winget Source add anything?
///
/// Input is `winget list --source winget`, checked in beside this file so the
/// measurement reproduces without running winget. Version and architecture noise
/// is trimmed first: `Signal 8.23.0` is reachable as `Signal`.
#[test]
#[ignore = "measures the host machine"]
fn v0_3_measure_what_winget_would_add() {
    let listing = include_str!("winget-list.txt");
    let dir = TempDir::new("winget");
    let p = pipeline_in(&dir);

    // winget appends the installer's own version string and bitness. Keep words
    // until one starts with a digit or looks like an arch tag.
    let stem = |name: &str| -> String {
        let mut kept: Vec<&str> = Vec::new();
        for word in name.split_whitespace() {
            let w = word.trim_matches(|c: char| c == '(' || c == ')' || c == ',');
            let versionish = w.chars().next().is_some_and(|c| c.is_ascii_digit())
                || matches!(w.to_lowercase().as_str(), "x64" | "x86" | "64-bit" | "32-bit");
            if versionish {
                break;
            }
            kept.push(word);
        }
        if kept.is_empty() { name.to_string() } else { kept.join(" ") }
    };

    let mut missing = Vec::new();
    let mut total = 0usize;
    for raw in listing.lines().map(str::trim).filter(|l| !l.is_empty()) {
        total += 1;
        let needle = stem(raw);
        let reached = p
            .query(&needle, 1)
            .entries
            .iter()
            .any(|e| e.title.to_lowercase().starts_with(&needle.to_lowercase()));
        if !reached {
            missing.push((raw, needle));
        }
    }

    eprintln!("  {total} winget packages, {} not reachable", missing.len());
    for (raw, needle) in &missing {
        eprintln!("    {raw:<52} (queried {needle:?})");
    }
}

/// The winget names that looked like real applications, queried the way a person
/// would type them rather than the way winget prints them.
#[test]
#[ignore = "measures the host machine"]
fn v0_3_measure_whether_winget_apps_are_already_reachable() {
    let dir = TempDir::new("wingetreach");
    let p = pipeline_in(&dir);
    for q in [
        "terminal", "visual studio code", "outlook", "onedrive", "roblox",
        "ollama", "nvm", "rustup", "gh", "wsl", "java", "r 4", "signal",
        "powertoys", "winrar", "zen", "docker", "f.lux", "hwinfo", "github cli",
    ] {
        let rows: Vec<String> = p.query(q, 1).entries.iter().take(2).map(|e| e.title.clone()).collect();
        eprintln!("  {:<20} {}", format!("{q:?}"), if rows.is_empty() { "(nothing)".into() } else { rows.join(" | ") });
    }
}
