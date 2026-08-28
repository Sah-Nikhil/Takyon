//! Recently-opened files, from the shell's Recent folder (ROADMAP v0.3 task 7).
//!
//! `%APPDATA%\Microsoft\Windows\Recent` holds one `.lnk` per recently-opened
//! document, so this is `lnk.rs`'s reader pointed at a different folder — no
//! index, no watcher, nothing to keep in sync.
//!
//! **These are the shell's recents, not ours.** They include files opened by any
//! application, so a document nobody opened *through Takyon* still appears. That
//! is the feature, and it is also why they rank below applications: an Entry the
//! user did not ask for must never take a top row.

use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::{Duration, Instant};

use crate::actions;
use crate::entry::{
    Action, Entry, EntryId, EntryKind, Query, Source, SourceId, SOURCE_SHORTLIST,
};
use crate::rank::{self, Haystack};
use crate::sources::apps::lnk;

pub const SOURCE_ID: SourceId = SourceId("recents");

/// How often the snapshot is rebuilt.
///
/// A few hundred shortcuts through COM is far too slow for the 20 ms query
/// budget, so a query never reads the folder — it answers from the last
/// snapshot. Recents are a convenience; twenty seconds stale costs nothing.
pub const REFRESH_EVERY: Duration = Duration::from_secs(20);

/// One recently-opened file.
#[derive(Clone, Debug)]
pub struct Recent {
    pub id: EntryId,
    pub title: String,
    pub subtitle: Option<String>,
    pub target: PathBuf,
    pub kind: EntryKind,
    pub hay: Haystack,
}

/// The shell's Recent folder.
pub fn recent_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|base| {
        PathBuf::from(base)
            .join("Microsoft")
            .join("Windows")
            .join("Recent")
    })
}

/// Is this a document worth offering?
///
/// The Recent folder also holds the jump-list databases, which are binary blobs
/// rather than shortcuts, and `desktop.ini`. Everything else is fair game — a
/// file the user opened is by definition one they wanted.
pub fn is_offerable(target: &Path) -> bool {
    let Some(name) = target.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    !name.eq_ignore_ascii_case("desktop.ini")
}

/// Build one Entry's worth of data from a shortcut target.
///
/// Split out so the shape is testable without a shell folder: the id rule, the
/// title and the kind are the parts worth asserting.
pub fn recent_from(target: &Path) -> Option<Recent> {
    if !is_offerable(target) {
        return None;
    }
    let title = target.file_name().and_then(|n| n.to_str())?.to_string();
    let kind = if target.is_dir() {
        EntryKind::Folder
    } else {
        EntryKind::File
    };
    // §2: the full path is the id for a File. Lowercased for the same reason an
    // App's is — one file reached two ways must not become two histories.
    let id = EntryId(target.to_string_lossy().to_lowercase());
    let stem = target.file_stem().and_then(|s| s.to_str());
    Some(Recent {
        hay: Haystack::new(&title, stem),
        id,
        title,
        subtitle: Some(target.to_string_lossy().to_string()),
        target: target.to_path_buf(),
        kind,
    })
}

/// The Recents Source.
pub struct RecentsSource {
    items: RwLock<Vec<Recent>>,
    /// When the snapshot was taken. `None` until the first read completes.
    read_at: RwLock<Option<Instant>>,
}

impl Default for RecentsSource {
    fn default() -> Self {
        Self::new()
    }
}

impl RecentsSource {
    pub fn new() -> Self {
        RecentsSource {
            items: RwLock::new(Vec::new()),
            read_at: RwLock::new(None),
        }
    }

    /// Re-read the Recent folder. Blocking; call it off the query path.
    pub fn refresh(&self) {
        let items = discover();
        if let Ok(mut guard) = self.items.write() {
            *guard = items;
        }
        if let Ok(mut guard) = self.read_at.write() {
            *guard = Some(Instant::now());
        }
    }

    pub fn len(&self) -> usize {
        self.items.read().map(|i| i.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Look up one recent file by id, for launching.
    pub fn find(&self, id: &EntryId) -> Option<Recent> {
        self.items.read().ok()?.iter().find(|r| &r.id == id).cloned()
    }

    /// How long ago the snapshot was taken, for diagnostics.
    pub fn age(&self) -> Option<Duration> {
        self.read_at.read().ok().and_then(|g| *g).map(|at| at.elapsed())
    }

    /// Populate without touching the shell. The seam the tests use.
    #[doc(hidden)]
    pub fn set_for_test(&self, items: Vec<Recent>) {
        if let Ok(mut guard) = self.items.write() {
            *guard = items;
        }
        if let Ok(mut guard) = self.read_at.write() {
            *guard = Some(Instant::now());
        }
    }
}

impl Source for RecentsSource {
    fn id(&self) -> SourceId {
        SOURCE_ID
    }

    fn query(&self, q: &Query, budget: Duration) -> Vec<Entry> {
        if q.is_empty() {
            return Vec::new();
        }
        let deadline = Instant::now() + budget;
        let Ok(items) = self.items.read() else {
            return Vec::new();
        };

        let mut out = Vec::new();
        for (i, item) in items.iter().enumerate() {
            if i % 64 == 0 && Instant::now() > deadline {
                break;
            }
            let Some(score) = rank::score(q, &item.hay) else {
                continue;
            };
            out.push(Entry {
                id: item.id.clone(),
                title: item.title.clone(),
                subtitle: item.subtitle.clone(),
                kind: item.kind,
                icon: None,
                score,
                actions: actions::for_file(),
            });
        }
        rank::order(out, SOURCE_SHORTLIST)
    }

    fn actions(&self, entry: &Entry) -> Vec<Action> {
        actions::for_entry(entry)
    }
}

/// Read every shortcut in the Recent folder.
fn discover() -> Vec<Recent> {
    let Some(dir) = recent_dir() else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    // Depth 1: the Recent folder is flat, and `AutomaticDestinations` beneath it
    // holds jump-list databases rather than shortcuts.
    for link in lnk::find_links(&dir) {
        let Some(sc) = lnk::read(&link) else { continue };
        let Some(recent) = recent_from(&sc.target) else {
            continue;
        };
        if seen.insert(recent.id.clone()) {
            out.push(recent);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_3_a_recent_file_is_titled_by_its_filename_and_keyed_by_its_path() {
        let r = recent_from(Path::new(r"C:\Users\me\Documents\Report Q3.docx")).unwrap();
        assert_eq!(r.title, "Report Q3.docx");
        assert_eq!(r.id.as_str(), r"c:\users\me\documents\report q3.docx");
        assert_eq!(r.kind, EntryKind::File);
    }

    /// The Recent folder's own furniture is not a document.
    #[test]
    fn v0_3_shell_furniture_is_not_offered() {
        assert!(!is_offerable(Path::new(r"C:\Users\me\Recent\desktop.ini")));
        assert!(is_offerable(Path::new(r"C:\Users\me\notes.txt")));
    }

    /// A recent document must never outrank an application, however well it
    /// matches — §3's kind rule, checked where the Source meets the ranker.
    #[test]
    fn v0_3_a_recent_file_ranks_below_an_application() {
        let source = RecentsSource::new();
        source.set_for_test(vec![
            recent_from(Path::new(r"C:\docs\photoshop.txt")).unwrap()
        ]);
        let entries = source.query(&Query::new("photoshop"), Duration::from_millis(20));
        assert_eq!(entries.len(), 1);
        assert!(entries[0].kind.tier() > EntryKind::App.tier());
    }

    /// A document cannot be run as administrator, and offering it teaches the
    /// user the menu lies (the same rule `actions.rs` applies to a packaged app).
    #[test]
    fn v0_3_a_recent_file_is_not_offered_run_as_administrator() {
        let source = RecentsSource::new();
        source.set_for_test(vec![recent_from(Path::new(r"C:\docs\notes.txt")).unwrap()]);
        let entries = source.query(&Query::new("notes"), Duration::from_millis(20));
        assert!(!entries[0].actions.contains(&actions::RUN_AS_ADMIN));
        assert!(entries[0].actions.contains(&actions::REVEAL));
    }

    #[test]
    fn v0_3_an_empty_query_returns_no_recents() {
        let source = RecentsSource::new();
        source.set_for_test(vec![recent_from(Path::new(r"C:\docs\notes.txt")).unwrap()]);
        assert!(source.query(&Query::new(""), Duration::from_millis(20)).is_empty());
    }
}
