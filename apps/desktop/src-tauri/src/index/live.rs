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
use std::sync::{Arc, Mutex, RwLock};

use crate::entry::Query;
use crate::rank::{self, Haystack};

use super::overlay::{Overlay, REBUILD_THRESHOLD};
use super::roots::Roots;
use super::store::{Builder, Index};
use super::watcher::{Change, Watcher};
use super::{walker, FileHit, FileIndex, IndexStatus};

/// Shortest needle the trigram postings can answer.
///
/// Below this every posting list is nearly the whole index, so the intersection
/// costs more than it saves — §5 sends short queries down their own path.
pub const MIN_TRIGRAM_LEN: usize = 3;

/// How often the overlay is checked against [`REBUILD_THRESHOLD`].
///
/// A timer rather than a per-event check: the threshold is about how large the
/// delta has grown, which no single event decides, and a rebuild in the middle of
/// a `git checkout` would be undone by the rest of it.
pub const REBUILD_CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(60);

/// The walk-backed [`FileIndex`].
pub struct WalkIndex {
    dir: PathBuf,
    roots: RwLock<Roots>,
    /// `None` until a first index exists. A query against `None` returns nothing
    /// and `status` says `Building`, so the UI never shows an empty list as if it
    /// were an answer.
    index: RwLock<Option<Arc<Index>>>,
    status: RwLock<IndexStatus>,
    /// What has changed since the mapped file was written. Read on every query,
    /// emptied by a rebuild.
    overlay: RwLock<Overlay>,
    /// Held so the watch threads live as long as the index does. Dropping it
    /// stops them.
    watcher: Mutex<Option<Watcher>>,
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
            overlay: RwLock::new(Overlay::new()),
            watcher: Mutex::new(None),
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
        let roots = self.current_roots();
        self.swap_in(walker::walk(&roots))
    }

    /// Re-walk one root, keeping the others from the mapped index (§5 task 6).
    ///
    /// What overflow triggers. Scoped because a `git checkout` in one project
    /// says nothing about OneDrive, and re-walking everything would make a
    /// routine event cost the full walk.
    pub fn rescan_root(&self, root: &Path) -> std::io::Result<()> {
        let roots = self.current_roots();
        let mut builder = Builder::new();

        let carried = self
            .index
            .read()
            .ok()
            .and_then(|guard| guard.as_ref().map(|index| copy_roots(index, root, &mut builder)))
            .unwrap_or_default();

        // Roots the mapped index did not carry — the affected one, and any added
        // since it was written — are walked fresh.
        for candidate in &roots.include {
            if !carried.iter().any(|c| paths_equal(c, candidate)) {
                walker::walk_into(candidate, &roots.exclude, &mut builder);
            }
        }
        self.swap_in(builder)
    }

    /// Apply one watcher event. Cheap by construction: no disk, no rebuild.
    pub fn apply(&self, change: Change) {
        match change {
            Change::Overflow(root) => {
                // Events were dropped, so what this root holds is unknown. Say so
                // before rescanning, never after — a silent stale index is the
                // one failure mode ADR-0007 refuses.
                self.set_status(IndexStatus::Stale);
                let _ = self.rescan_root(&root);
            }
            Change::Added(path, is_dir) => self.with_overlay(|o| o.add(path, is_dir)),
            Change::Removed(path) => self.with_overlay(|o| o.remove(&path)),
            Change::Renamed { from, to, is_dir } => {
                self.with_overlay(|o| o.rename(&from, to, is_dir))
            }
        }
    }

    /// Whether the overlay has grown past the point where a rebuild pays.
    pub fn wants_rebuild(&self) -> bool {
        self.overlay
            .read()
            .is_ok_and(|o| o.additions() >= REBUILD_THRESHOLD)
    }

    /// Watch the roots. Events apply to `self` until it is dropped.
    ///
    /// **Callable again after the roots change**, and must be: watchers bind to
    /// the paths they started on, so a root added in Settings would be walked
    /// once and never updated. Replacing the [`Watcher`] stops the old threads.
    pub fn watch(self: &Arc<Self>) {
        let roots = self.current_roots();
        let (tx, rx) = std::sync::mpsc::channel();
        let Some(watcher) = Watcher::start(roots.include, roots.exclude, tx) else {
            return;
        };
        if let Ok(mut slot) = self.watcher.lock() {
            *slot = Some(watcher);
        }

        let index = Arc::clone(self);
        std::thread::spawn(move || {
            // Ends when every sender is dropped, which happens when the Watcher
            // is dropped with the index.
            while let Ok(change) = rx.recv() {
                index.apply(change);
            }
        });
    }

    fn with_overlay(&self, edit: impl FnOnce(&mut Overlay)) {
        if let Ok(mut overlay) = self.overlay.write() {
            edit(&mut overlay);
        }
    }

    fn current_roots(&self) -> Roots {
        self.roots.read().map(|r| r.clone()).unwrap_or(Roots {
            include: Vec::new(),
            exclude: Vec::new(),
        })
    }

    /// Write a built index, map it, and drop the deltas it now contains.
    fn swap_in(&self, builder: Builder) -> std::io::Result<()> {
        let generation = self.generation() + 1;
        let bytes = builder.finish(generation);

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
            // The deltas are in the file now. Clearing before the swap would open
            // a window where neither held them.
            self.with_overlay(Overlay::clear);
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

    /// Names starting with a needle too short for the postings (§5).
    ///
    /// A linear pass over every name. No allocation per entry: the comparison is
    /// an ASCII-insensitive prefix test on the bytes, so the cost is one pass
    /// over the arena rather than one lowercased `String` per entry.
    pub fn scan_prefixes(&self, needle: &str, limit: usize) -> Vec<FileHit> {
        let Ok(guard) = self.index.read() else {
            return Vec::new();
        };
        let Some(index) = guard.as_ref() else {
            return Vec::new();
        };
        let bytes = needle.as_bytes();
        let mut hits = self.overlay_hits(needle, limit);

        // Every match is scored before any is dropped. Stopping at `limit` would
        // return the first twelve in **walk order**, so a folder named exactly
        // `HH` loses to `hh.aac` for having been walked later — which is how a
        // two-letter search stops answering the question that was asked.
        let mut scored: Vec<(u32, f32)> = Vec::new();
        for id in 0..index.entry_count() {
            let name = index.name(id).as_bytes();
            if name.len() >= bytes.len() && name[..bytes.len()].eq_ignore_ascii_case(bytes) {
                scored.push((
                    id,
                    // Exact name or prefix, borrowing the ladder's own two rungs
                    // so a short answer sorts against a long one consistently.
                    if name.len() == bytes.len() {
                        rank::TIER_EXACT_NAME
                    } else {
                        rank::TIER_NAME_PREFIX
                    },
                ));
            }
        }
        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        scored.truncate(limit * 4);

        hits.extend(scored.into_iter().filter_map(|(id, score)| {
            let path = index.path(id);
            if self.is_removed(&path) {
                return None;
            }
            Some(FileHit {
                is_dir: index.is_dir(id),
                score,
                path,
            })
        }));
        hits.truncate(limit);
        hits
    }

    /// Hits from the delta, which the mapped file does not yet know about.
    fn overlay_hits(&self, needle: &str, limit: usize) -> Vec<FileHit> {
        self.overlay
            .read()
            .map(|o| o.search(needle, limit))
            .unwrap_or_default()
    }

    /// Whether a mapped path has been deleted since the file was written.
    fn is_removed(&self, path: &Path) -> bool {
        self.overlay.read().is_ok_and(|o| o.is_removed(path))
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
        if needle.is_empty() {
            return Vec::new();
        }
        if needle.len() < MIN_TRIGRAM_LEN {
            // §5 says scan the recent set here, assuming a full scan is too slow.
            // Measured at 646 us over 26,846 entries, so it isn't — and a full
            // scan finds a two-letter folder the recent set has never seen.
            return self.scan_prefixes(&needle, limit);
        }
        // Deltas first: a file created a second ago is in neither the postings
        // nor the arena, and is exactly what the user is most likely hunting.
        let mut hits = self.overlay_hits(&needle, limit);

        let Ok(guard) = self.index.read() else {
            return hits;
        };
        let Some(index) = guard.as_ref() else {
            return hits;
        };

        // Scored as ids, not as Entries. Reconstructing a path walks the parent
        // chain and allocates, and a common needle matches tens of thousands of
        // candidates against twelve visible rows — so paths are built only for
        // the rows that survive the cut.
        let query = Query::new(&needle);
        let mut scored: Vec<(u32, f32)> = index
            .candidates(&needle)
            .into_iter()
            .filter_map(|id| {
                let name = index.name(id);
                // Cheap prefilter first: every rung a file can clear implies the
                // name contains the needle, and building a Haystack for a
                // candidate that cannot match is the whole cost of the query.
                if !rank::contains_fold(name, &needle) {
                    return None;
                }
                let score = rank::score_path(&query, &Haystack::new(name, None))?;
                Some((id, score))
            })
            .collect();
        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                // Ties broken by id, which is walk order and therefore stable
                // between runs — the Stability rule would see anything else move.
                .then_with(|| a.0.cmp(&b.0))
        });
        // Wider than `limit`: a deleted entry drops out below and the overlay may
        // supply some of the visible rows.
        scored.truncate(limit * 4);

        hits.extend(scored.into_iter().filter_map(|(id, score)| {
            let path = index.path(id);
            // A hit the watcher has seen deleted is not a hit. Serving it is how
            // a recents list rots, and an index rots the same way.
            if self.is_removed(&path) {
                return None;
            }
            Some(FileHit {
                is_dir: index.is_dir(id),
                score,
                path,
            })
        }));

        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                // Ties broken by path so two runs agree. Without it the order is
                // whatever the postings happened to hold, which the Stability
                // rule would then see move under it.
                .then_with(|| a.path.cmp(&b.path))
        });
        // A path can reach here twice: once from the overlay, once from the file
        // it was already in. The overlay copy sorts first and wins.
        let mut seen = std::collections::HashSet::new();
        hits.retain(|hit| seen.insert(hit.path.to_string_lossy().to_lowercase()));
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

/// Copy every root except `skip` out of a mapped index into a Builder.
///
/// Returns the roots actually carried, so the caller knows which to walk. Ids
/// ascend from parent to child, so one pass suffices and a skipped parent takes
/// its children with it.
fn copy_roots(index: &Index, skip: &Path, builder: &mut Builder) -> Vec<PathBuf> {
    let mut remap: Vec<Option<u32>> = vec![None; index.entry_count() as usize];
    let mut carried = Vec::new();

    for id in 0..index.entry_count() {
        let name = index.name(id);
        let new_parent = match index.parent(id) {
            None => {
                if paths_equal(Path::new(name), skip) {
                    continue;
                }
                carried.push(PathBuf::from(name));
                None
            }
            // A child whose parent was skipped is skipped with it.
            Some(parent) => match remap.get(parent as usize).copied().flatten() {
                Some(mapped) => Some(mapped),
                None => continue,
            },
        };
        remap[id as usize] = Some(builder.push(name, new_parent, index.is_dir(id)));
    }
    carried
}

/// Windows path equality: case-insensitive, trailing separator ignored.
fn paths_equal(a: &Path, b: &Path) -> bool {
    let strip = |p: &Path| {
        p.to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_lowercase()
    };
    strip(a) == strip(b)
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

    /// Under three characters there is no trigram, so the scan answers instead.
    ///
    /// A two-letter folder is a real case — `C:\Data\0Projects\Create\HH` is what
    /// found this. Declining would make it permanently unfindable by its own name.
    #[test]
    fn v0_7_a_short_query_is_answered_by_the_prefix_scan() {
        let (tree, store, roots) = fixture("short");
        let index = WalkIndex::load(store.clone(), roots);
        index.rebuild().unwrap();

        let short = index.search("ba", 10);
        assert_eq!(short.len(), 1, "bangs.rs starts with ba");
        assert!(short[0].path.ends_with("bangs.rs"));
        assert!(!index.search("ban", 10).is_empty());

        let _ = std::fs::remove_dir_all(&tree);
        let _ = std::fs::remove_dir_all(&store);
    }

    /// An exact name beats a longer one that merely starts with the needle, and
    /// it beats it wherever the walk happened to reach each.
    ///
    /// `HH` the folder against `hh.aac` the file: the folder is walked later on
    /// the real machine, so a scan that stopped at the limit never saw it.
    #[test]
    fn v0_7_a_short_query_ranks_an_exact_name_first() {
        let (tree, store, roots) = fixture("short-rank");
        // Written after the others, so the exact match is late in walk order.
        for n in 0..30 {
            std::fs::write(tree.join(format!("re{n}.md")), "x").unwrap();
        }
        std::fs::create_dir_all(tree.join("re")).unwrap();

        let index = WalkIndex::load(store.clone(), roots);
        index.rebuild().unwrap();

        let hits = index.search("re", 5);
        assert!(!hits.is_empty());
        assert!(hits[0].path.ends_with("re"), "got {:?}", hits[0].path);
        assert!(hits[0].is_dir);

        let _ = std::fs::remove_dir_all(&tree);
        let _ = std::fs::remove_dir_all(&store);
    }

    /// The scan is a prefix test, not a substring one: `an` must not drag in
    /// `bangs.rs`, or two letters return most of the index.
    #[test]
    fn v0_7_the_short_scan_matches_a_prefix_not_a_substring() {
        let (tree, store, roots) = fixture("prefix");
        let index = WalkIndex::load(store.clone(), roots);
        index.rebuild().unwrap();

        assert!(index.search("an", 10).is_empty());
        assert!(!index.search("re", 10).is_empty(), "README.md starts with re");

        let _ = std::fs::remove_dir_all(&tree);
        let _ = std::fs::remove_dir_all(&store);
    }

    /// The exit criterion watchers exist for: a file created after the walk is
    /// findable without the index being rewritten.
    #[test]
    fn v0_7_a_file_created_after_the_walk_is_findable() {
        let (tree, store, roots) = fixture("created");
        let index = WalkIndex::load(store.clone(), roots);
        index.rebuild().unwrap();
        assert!(index.search("brandnew", 10).is_empty());

        let fresh = tree.join("brandnew.rs");
        std::fs::write(&fresh, "x").unwrap();
        index.apply(Change::Added(fresh.clone(), false));

        let hits = index.search("brandnew", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, fresh);
        // The generation is untouched: nothing was rewritten to answer this.
        assert_eq!(index.generation(), 1);

        let _ = std::fs::remove_dir_all(&tree);
        let _ = std::fs::remove_dir_all(&store);
    }

    /// A two-letter folder created after the walk reaches the short-query path
    /// too, or the fix for `HH` only works on files that existed at boot.
    #[test]
    fn v0_7_a_short_query_sees_the_overlay() {
        let (tree, store, roots) = fixture("short-overlay");
        let index = WalkIndex::load(store.clone(), roots);
        index.rebuild().unwrap();

        let created = tree.join("HH");
        std::fs::create_dir_all(&created).unwrap();
        index.apply(Change::Added(created.clone(), true));

        let hits = index.search("hh", 10);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].is_dir);

        let _ = std::fs::remove_dir_all(&tree);
        let _ = std::fs::remove_dir_all(&store);
    }

    /// A deleted file must stop being returned immediately. Serving one is how
    /// the user learns the feature lies.
    #[test]
    fn v0_7_a_deleted_file_stops_being_returned() {
        let (tree, store, roots) = fixture("deleted");
        let index = WalkIndex::load(store.clone(), roots);
        index.rebuild().unwrap();
        assert_eq!(index.search("readme", 10).len(), 1);

        index.apply(Change::Removed(tree.join("README.md")));
        assert!(index.search("readme", 10).is_empty());

        let _ = std::fs::remove_dir_all(&tree);
        let _ = std::fs::remove_dir_all(&store);
    }

    /// Deleting a directory hides everything under it, on one event.
    #[test]
    fn v0_7_deleting_a_directory_hides_its_contents() {
        let (tree, store, roots) = fixture("deleted-dir");
        let index = WalkIndex::load(store.clone(), roots);
        index.rebuild().unwrap();
        assert_eq!(index.search("bangs", 10).len(), 1);

        index.apply(Change::Removed(tree.join("src")));
        assert!(index.search("bangs", 10).is_empty());

        let _ = std::fs::remove_dir_all(&tree);
        let _ = std::fs::remove_dir_all(&store);
    }

    /// Overflow means events were lost, so the root is re-walked and whatever
    /// appeared while nobody was looking is picked up.
    #[test]
    fn v0_7_overflow_rescans_the_root_it_came_from() {
        let (tree, store, roots) = fixture("overflow");
        let index = WalkIndex::load(store.clone(), roots);
        index.rebuild().unwrap();

        // Created with no event at all, which is what a dropped event looks like.
        std::fs::write(tree.join("missed.rs"), "x").unwrap();
        assert!(index.search("missed", 10).is_empty());

        index.apply(Change::Overflow(tree.clone()));
        assert_eq!(index.search("missed", 10).len(), 1);
        assert_eq!(index.status(), IndexStatus::Ready);
        assert_eq!(index.generation(), 2, "the rescan wrote a new generation");

        let _ = std::fs::remove_dir_all(&tree);
        let _ = std::fs::remove_dir_all(&store);
    }

    /// Stale is set **before** the rescan, not after. If the rescan cannot run,
    /// the index must still be saying so rather than reporting Ready.
    #[test]
    fn v0_7_a_failed_rescan_leaves_the_index_stale() {
        let (tree, store, roots) = fixture("stale");
        let index = WalkIndex::load(store.clone(), roots.clone());
        index.rebuild().unwrap();
        assert_eq!(index.status(), IndexStatus::Ready);
        drop(index);

        // A file where the index directory should be: every write fails.
        let blocked = store.join("blocked");
        std::fs::write(&blocked, "not a directory").unwrap();
        let index = WalkIndex::load(blocked, roots);
        index.apply(Change::Overflow(tree.clone()));
        assert_eq!(index.status(), IndexStatus::Stale);

        let _ = std::fs::remove_dir_all(&tree);
        let _ = std::fs::remove_dir_all(&store);
    }

    /// A rescan is scoped: the other roots are carried out of the mapped index
    /// rather than walked again.
    #[test]
    fn v0_7_a_scoped_rescan_keeps_the_other_roots() {
        let (a, store, _) = fixture("scoped-a");
        let (b, _, _) = fixture("scoped-b");
        let roots = Roots {
            include: vec![a.clone(), b.clone()],
            exclude: Vec::new(),
        };
        let index = WalkIndex::load(store.clone(), roots);
        index.rebuild().unwrap();
        let before = index.entry_count();

        // Deleted from disk but carried from the index, which is what proves it
        // was copied rather than re-walked.
        std::fs::remove_dir_all(b.join("src")).unwrap();
        index.rescan_root(&a).unwrap();

        assert_eq!(index.entry_count(), before);
        assert_eq!(index.search("bangs", 10).len(), 2, "both roots still answer");

        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
        let _ = std::fs::remove_dir_all(&store);
    }

    /// A rebuild folds the deltas into the file. The overlay must not keep
    /// answering with them, or every entry appears twice.
    #[test]
    fn v0_7_a_rebuild_folds_the_overlay_into_the_file() {
        let (tree, store, roots) = fixture("fold");
        let index = WalkIndex::load(store.clone(), roots);
        index.rebuild().unwrap();

        let fresh = tree.join("folded.rs");
        std::fs::write(&fresh, "x").unwrap();
        index.apply(Change::Added(fresh, false));
        index.rebuild().unwrap();

        let hits = index.search("folded", 10);
        assert_eq!(hits.len(), 1, "once from the file, not twice");

        let _ = std::fs::remove_dir_all(&tree);
        let _ = std::fs::remove_dir_all(&store);
    }

    /// An empty query returns nothing, never everything. The scan would otherwise
    /// match every name in the index on a zero-length prefix.
    #[test]
    fn v0_7_an_empty_query_is_not_a_match_for_everything() {
        let (tree, store, roots) = fixture("empty");
        let index = WalkIndex::load(store.clone(), roots);
        index.rebuild().unwrap();

        assert!(index.search("", 10).is_empty());
        assert!(index.search("   ", 10).is_empty());

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
