//! The live index: mapped at boot, rebuilt in the background (§5 tasks 4 and 8).
//!
//! **Never re-walk at startup.** An existing index is mapped and served
//! immediately, which costs a page fault rather than the 25-odd seconds a walk
//! takes. Rebuilding happens off the startup path, and the UI is told which of
//! the two states it is in — see [`IndexStatus`].
//!
//! Files are named `<generation>.tkx` rather than one fixed name because Windows
//! will not replace a file that is still mapped. A new generation is written
//! beside the old one and swapped in; the old one is deleted once nothing holds
//! it.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::entry::Query;
use crate::rank::{self, Haystack};

use super::roots::Roots;
use super::store::Index;
use super::{walker, FileHit, FileIndex, IndexStatus};

/// Shortest needle the trigram postings can answer.
///
/// Below this every posting list is nearly the whole index, so the intersection
/// costs more than it saves — §5 sends short queries down their own path.
pub const MIN_TRIGRAM_LEN: usize = 3;

/// The walk-backed [`FileIndex`].
pub struct WalkIndex {
    dir: PathBuf,
    roots: RwLock<Roots>,
    /// `None` until a first index exists. A query against `None` returns nothing
    /// and `status` says `Building`, so the UI never shows an empty list as if it
    /// were an answer.
    index: RwLock<Option<Arc<Index>>>,
    status: RwLock<IndexStatus>,
}

impl WalkIndex {
    /// Map whatever is already on disk. Does not walk.
    ///
    /// Status is `Ready` when something mapped and `Building` when nothing did,
    /// so a first run and a returning one are distinguishable before any query.
    pub fn load(dir: PathBuf, roots: Roots) -> WalkIndex {
        let index = load_latest(&dir).map(Arc::new);
        let status = if index.is_some() {
            IndexStatus::Ready
        } else {
            IndexStatus::Building { pct: 0 }
        };
        WalkIndex {
            dir,
            roots: RwLock::new(roots),
            index: RwLock::new(index),
            status: RwLock::new(status),
        }
    }

    /// Whether an index was mapped at load. The boot path checks this rather than
    /// walking unconditionally.
    pub fn is_loaded(&self) -> bool {
        self.index.read().is_ok_and(|i| i.is_some())
    }

    pub fn entry_count(&self) -> u32 {
        self.index
            .read()
            .ok()
            .and_then(|i| i.as_ref().map(|i| i.entry_count()))
            .unwrap_or(0)
    }

    /// Walk the roots and swap the result in. Blocking; callers run it off the
    /// startup path.
    pub fn rebuild(&self) -> std::io::Result<()> {
        let roots = self.roots.read().map(|r| r.clone()).unwrap_or(Roots {
            include: Vec::new(),
            exclude: Vec::new(),
        });
        let generation = self.generation() + 1;
        let bytes = walker::walk(&roots).finish(generation);

        std::fs::create_dir_all(&self.dir)?;
        // Written under a temp name and renamed, so a crash mid-write leaves the
        // previous index intact rather than a truncated one beside it.
        let temp = self.dir.join(format!("{generation}.tkx.part"));
        let final_path = self.dir.join(format!("{generation}.tkx"));
        std::fs::write(&temp, &bytes)?;
        std::fs::rename(&temp, &final_path)?;

        if let Some(fresh) = Index::open(&final_path) {
            if let Ok(mut slot) = self.index.write() {
                *slot = Some(Arc::new(fresh));
            }
            self.set_status(IndexStatus::Ready);
        }
        sweep(&self.dir, generation);
        Ok(())
    }

    pub fn set_status(&self, status: IndexStatus) {
        if let Ok(mut slot) = self.status.write() {
            *slot = status;
        }
    }

    /// Replace the roots. Takes effect at the next rebuild, not retroactively.
    pub fn set_roots(&self, roots: Roots) {
        if let Ok(mut slot) = self.roots.write() {
            *slot = roots;
        }
    }
}

impl FileIndex for WalkIndex {
    /// Candidates from the postings, then the real matcher (§5).
    ///
    /// The postings are an over-approximation, so every candidate is scored by
    /// `rank::score` against its own filename — the same ladder applications are
    /// matched by, so a file and an app that match equally well score equally.
    fn search(&self, q: &str, limit: usize) -> Vec<FileHit> {
        let needle = q.trim().to_lowercase();
        if needle.len() < MIN_TRIGRAM_LEN {
            // Short queries take the recent-set path instead of scanning
            // everything (§5). That set arrives with task 12; until then a short
            // needle has no file answer rather than a slow one.
            return Vec::new();
        }
        let Ok(guard) = self.index.read() else {
            return Vec::new();
        };
        let Some(index) = guard.as_ref() else {
            return Vec::new();
        };

        let query = Query::new(&needle);
        let mut hits: Vec<FileHit> = index
            .candidates(&needle)
            .into_iter()
            .filter_map(|id| {
                let hay = Haystack::new(index.name(id), None);
                let score = rank::score(&query, &hay)?;
                Some(FileHit {
                    path: index.path(id),
                    is_dir: index.is_dir(id),
                    score,
                })
            })
            .collect();

        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                // Ties broken by path so two runs agree. Without it the order is
                // whatever the postings happened to hold, which the Stability
                // rule would then see move under it.
                .then_with(|| a.path.cmp(&b.path))
        });
        hits.truncate(limit);
        hits
    }

    fn generation(&self) -> u64 {
        self.index
            .read()
            .ok()
            .and_then(|i| i.as_ref().map(|i| i.generation()))
            .unwrap_or(0)
    }

    fn status(&self) -> IndexStatus {
        self.status
            .read()
            .map(|s| *s)
            .unwrap_or(IndexStatus::Building { pct: 0 })
    }
}

/// Map the highest generation in `dir` that opens.
///
/// Highest that *opens*, not highest that exists: a newer file refused for a
/// format bump or a truncated write must fall back to the older one rather than
/// leaving the user with nothing.
pub fn load_latest(dir: &Path) -> Option<Index> {
    let mut generations = generations(dir);
    generations.sort_unstable_by_key(|g| std::cmp::Reverse(g.0));
    generations
        .into_iter()
        .find_map(|(_, path)| Index::open(&path))
}

/// Delete every index file except `keep`.
///
/// A failure here is ignored on purpose: the usual cause is the previous
/// generation still being mapped by this process, and it will be gone at the next
/// sweep. A leaked file is cheaper than a failed rebuild.
fn sweep(dir: &Path, keep: u64) {
    for (generation, path) in generations(dir) {
        if generation != keep {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn generations(dir: &Path) -> Vec<(u64, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let stem = path.file_name()?.to_str()?.strip_suffix(".tkx")?;
            Some((stem.parse::<u64>().ok()?, path))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("takyon-live")
            .join(format!("{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A tree to walk, plus the directory the index is written to.
    fn fixture(label: &str) -> (PathBuf, PathBuf, Roots) {
        let root = scratch(&format!("{label}-tree"));
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src").join("bangs.rs"), "x").unwrap();
        std::fs::write(root.join("README.md"), "x").unwrap();
        let store = scratch(&format!("{label}-store"));
        let roots = Roots {
            include: vec![root.clone()],
            exclude: Vec::new(),
        };
        (root, store, roots)
    }

    /// The boot path's whole promise: a second load maps what the first wrote and
    /// answers without walking anything.
    #[test]
    fn v0_7_an_index_survives_a_restart_without_rewalking() {
        let (tree, store, roots) = fixture("restart");
        let first = WalkIndex::load(store.clone(), roots.clone());
        assert!(!first.is_loaded(), "nothing is on disk yet");
        first.rebuild().unwrap();
        let built = first.entry_count();
        drop(first);

        let second = WalkIndex::load(store.clone(), roots);
        assert!(second.is_loaded(), "the written index must map at load");
        assert_eq!(second.entry_count(), built);
        assert_eq!(second.status(), IndexStatus::Ready);
        assert!(!second.search("bangs", 10).is_empty());

        let _ = std::fs::remove_dir_all(&tree);
        let _ = std::fs::remove_dir_all(&store);
    }

    /// Before a first walk there is nothing to serve, and the status has to say
    /// so — an empty list and "no such file" look identical and one is a lie.
    #[test]
    fn v0_7_an_unbuilt_index_reports_building_rather_than_empty_results() {
        let store = scratch("unbuilt");
        let index = WalkIndex::load(
            store.clone(),
            Roots {
                include: Vec::new(),
                exclude: Vec::new(),
            },
        );
        assert_eq!(index.status(), IndexStatus::Building { pct: 0 });
        assert!(index.search("anything", 10).is_empty());
        let _ = std::fs::remove_dir_all(&store);
    }

    /// A rebuild bumps the generation, and only the newest file is kept — old
    /// generations would otherwise accumulate one full index per rescan.
    #[test]
    fn v0_7_a_rebuild_bumps_the_generation_and_sweeps_the_old_file() {
        let (tree, store, roots) = fixture("sweep");
        let index = WalkIndex::load(store.clone(), roots);
        index.rebuild().unwrap();
        assert_eq!(index.generation(), 1);
        index.rebuild().unwrap();
        assert_eq!(index.generation(), 2);

        let left: Vec<u64> = generations(&store).into_iter().map(|(g, _)| g).collect();
        assert_eq!(left, vec![2]);

        let _ = std::fs::remove_dir_all(&tree);
        let _ = std::fs::remove_dir_all(&store);
    }

    /// A newer file that will not open must not shadow an older one that will.
    /// Losing the whole index to one bad write is the expensive version of this
    /// bug, and it only shows up after a crash.
    #[test]
    fn v0_7_an_unreadable_newer_generation_falls_back_to_an_older_one() {
        let (tree, store, roots) = fixture("fallback");
        let index = WalkIndex::load(store.clone(), roots);
        index.rebuild().unwrap();
        std::fs::write(store.join("9.tkx"), b"truncated nonsense").unwrap();

        let reopened = load_latest(&store).expect("the good generation still opens");
        assert_eq!(reopened.generation(), 1);

        let _ = std::fs::remove_dir_all(&tree);
        let _ = std::fs::remove_dir_all(&store);
    }

    /// Under three characters there is no trigram to intersect, so this path
    /// declines rather than scanning every entry against the 20 ms budget.
    #[test]
    fn v0_7_a_short_query_is_declined_by_the_trigram_path() {
        let (tree, store, roots) = fixture("short");
        let index = WalkIndex::load(store.clone(), roots);
        index.rebuild().unwrap();

        assert!(index.search("ba", 10).is_empty());
        assert!(!index.search("ban", 10).is_empty());

        let _ = std::fs::remove_dir_all(&tree);
        let _ = std::fs::remove_dir_all(&store);
    }

    /// Candidates are a superset; the matcher is what decides. A trigram that
    /// coincidentally intersects must not survive as a hit.
    #[test]
    fn v0_7_candidates_are_verified_by_the_real_matcher() {
        let (tree, store, roots) = fixture("verify");
        let index = WalkIndex::load(store.clone(), roots);
        index.rebuild().unwrap();

        let hits = index.search("readme", 10);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].path.ends_with("README.md"));
        assert!(!hits[0].is_dir);

        let _ = std::fs::remove_dir_all(&tree);
        let _ = std::fs::remove_dir_all(&store);
    }
}
