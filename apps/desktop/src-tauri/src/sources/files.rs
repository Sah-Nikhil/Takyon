//! Files and folders, from the index (§5 tasks 10 and 11).
//!
//! Two doors, and the Bang is the primary one. `!e` is always file search;
//! Bangless file Entries are a setting, default off, and when on they sit below
//! applications — a document the user did not ask for must never take a top row,
//! which `EntryKind`'s tiers already enforce.
//!
//! An `EntryId` here is the **lowercased full path**, the same rule the Recents
//! Source follows (§2). Windows paths are case-insensitive, so the id is also a
//! usable path, which is what lets activation find the file without a second
//! index.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::actions;
use crate::entry::{
    Action, Entry, EntryId, EntryKind, Query, Source, SourceId, MAX_ENTRIES, SOURCE_SHORTLIST,
};
use crate::frecency::Frecency;
use crate::index::live::WalkIndex;
use crate::index::wsearch::WindowsSearch;
use crate::index::{FileHit, FileIndex};

pub const SOURCE_ID: SourceId = SourceId("files");

/// The file Source, and the `!e` Mode behind the same index.
pub struct FileSource {
    index: Arc<WalkIndex>,
    /// Whether file Entries join Bangless results (task 11). Default off, and
    /// atomic because it is read on the keystroke path.
    bangless: AtomicBool,
    /// Whether Windows Search answers for locations outside the roots (task 9).
    /// Default off — measured at 10–72 ms, which is the whole budget.
    fallback: AtomicBool,
}

impl FileSource {
    pub fn new(index: Arc<WalkIndex>) -> Self {
        FileSource {
            index,
            bangless: AtomicBool::new(false),
            fallback: AtomicBool::new(false),
        }
    }

    pub fn set_bangless(&self, on: bool) {
        self.bangless.store(on, Ordering::Relaxed);
    }

    pub fn bangless_enabled(&self) -> bool {
        self.bangless.load(Ordering::Relaxed)
    }

    pub fn set_fallback(&self, on: bool) {
        self.fallback.store(on, Ordering::Relaxed);
    }

    pub fn fallback_enabled(&self) -> bool {
        self.fallback.load(Ordering::Relaxed)
    }

    pub fn index(&self) -> &Arc<WalkIndex> {
        &self.index
    }

    /// What `!e` answers with (task 10).
    ///
    /// The local index first and alone on the fast path. The fallback is asked
    /// only when it is on **and** the index came up short, so a machine with it
    /// enabled still pays nothing on the queries the index can answer.
    pub fn mode_entries(&self, needle: &str, limit: usize) -> Vec<Entry> {
        let mut hits = self.index.search(needle, limit);
        if self.fallback_enabled() && hits.len() < limit {
            let already: Vec<String> = hits.iter().map(|h| key(&h.path)).collect();
            // Appended, never merged by score: a fallback hit is from outside the
            // indexed roots, so it is a wider guess and belongs below.
            for hit in WindowsSearch::search(needle, limit - hits.len()) {
                if !already.contains(&key(&hit.path)) {
                    hits.push(hit);
                }
            }
        }
        hits.iter().map(entry_of).collect()
    }

    /// What `!e` shows with no query: the recents list Takyon owns (TBC-0010).
    ///
    /// Files and folders only. Applications pass through the same table, and
    /// Frecency already ranks those better than a chronological list would.
    pub fn recent_entries(frecency: &Frecency, limit: usize) -> Vec<Entry> {
        frecency
            .opened(&[EntryKind::File, EntryKind::Folder], limit)
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let mut entry = entry_at(&row.path, row.path.is_dir());
                // Chronological, so the score only has to preserve the order the
                // query returned. Nothing here competes with a matched Entry.
                entry.score = (limit - i) as f32;
                entry
            })
            .collect()
    }
}

impl Source for FileSource {
    fn id(&self) -> SourceId {
        SOURCE_ID
    }

    fn query(&self, q: &Query, budget: Duration) -> Vec<Entry> {
        // Off by default (task 11). The Bang is the door; this is the setting.
        if q.is_empty() || !self.bangless_enabled() {
            return Vec::new();
        }
        let deadline = Instant::now() + budget;
        // Never the fallback on the Bangless path, whatever the setting says:
        // ADR-0002 aside, 10–72 ms against a 20 ms budget cannot ride here.
        let hits = self.index.search(&q.needle, SOURCE_SHORTLIST);
        if Instant::now() > deadline {
            return Vec::new();
        }
        hits.iter().map(entry_of).collect()
    }

    fn actions(&self, entry: &Entry) -> Vec<Action> {
        actions::for_entry(entry)
    }
}

/// One index hit as an Entry.
pub fn entry_of(hit: &FileHit) -> Entry {
    let mut entry = entry_at(&hit.path, hit.is_dir);
    entry.score = hit.score;
    entry
}

/// One path as an Entry, before it has a score.
fn entry_at(path: &Path, is_dir: bool) -> Entry {
    let title = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    Entry {
        // §2: the full path, lowercased, exactly as the Recents Source spells it.
        // One file reached two ways must not become two histories.
        id: EntryId(key(path)),
        title,
        subtitle: Some(path.to_string_lossy().to_string()),
        kind: if is_dir {
            EntryKind::Folder
        } else {
            EntryKind::File
        },
        icon: None,
        score: 0.0,
        actions: actions::for_file(),
        version: None,
    }
}

/// How many Entries `!e` returns. The Palette shows no more than this anyway.
pub const MODE_LIMIT: usize = MAX_ENTRIES;

fn key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::roots::Roots;
    use std::path::PathBuf;

    /// Labelled per test: these run in parallel in one process, and a shared
    /// directory means one test deleting the tree another is walking.
    fn source(label: &str) -> (PathBuf, PathBuf, Arc<FileSource>) {
        let base = std::env::temp_dir()
            .join("takyon-filesource")
            .join(format!("{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let tree = base.join("tree");
        std::fs::create_dir_all(tree.join("notes")).unwrap();
        std::fs::write(tree.join("readme.md"), "x").unwrap();
        std::fs::write(tree.join("notes").join("plan.md"), "x").unwrap();

        let index = Arc::new(WalkIndex::load(
            base.join("index"),
            Roots {
                include: vec![tree.clone()],
                exclude: Vec::new(),
            },
        ));
        index.rebuild().unwrap();
        (base, tree, Arc::new(FileSource::new(index)))
    }

    /// A file Entry's id is its path, so activation can find it without a
    /// second index — and so Frecency keys on the file rather than its name.
    #[test]
    fn v0_7_a_file_entry_is_identified_by_its_path() {
        let (base, tree, files) = source("id");
        let entries = files.mode_entries("readme", MODE_LIMIT);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, "readme.md");
        assert_eq!(
            entries[0].id,
            EntryId(tree.join("readme.md").to_string_lossy().to_lowercase())
        );
        assert_eq!(entries[0].kind, EntryKind::File);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A folder is its own Kind, and it sorts above a file — you can open one.
    #[test]
    fn v0_7_a_folder_entry_carries_the_folder_kind() {
        let (base, _tree, files) = source("folder");
        let entries = files.mode_entries("notes", MODE_LIMIT);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, EntryKind::Folder);
        assert!(EntryKind::Folder.tier() < EntryKind::File.tier());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Every file Entry offers exactly open, reveal and copy path (task 10).
    #[test]
    fn v0_7_a_file_entry_offers_the_three_file_actions() {
        let (base, _tree, files) = source("actions");
        let entry = files.mode_entries("readme", MODE_LIMIT).remove(0);
        assert_eq!(
            entry.actions,
            vec![actions::OPEN, actions::REVEAL, actions::COPY_PATH]
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Default off (task 11). The Bang is the door; Bangless is the setting.
    #[test]
    fn v0_7_bangless_file_entries_are_off_until_switched_on() {
        let (base, _tree, files) = source("toggle");
        let q = Query::new("readme");

        assert!(!files.bangless_enabled());
        assert!(files.query(&q, Duration::from_millis(20)).is_empty());

        files.set_bangless(true);
        assert!(!files.query(&q, Duration::from_millis(20)).is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Bangless file Entries sit below applications, always. `EntryKind`'s tiers
    /// carry it, and this pins that they were not given an App-tier Kind.
    #[test]
    fn v0_7_bangless_file_entries_rank_below_applications() {
        let (base, _tree, files) = source("tier");
        files.set_bangless(true);
        for entry in files.query(&Query::new("readme"), Duration::from_millis(20)) {
            assert!(entry.kind.tier() > EntryKind::App.tier());
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    /// `!e` with no query shows what Takyon opened, newest first, and never the
    /// applications sharing that table (TBC-0010).
    #[test]
    fn v0_7_an_empty_file_bang_shows_the_owned_recents() {
        let (base, tree, _files) = source("recents");
        let frecency = Frecency::open(None).unwrap();
        let doc = tree.join("readme.md");
        frecency
            .record_opened_at(
                &EntryId(doc.to_string_lossy().to_lowercase()),
                &doc.to_string_lossy(),
                EntryKind::File,
                100,
            )
            .unwrap();
        frecency
            .record_opened_at(&EntryId("some.exe".into()), "some.exe", EntryKind::App, 200)
            .unwrap();

        let entries = FileSource::recent_entries(&frecency, MODE_LIMIT);
        assert_eq!(entries.len(), 1, "the application must not appear");
        assert_eq!(entries[0].title, "readme.md");
        let _ = std::fs::remove_dir_all(&base);
    }
}
