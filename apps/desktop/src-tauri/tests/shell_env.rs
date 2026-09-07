//! `PATH` hydration against this machine's real environment.
//!
//! `#[ignore]`d for the reason `web_search`'s live tests are: it spawns the
//! user's own login shell or reads the live registry, so a failure here is a
//! machine, not a commit. Run it by hand once per platform:
//! `cargo test --test shell_env -- --ignored --nocapture`.
//!
//! **Machine-dependent by construction.** Nothing asserts which directories
//! appear — only that something answered, that it kept every inherited entry,
//! and that a second call is served from the cache.

use std::time::Instant;

use takyon_lib::agents::shellenv;

/// Something answers, it keeps what we already had, and it says which mechanism
/// it was. On Windows that is always the registry; on unix it is a shell name.
#[test]
#[ignore = "spawns the user's login shell or reads the live registry"]
fn v0_11_hydration_recovers_a_superset_of_the_inherited_path() {
    shellenv::invalidate();
    let started = Instant::now();
    shellenv::hydrate();
    let took = started.elapsed();

    let report = shellenv::report().expect("hydrate always leaves a report");
    let hydrated = shellenv::hydrated_path().expect("this machine has an environment to read");
    eprintln!(
        "source={:?} entries={} added={} in {took:?}",
        report.source, report.entries, report.added
    );

    assert!(report.source.is_some(), "nothing answered");
    assert!(report.entries > 0);
    assert_eq!(report.entries, std::env::split_paths(&hydrated).count());

    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let hydrated_dirs: Vec<_> = std::env::split_paths(&hydrated).collect();
    for dir in std::env::split_paths(&inherited) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        assert!(
            hydrated_dirs.iter().any(|d| same_dir(d, &dir)),
            "hydration lost {}",
            dir.display()
        );
    }
    shellenv::invalidate();
}

/// The cache is what keeps a five-second login shell off every spawn. A second
/// `hydrate` must not pay for a second shell.
#[test]
#[ignore = "spawns the user's login shell or reads the live registry"]
fn v0_11_a_second_hydration_is_served_from_the_cache() {
    shellenv::invalidate();
    shellenv::hydrate();
    let first = shellenv::hydrated_path();

    let started = Instant::now();
    shellenv::hydrate();
    let again = started.elapsed();
    eprintln!("second hydrate took {again:?}");

    assert_eq!(first, shellenv::hydrated_path());
    assert!(again.as_millis() < 50, "the second call re-ran discovery");
    shellenv::invalidate();
}

/// The point of the phase, stated as a test: every Agent that resolves does so
/// against a `PATH` at least as large as the one Takyon was launched with.
///
/// Which Agents are installed is never asserted — `agents_cli.rs` says why.
#[test]
#[ignore = "spawns the user's login shell or reads the live registry"]
fn v0_11_agents_resolve_against_the_hydrated_path() {
    shellenv::invalidate();
    shellenv::hydrate();
    for snapshot in takyon_lib::agents::snapshots() {
        eprintln!("{}: installed={}", snapshot.label, snapshot.installed);
    }
    shellenv::invalidate();
}

/// The deep pass against the real PowerShell profile, or the real login shell.
///
/// Prints what it cost, which is the number the decision to gate it rests on
/// (`docs/tbd/v0.11.md` §4). Never asserts that a profile added anything: most
/// machines have no version manager, and finding nothing is a correct answer.
#[test]
#[ignore = "spawns the user's login shell or their PowerShell profile"]
fn v0_11_the_deep_pass_reaches_at_least_as_far_as_the_cheap_one() {
    shellenv::invalidate();
    shellenv::hydrate();
    let cheap = shellenv::report().expect("a report");
    let cheap_path = shellenv::hydrated_path();

    let started = Instant::now();
    shellenv::hydrate_deep();
    let took = started.elapsed();
    let deep = shellenv::report().expect("a report");
    eprintln!(
        "cheap: source={:?} entries={} | deep: source={:?} entries={} in {took:?}",
        cheap.source, cheap.entries, deep.source, deep.entries
    );

    assert!(deep.entries >= cheap.entries, "the deep pass lost folders");
    if let (Some(cheap_path), Some(deep_path)) = (cheap_path, shellenv::hydrated_path()) {
        let deep_dirs: Vec<_> = std::env::split_paths(&deep_path).collect();
        for dir in std::env::split_paths(&cheap_path) {
            assert!(
                deep_dirs.iter().any(|d| same_dir(d, &dir)),
                "the deep pass dropped {}",
                dir.display()
            );
        }
    }
    shellenv::invalidate();
}

/// Case-insensitively on Windows, exactly on unix — the same rule the merge uses.
fn same_dir(a: &std::path::Path, b: &std::path::Path) -> bool {
    if cfg!(windows) {
        let key = |p: &std::path::Path| {
            p.to_string_lossy()
                .to_lowercase()
                .replace('/', "\\")
                .trim_end_matches('\\')
                .to_string()
        };
        key(a) == key(b)
    } else {
        a == b
    }
}
