//! The file index against a real filesystem (§5, slice 1).
//!
//! Everything here touches disk, which is why it is not a unit test. The shaped
//! tests build their own tree so they assert exactly; the measurement at the foot
//! walks this machine's real roots and is `#[ignore]`d, because what it reports
//! depends on whose machine it runs on.

mod common;

use std::path::Path;
use std::time::Instant;

use takyon_lib::index::live::WalkIndex;
use takyon_lib::index::roots::{self, Roots};
use takyon_lib::index::{FileIndex, IndexStatus};

use common::TempDir;

/// Write a tree with a known shape: two projects, a build directory that must be
/// skipped, and a file buried four levels down.
fn seed(root: &Path) {
    std::fs::create_dir_all(root.join("alpha").join("src")).unwrap();
    std::fs::create_dir_all(root.join("alpha").join("node_modules").join("left-pad")).unwrap();
    std::fs::create_dir_all(root.join("beta").join("a").join("b").join("c")).unwrap();
    std::fs::write(root.join("alpha").join("src").join("palette.rs"), "x").unwrap();
    std::fs::write(root.join("alpha").join("Cargo.toml"), "x").unwrap();
    std::fs::write(
        root.join("alpha").join("node_modules").join("left-pad").join("index.js"),
        "x",
    )
    .unwrap();
    std::fs::write(root.join("beta").join("a").join("b").join("c").join("buried.md"), "x").unwrap();
}

fn built(temp: &TempDir, label: &str) -> WalkIndex {
    let tree = temp.path().join(label);
    std::fs::create_dir_all(&tree).unwrap();
    seed(&tree);

    let index = WalkIndex::load(
        temp.path().join(format!("{label}-index")),
        Roots {
            include: vec![tree],
            exclude: roots::DEFAULT_EXCLUDES.iter().map(|s| s.to_string()).collect(),
        },
    );
    index.rebuild().expect("the index writes");
    index
}

/// The whole of slice 1 in one assertion: walked, written, mapped, queried.
#[test]
fn v0_7_a_walked_tree_is_searchable_by_name() {
    let temp = TempDir::new("index-search");
    let index = built(&temp, "tree");

    assert_eq!(index.status(), IndexStatus::Ready);
    assert_eq!(index.generation(), 1);

    let hits = index.search("palette", 10);
    assert_eq!(hits.len(), 1, "one file is named palette.rs");
    assert!(hits[0].path.ends_with(r"alpha\src\palette.rs"));
    assert!(!hits[0].is_dir);
}

/// Depth is unbounded by design, so a file four levels down is found on the same
/// terms as one at the top.
#[test]
fn v0_7_depth_does_not_hide_a_file() {
    let temp = TempDir::new("index-depth");
    let index = built(&temp, "tree");

    let hits = index.search("buried", 10);
    assert_eq!(hits.len(), 1);
    assert!(hits[0].path.ends_with(r"beta\a\b\c\buried.md"));
}

/// A directory is a hit in its own right — `!e` opens folders too.
#[test]
fn v0_7_a_folder_is_indexed_as_well_as_a_file() {
    let temp = TempDir::new("index-folder");
    let index = built(&temp, "tree");

    let hits = index.search("alpha", 10);
    assert!(hits.iter().any(|h| h.is_dir), "alpha is a directory");
}

/// The exclusion list is applied during the walk, so nothing under
/// `node_modules` exists to be found afterwards.
#[test]
fn v0_7_nothing_under_an_excluded_directory_is_indexed() {
    let temp = TempDir::new("index-exclude");
    let index = built(&temp, "tree");

    assert!(index.search("left-pad", 10).is_empty());
    assert!(index.search("index.js", 10).is_empty());
    // The exclusion is by name, so an ordinary file is untouched by it.
    assert!(!index.search("cargo", 10).is_empty());
}

/// The boot path: a second `load` maps what the first wrote and never walks.
#[test]
fn v0_7_a_second_load_maps_the_existing_index() {
    let temp = TempDir::new("index-boot");
    let index = built(&temp, "tree");
    let count = index.entry_count();
    drop(index);

    let store = temp.path().join("tree-index");
    let reopened = WalkIndex::load(
        store,
        Roots {
            include: Vec::new(),
            exclude: Vec::new(),
        },
    );
    assert!(reopened.is_loaded());
    assert_eq!(reopened.entry_count(), count);
    assert_eq!(reopened.status(), IndexStatus::Ready);
    // Roots are empty, so anything found came from the mapped file rather than a
    // walk this process performed.
    assert!(!reopened.search("palette", 10).is_empty());
}

/// This machine's real roots, measured rather than asserted.
///
/// `cargo test --test index_disk -- --ignored --nocapture` prints walk time,
/// entry count, index size and query latency — the four numbers TBC-0005 and the
/// exit criteria are stated in. Ignored: every one depends on whose disk it runs on.
#[test]
#[ignore]
fn v0_7_measure_the_real_roots() {
    let temp = TempDir::new("index-real");
    let defaults = roots::defaults();
    println!("roots:");
    for root in &defaults.include {
        println!("  {}", root.display());
    }

    let index = WalkIndex::load(temp.path().join("index"), defaults);
    let started = Instant::now();
    index.rebuild().expect("the index writes");
    let walk_ms = started.elapsed().as_millis();

    let bytes = std::fs::read_dir(temp.path().join("index"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.metadata().ok().map(|m| m.len()))
        .sum::<u64>();

    println!("entries:    {}", index.entry_count());
    println!("walk:       {walk_ms} ms");
    println!("index size: {:.1} MB", bytes as f64 / 1_048_576.0);

    let mut worst = 0u128;
    let mut total = 0u128;
    let needles = [
        "main", "readme", "cargo", "index", "config", "package", "test", "src", "takyon", "notes",
    ];
    for needle in needles {
        let at = Instant::now();
        let hits = index.search(needle, 12);
        let us = at.elapsed().as_micros();
        total += us;
        worst = worst.max(us);
        println!("  {needle:<8} {:>3} hits  {us:>6} us", hits.len());
    }
    println!("query mean: {} us", total / needles.len() as u128);
    println!("query worst: {worst} us");
}
