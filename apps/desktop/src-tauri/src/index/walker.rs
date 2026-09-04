//! The scoped directory walk that fills the index (ADR-0007, §5 task 2).
//!
//! Unelevated, breadth-first, names only. Roots are walked in parallel and each
//! root's subtree sequentially: the disk is the bottleneck, not the CPU, and one
//! thread per directory would thrash a spinning volume for no gain.
//!
//! **Exclusions are applied during the walk**, never after. Walking 400,000
//! `node_modules` entries and then discarding them spends the whole 60 s budget
//! on files nobody searches for.

use std::path::{Path, PathBuf};

use rayon::prelude::*;

use super::roots::{is_excluded, Roots};
use super::store::Builder;

/// What one root's walk produced, before it is folded into the shared Builder.
///
/// Collected per root rather than pushed straight into the Builder because ids
/// are positional: two threads interleaving `push` would give a child a parent id
/// belonging to the other root's tree.
pub struct Walked {
    pub root: PathBuf,
    /// Name, parent (an index into this vector), and whether it is a directory.
    /// The root itself is index 0 with no parent.
    pub nodes: Vec<(String, Option<usize>, bool)>,
    /// Directories skipped for the exclusion list, counted for the settings UI
    /// and for TBC-0005's "is an exclusion missing" trigger.
    pub skipped: usize,
}

/// Walk every root, in parallel, and build one index from the result.
pub fn walk(roots: &Roots) -> Builder {
    let walked: Vec<Walked> = roots
        .include
        .par_iter()
        .map(|root| walk_root(root, &roots.exclude))
        .collect();

    let mut builder = Builder::new();
    for tree in walked {
        fold(&tree, &mut builder);
    }
    builder
}

/// Walk one root straight into an existing Builder.
///
/// What a scoped rescan uses: the other roots are copied from the mapped index,
/// and only the affected one is walked again.
pub fn walk_into(root: &Path, exclude: &[String], builder: &mut Builder) {
    fold(&walk_root(root, exclude), builder);
}

/// Copy one walked tree into a Builder, remapping local indices to ids.
///
/// The root is pushed first and a child always follows its parent, so a node's
/// parent is already mapped by the time the node is reached.
fn fold(tree: &Walked, builder: &mut Builder) {
    let mut ids: Vec<u32> = Vec::with_capacity(tree.nodes.len());
    for (name, parent, is_dir) in &tree.nodes {
        let parent = parent.map(|p| ids[p]);
        ids.push(builder.push(name, parent, *is_dir));
    }
}

/// Walk one root breadth-first. Never recurses: a deep tree is a queue, not a
/// stack, and the depth here is whatever the user's disk happens to hold.
pub fn walk_root(root: &Path, exclude: &[String]) -> Walked {
    let mut nodes: Vec<(String, Option<usize>, bool)> = Vec::new();
    let mut skipped = 0usize;

    if !root.is_dir() {
        return Walked {
            root: root.to_path_buf(),
            nodes,
            skipped,
        };
    }
    // The root's "name" is its whole path: it is what `store::path` rebuilds from,
    // and a root has no parent to supply the rest.
    nodes.push((root.to_string_lossy().to_string(), None, true));

    let mut queue: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    while let Some((dir, parent)) = queue.pop() {
        let Ok(children) = std::fs::read_dir(&dir) else {
            // An unreadable directory is normal unelevated — a permission denial
            // is not a failure of the walk, it is the reason there is no service.
            continue;
        };
        for child in children.flatten() {
            let name = child.file_name().to_string_lossy().to_string();
            if is_excluded(&name, exclude) {
                skipped += 1;
                continue;
            }
            let Ok(meta) = child.metadata() else { continue };
            let is_dir = meta.is_dir();
            let id = nodes.len();
            nodes.push((name, Some(parent), is_dir));

            // A junction pointing at an ancestor makes the walk a cycle, and the
            // walk has no depth cap by design. Index the reparse point itself,
            // never descend it: anything worth indexing behind one is reachable
            // as its own root.
            if is_dir && !is_reparse_point(&meta) {
                queue.push((child.path(), id));
            }
        }
    }

    Walked {
        root: root.to_path_buf(),
        nodes,
        skipped,
    }
}

/// Whether this is a junction, symlink or other reparse point.
#[cfg(windows)]
fn is_reparse_point(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(meta: &std::fs::Metadata) -> bool {
    meta.is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::store::Index;

    /// Writes a small tree under a directory the caller owns, and returns it.
    fn tree(label: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("takyon-walk")
            .join(format!("{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules").join("left-pad")).unwrap();
        std::fs::create_dir_all(dir.join("deep").join("er").join("est")).unwrap();
        std::fs::write(dir.join("README.md"), "x").unwrap();
        std::fs::write(dir.join("src").join("main.rs"), "x").unwrap();
        std::fs::write(dir.join("node_modules").join("left-pad").join("index.js"), "x").unwrap();
        std::fs::write(dir.join("deep").join("er").join("est").join("buried.txt"), "x").unwrap();
        dir
    }

    fn roots_for(dir: &Path) -> Roots {
        Roots {
            include: vec![dir.to_path_buf()],
            exclude: vec!["node_modules".to_string()],
        }
    }

    /// The walk is recursive with no depth cap: a file four levels down is found
    /// exactly like one at the top.
    #[test]
    fn v0_7_the_walk_reaches_every_depth() {
        let dir = tree("depth");
        let index = Index::from_bytes(walk(&roots_for(&dir)).finish(1)).unwrap();

        let paths: Vec<PathBuf> = (0..index.entry_count()).map(|id| index.path(id)).collect();
        assert!(paths.contains(&dir.join("README.md")));
        assert!(paths.contains(&dir.join("src").join("main.rs")));
        assert!(paths.contains(&dir.join("deep").join("er").join("est").join("buried.txt")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Excluded during the walk, so nothing inside is even visited. Asserting on
    /// the contents rather than the count: `left-pad` never being entered is the
    /// property, and a filter applied afterwards would pass a count check.
    #[test]
    fn v0_7_an_excluded_directory_is_never_entered() {
        let dir = tree("exclude");
        let walked = walk_root(&dir, &["node_modules".to_string()]);
        let names: Vec<&str> = walked.nodes.iter().map(|n| n.0.as_str()).collect();

        assert!(!names.contains(&"node_modules"));
        assert!(!names.contains(&"left-pad"));
        assert!(!names.contains(&"index.js"));
        assert_eq!(walked.skipped, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Ids are positional, so two roots folded into one Builder must not have
    /// their parents crossed — the symptom is a real file at an invented path.
    #[test]
    fn v0_7_two_roots_keep_their_own_parents() {
        let a = tree("roots-a");
        let b = tree("roots-b");
        let roots = Roots {
            include: vec![a.clone(), b.clone()],
            exclude: vec!["node_modules".to_string()],
        };
        let index = Index::from_bytes(walk(&roots).finish(1)).unwrap();

        let paths: Vec<PathBuf> = (0..index.entry_count()).map(|id| index.path(id)).collect();
        assert!(paths.contains(&a.join("src").join("main.rs")));
        assert!(paths.contains(&b.join("src").join("main.rs")));
        assert_eq!(index.root_count(), 2);
        // Every path has to start at a real root. A crossed parent produces a
        // path under the wrong one, which is still plausible-looking.
        for path in &paths {
            assert!(
                path.starts_with(&a) || path.starts_with(&b),
                "{path:?} belongs to neither root"
            );
        }
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    /// A root that does not exist is not an error. Probing keeps that rare, and a
    /// removed drive should cost its own root, not the walk.
    #[test]
    fn v0_7_a_missing_root_contributes_nothing_and_does_not_fail() {
        let roots = Roots {
            include: vec![PathBuf::from(r"Z:\nothing\here")],
            exclude: Vec::new(),
        };
        assert!(walk(&roots).is_empty());
    }
}
