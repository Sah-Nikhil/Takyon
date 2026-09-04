//! Changes that have landed since the index was written (§5 task 5).
//!
//! The mapped index is immutable — rewriting a 2.5 MB file on every filesystem
//! event would put a disk write on the path a `git checkout` fires thousands of
//! times. Deltas accumulate here instead, and a query reads both: mapped hits
//! minus what has been deleted, plus what has appeared.
//!
//! Purely in memory and deliberately small. It is emptied by a rebuild, which is
//! what makes it a delta rather than a second index.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::entry::Query;
use crate::rank::{self, Haystack};

use super::FileHit;

/// How many additions accumulate before a rebuild is worth doing.
///
/// Not a correctness bound — the overlay stays correct at any size — but a
/// linear scan of it happens on every keystroke, and past a few thousand it
/// stops being cheaper than the file it is patching.
pub const REBUILD_THRESHOLD: usize = 4096;

/// Additions and deletions since the last rebuild.
#[derive(Default, Debug)]
pub struct Overlay {
    added: Vec<(PathBuf, bool)>,
    /// Lowercased full paths. A directory here removes everything under it.
    removed: HashSet<String>,
}

impl Overlay {
    pub fn new() -> Self {
        Overlay::default()
    }

    pub fn add(&mut self, path: PathBuf, is_dir: bool) {
        // A path can be deleted and recreated — an editor's atomic save is
        // exactly that — so adding has to lift the tombstone, not sit under it.
        self.removed.remove(&key(&path));
        if !self.added.iter().any(|(p, _)| p == &path) {
            self.added.push((path, is_dir));
        }
    }

    pub fn remove(&mut self, path: &Path) {
        self.added.retain(|(p, _)| p != path);
        self.removed.insert(key(path));
    }

    pub fn rename(&mut self, from: &Path, to: PathBuf, is_dir: bool) {
        self.remove(from);
        self.add(to, is_dir);
    }

    /// Whether a mapped entry has been deleted out from under the index.
    ///
    /// Checks every ancestor, not just the path: deleting a directory produces
    /// one event, and the thousands of files beneath it are gone with no event
    /// of their own.
    pub fn is_removed(&self, path: &Path) -> bool {
        if self.removed.is_empty() {
            return false;
        }
        let mut cursor = Some(path);
        while let Some(current) = cursor {
            if self.removed.contains(&key(current)) {
                return true;
            }
            cursor = current.parent();
        }
        false
    }

    /// Hits from the additions alone, matched exactly as the mapped index is.
    pub fn search(&self, needle: &str, limit: usize) -> Vec<FileHit> {
        if needle.is_empty() {
            return Vec::new();
        }
        let query = Query::new(needle);
        let mut hits: Vec<FileHit> = Vec::new();
        for (path, is_dir) in &self.added {
            if self.is_removed(path) {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(score) = score_name(name, needle, &query) else {
                continue;
            };
            hits.push(FileHit {
                path: path.clone(),
                is_dir: *is_dir,
                score,
            });
            if hits.len() >= limit {
                break;
            }
        }
        hits
    }

    pub fn additions(&self) -> usize {
        self.added.len()
    }

    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }

    /// Emptied by a rebuild, which has folded every delta into the file.
    pub fn clear(&mut self) {
        self.added.clear();
        self.removed.clear();
    }
}

/// One name against one needle, on the same two routes the mapped index takes.
///
/// Short needles are a prefix test and longer ones go through `rank::score`, so
/// an overlay hit and a mapped hit are directly comparable.
pub fn score_name(name: &str, needle: &str, query: &Query) -> Option<f32> {
    if needle.len() < super::live::MIN_TRIGRAM_LEN {
        let (name_bytes, needle_bytes) = (name.as_bytes(), needle.as_bytes());
        if name_bytes.len() < needle_bytes.len()
            || !name_bytes[..needle_bytes.len()].eq_ignore_ascii_case(needle_bytes)
        {
            return None;
        }
        return Some(if name_bytes.len() == needle_bytes.len() {
            rank::TIER_EXACT_NAME
        } else {
            rank::TIER_NAME_PREFIX
        });
    }
    rank::score_path(query, &Haystack::new(name, None))
}

fn key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlay() -> Overlay {
        let mut o = Overlay::new();
        o.add(PathBuf::from(r"C:\Data\notes.md"), false);
        o.add(PathBuf::from(r"C:\Data\HH"), true);
        o
    }

    /// The point of the whole file: something created a second ago is findable
    /// without the index being rewritten.
    #[test]
    fn v0_7_a_new_file_is_findable_before_any_rebuild() {
        let o = overlay();
        let hits = o.search("notes", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, PathBuf::from(r"C:\Data\notes.md"));
        assert!(!hits[0].is_dir);
    }

    /// Short needles take the prefix route here too, so a two-letter folder is
    /// findable the moment it is created.
    #[test]
    fn v0_7_a_short_needle_finds_a_new_folder() {
        let o = overlay();
        let hits = o.search("hh", 10);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].is_dir);
    }

    /// Deleting a directory produces one event. Everything under it is gone with
    /// no event of its own, so containment has to be what hides them.
    #[test]
    fn v0_7_removing_a_directory_removes_everything_under_it() {
        let mut o = Overlay::new();
        o.remove(Path::new(r"C:\Data\0Projects"));

        assert!(o.is_removed(Path::new(r"C:\Data\0Projects")));
        assert!(o.is_removed(Path::new(r"C:\Data\0Projects\Create\HH\bg.jpg")));
        assert!(!o.is_removed(Path::new(r"C:\Data\Other\bg.jpg")));
    }

    /// Windows paths are case-insensitive, and the events do not agree with the
    /// index on casing. A tombstone that missed would resurrect a deleted file.
    #[test]
    fn v0_7_removal_is_case_insensitive() {
        let mut o = Overlay::new();
        o.remove(Path::new(r"C:\Data\Notes.md"));
        assert!(o.is_removed(Path::new(r"c:\data\notes.md")));
    }

    /// An atomic save deletes and recreates. The recreation has to lift the
    /// tombstone or every editor save makes a file disappear from search.
    #[test]
    fn v0_7_recreating_a_removed_path_makes_it_findable_again() {
        let mut o = Overlay::new();
        let path = PathBuf::from(r"C:\Data\notes.md");
        o.add(path.clone(), false);
        o.remove(&path);
        assert!(o.search("notes", 10).is_empty());

        o.add(path.clone(), false);
        assert!(!o.is_removed(&path));
        assert_eq!(o.search("notes", 10).len(), 1);
    }

    /// A rename is a removal and an addition, and both halves have to land.
    #[test]
    fn v0_7_a_rename_moves_the_entry() {
        let mut o = Overlay::new();
        o.add(PathBuf::from(r"C:\Data\draft.md"), false);
        o.rename(
            Path::new(r"C:\Data\draft.md"),
            PathBuf::from(r"C:\Data\final.md"),
            false,
        );
        assert!(o.search("draft", 10).is_empty());
        assert_eq!(o.search("final", 10).len(), 1);
    }

    /// The same path arriving twice — Windows sends both a create and a modify —
    /// must not appear twice in the Palette.
    #[test]
    fn v0_7_the_same_path_added_twice_is_one_entry() {
        let mut o = Overlay::new();
        o.add(PathBuf::from(r"C:\Data\notes.md"), false);
        o.add(PathBuf::from(r"C:\Data\notes.md"), false);
        assert_eq!(o.additions(), 1);
        assert_eq!(o.search("notes", 10).len(), 1);
    }

    /// A rebuild folds the deltas into the file, so the overlay must not keep
    /// answering with them afterwards.
    #[test]
    fn v0_7_a_rebuild_empties_the_overlay() {
        let mut o = overlay();
        o.remove(Path::new(r"C:\Data\gone.txt"));
        assert!(!o.is_empty());
        o.clear();
        assert!(o.is_empty());
        assert!(o.search("notes", 10).is_empty());
        assert!(!o.is_removed(Path::new(r"C:\Data\gone.txt")));
    }
}
