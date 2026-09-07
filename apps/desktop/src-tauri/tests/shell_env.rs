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
