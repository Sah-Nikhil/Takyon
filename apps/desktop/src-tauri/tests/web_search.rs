//! Web search against the real network and the real disk (v0.9).
//!
//! The unit tests parse captured bodies and stored blobs; this reaches the
//! internet. It is the only layer that catches Brave changing its response
//! shape, WinHTTP refusing a request, or extraction meeting a real page rather
//! than a fixture.
//!
//! **Machine-dependent by construction.** Nothing asserts what the web says —
//! only that a request completes, that its answer is coherent, and that the
//! guards hold on paths that do not need a network at all.

mod common;

use takyon_lib::search::{self, extract, fetch, key, SearchError};

/// The key store, against a real directory rather than an in-memory one.
///
/// DPAPI is per Windows account and cannot be faked, so this is the only place
/// the wrap and unwrap are exercised end to end on disk.
#[test]
fn v0_9_a_key_survives_a_round_trip_through_a_real_directory() {
    let temp = common::TempDir::new("takyon-web-key");
    assert!(!key::present(temp.path()));

    key::store(temp.path(), "BSA-integration-token").expect("store");
    assert!(key::present(temp.path()));
    assert_eq!(key::load(temp.path()).as_deref(), Some("BSA-integration-token"));
    assert_eq!(key::hint(temp.path()).as_deref(), Some("…oken"));

    // The file is what another account would find. It must not be the key.
    let raw = std::fs::read(key::key_file(temp.path())).expect("the blob");
    assert!(!raw.windows(21).any(|w| w == b"BSA-integration-token"));

    key::clear(temp.path()).expect("clear");
    assert!(!key::present(temp.path()));
}

/// No key means no request. Checked here because the unit test can only assert
/// the provider's own guard, not that nothing reached the network.
#[test]
fn v0_9_a_search_without_a_key_never_leaves_the_machine() {
    let started = std::time::Instant::now();
    let error = search::provider()
        .search("ferrari in f1", "")
        .expect_err("an empty key cannot search");
    assert_eq!(error, SearchError::NoKey);
    // A DNS lookup alone costs more than this. Failing instantly is the proof
    // that no socket was opened.
    assert!(started.elapsed() < std::time::Duration::from_millis(50));
}

/// The URL guard, against the shapes a hostile page would actually carry.
#[test]
fn v0_9_only_web_urls_survive_the_parser() {
    for url in [
        "file:///C:/Windows/System32/cmd.exe",
        "javascript:alert(1)",
        "data:text/html,<script>x</script>",
        "https://user:password@example.com/",
        "ftp://example.com/x",
    ] {
        assert!(fetch::parse_url(url).is_none(), "{url} was accepted");
    }
    assert!(fetch::parse_url("https://example.com/a?b=c").is_some());
}

/// A real page, read over the real network, reduced to real prose.
///
/// `#[ignore]` because it needs a network: run it by hand with
/// `cargo test --test web_search -- --ignored --nocapture`. It is what catches
/// WinHTTP and extraction disagreeing with the actual web.
#[test]
#[ignore]
fn v0_9_a_real_page_is_fetched_and_reduced_to_prose() {
    let response = fetch::get_url("https://example.com/").expect("example.com answers");
    assert_eq!(response.status, 200);
    assert!(response.body.contains("<html"), "no HTML came back");
    assert_eq!(
        extract::title(&response.body).as_deref(),
        Some("Example Domain")
    );

    // example.com is deliberately tiny — below the prose floor — so this asserts
    // the floor holds on a real document rather than only on a fixture.
    assert_eq!(extract::readable(&response.body), None);
}

/// Several pages at once, with the total budget enforced.
///
/// `#[ignore]`: needs a network. Reads a host that does not resolve alongside
/// real ones, because a dead source in the list is the ordinary case.
#[test]
#[ignore]
fn v0_9_pages_are_read_in_parallel_and_a_dead_one_does_not_stop_the_rest() {
    let urls = vec![
        "https://example.com/".to_string(),
        "https://not-a-real-host-takyon-test.invalid/".to_string(),
        "https://www.rust-lang.org/".to_string(),
    ];
    let started = std::time::Instant::now();
    let bodies = fetch::pages(&urls);

    assert_eq!(bodies.len(), 3);
    assert!(bodies[0].is_ok(), "example.com: {:?}", bodies[0]);
    assert!(bodies[1].is_err(), "a host that does not exist answered");
    assert!(bodies[2].is_ok(), "rust-lang.org: {:?}", bodies[2]);
    // Three sequential requests, one of them a DNS failure, would exceed this.
    assert!(started.elapsed() < fetch::DEADLINE + std::time::Duration::from_secs(3));
}

/// A whole search, against the key this machine actually holds.
///
/// `#[ignore]` twice over: it needs a network and it spends a Brave request.
/// Skips itself rather than failing when no key is stored, because that is the
/// state a machine that has never used `!s` is in.
#[test]
#[ignore]
fn v0_9_a_real_search_returns_coherent_hits() {
    let Some(dir) = takyon_lib::identity::data_dir() else {
        eprintln!("no data directory on this machine; skipped");
        return;
    };
    let Some(stored) = key::load(&dir) else {
        eprintln!("no Brave key stored; skipped");
        return;
    };

    let hits = search::provider()
        .search("ferrari formula one", &stored)
        .expect("Brave answered");
    assert!(!hits.is_empty(), "a well-known query returned nothing");
    assert!(hits.len() <= search::MAX_HITS);

    for hit in &hits {
        assert!(!hit.title.is_empty(), "a hit had no title");
        assert!(fetch::parse_url(&hit.url).is_some(), "{}", hit.url);
        // Highlight markup rendered raw is the failure this catches.
        assert!(!hit.title.contains("<strong>"), "{}", hit.title);
    }
}
