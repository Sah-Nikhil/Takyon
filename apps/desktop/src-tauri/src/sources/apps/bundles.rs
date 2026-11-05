//! `.app` bundles, macOS's answer to the Start Menu walk (`docs/plans/v0.12-macos.md`).
//!
//! An application on macOS is a directory ending `.app` in a few known places.
//! No COM, no shortcut parsing — a bounded listing finds every one, which is why
//! this file is a tenth the size of `lnk.rs`.
//!
//! Depth is capped rather than recursive, and a `.app` is a leaf. Folders nest
//! (`~/Applications/Chrome Apps.localized/…`, vendor suites), but a bundle's own
//! contents hold helpers, not applications: descending into `Xcode.app` would
//! offer `Instruments` and forty things nobody has heard of.
//!
//! Never Spotlight. A reindex hides apps from Spotlight for hours; this walk
//! cannot lose one. Reasoning in `docs/tbd/v0.12.md` §17.

#![cfg(target_os = "macos")]

use std::collections::HashSet;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// How far below a root to look. 1 is `/Applications/Utilities/Terminal.app`.
///
/// 3 matches Raycast's Deep Search ceiling. Bounded by [`MAX_PER_DIR`] and by
/// never entering a bundle, so the extra levels only cost plain folders.
const MAX_DEPTH: usize = 3;

/// Most bundles taken from any one directory, matching `path.rs`'s guard: a
/// directory of a hundred thousand entries must not stall discovery silently.
const MAX_PER_DIR: usize = 4000;

/// One discovered application bundle.
#[derive(Clone, Debug)]
pub struct Bundle {
    /// What Finder shows, which is the bundle name without `.app`.
    ///
    /// Not `CFBundleDisplayName`: reading it means parsing an `Info.plist` that
    /// is usually a *binary* plist, so it costs a dependency. The stem is right
    /// for almost everything and wrong quietly rather than badly — `docs/plans/v0.12-macos.md`.
    pub name: String,
    pub path: PathBuf,
}

/// Where applications live. Order is quality of metadata, like `lnk.rs`'s.
///
/// `~/Applications` first (the copy the user chose), Apple's own last.
/// `CoreServices/Applications` holds Feedback Assistant and Archive Utility;
/// never all of `/System/Library`, which is thousands of helpers.
pub fn roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }
    roots.push(PathBuf::from("/Applications"));
    roots.push(PathBuf::from("/System/Applications"));
    roots.push(PathBuf::from("/System/Library/CoreServices/Applications"));
    roots
}

/// Every bundle under [`roots`], nearest root first.
///
/// Two copies in two roots are two real installs and both stay. One bundle
/// reached twice, through a symlink, is one: deduped by `(dev, ino)`, first
/// path wins, so the nearer root names it.
pub fn discover() -> Vec<Bundle> {
    discover_in(&roots())
}

/// [`discover`] over explicit roots, so a test can hand it a temp tree.
pub fn discover_in(roots: &[PathBuf]) -> Vec<Bundle> {
    let mut walk = Walk::default();
    for root in roots {
        walk.dir(root, 0);
    }
    walk.found
}

#[derive(Default)]
struct Walk {
    found: Vec<Bundle>,
    /// `(dev, ino)` of every bundle taken. Also stops a symlink cycle re-adding.
    seen: HashSet<(u64, u64)>,
}

/// One directory, then its subdirectories while `depth` allows.
///
/// A `.app` is a leaf and is never descended into. An unreadable directory is
/// skipped rather than reported: a root that does not exist is the normal case
/// on a machine with no `~/Applications`.
impl Walk {
    fn dir(&mut self, dir: &Path, depth: usize) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };

        let mut taken = 0usize;
        for entry in entries.flatten() {
            if taken >= MAX_PER_DIR {
                break;
            }
            let path = entry.path();
            // `is_dir` follows symlinks on purpose: `/Applications` on many machines
            // holds links to bundles installed elsewhere, and those are applications
            // the user expects to find.
            if !path.is_dir() {
                continue;
            }
            taken += 1;

            if let Some(stem) = bundle_name(&path) {
                // `metadata` follows the link, so a symlinked bundle and its target
                // share one identity.
                let identity = std::fs::metadata(&path).map(|m| (m.dev(), m.ino()));
                if identity.is_ok_and(|id| !self.seen.insert(id)) {
                    continue;
                }
                let name = finder_name(&path).unwrap_or(stem);
                self.found.push(Bundle { name, path });
            } else if depth < MAX_DEPTH {
                self.dir(&path, depth + 1);
            }
        }
    }
}

/// The display name of a bundle path, or `None` where it is not a bundle.
///
/// Pure, so the `.app` rule is testable without a filesystem. Case-insensitive
/// because HFS+ and APFS both are by default, and a `Foo.APP` is still a bundle.
pub fn bundle_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_suffix(".app").or_else(|| {
        name.to_lowercase()
            .ends_with(".app")
            .then(|| &name[..name.len() - 4])
    })?;
    (!stem.is_empty()).then(|| stem.to_string())
}

/// What Finder itself shows for this path, when it differs from the stem.
///
/// `displayNameAtPath` is Finder's own answer, so it is localized and already
/// strips `.app` — better than reading `CFBundleDisplayName`, which needs a
/// binary-plist parse. Returns `None` when it adds nothing over the stem.
fn finder_name(path: &Path) -> Option<String> {
    use objc2_foundation::{NSFileManager, NSString};

    let shown = NSFileManager::defaultManager()
        .displayNameAtPath(&NSString::from_str(&path.to_string_lossy()))
        .to_string();
    // It returns the last path component when it knows nothing better, which
    // still carries `.app` — the stem is the better answer in that case.
    (!shown.is_empty() && !shown.ends_with(".app")).then_some(shown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_11_a_bundle_name_is_the_stem_without_the_extension() {
        assert_eq!(
            bundle_name(Path::new("/Applications/Safari.app")).as_deref(),
            Some("Safari")
        );
        assert_eq!(
            bundle_name(Path::new("/Applications/Visual Studio Code.app")).as_deref(),
            Some("Visual Studio Code")
        );
    }

    /// Both filesystems macOS ships are case-insensitive by default, so a
    /// differently-cased extension is the same bundle and not a directory to
    /// descend into.
    #[test]
    fn v0_11_the_extension_is_matched_case_insensitively() {
        assert_eq!(
            bundle_name(Path::new("/Applications/Thing.APP")).as_deref(),
            Some("Thing")
        );
    }

    /// Anything else is a directory to walk, not an application to offer.
    #[test]
    fn v0_11_a_plain_directory_is_not_a_bundle() {
        assert_eq!(bundle_name(Path::new("/Applications/Utilities")), None);
        assert_eq!(bundle_name(Path::new("/Applications/.app")), None);
        assert_eq!(bundle_name(Path::new("/")), None);
    }

    /// `~/Applications` leads because a per-user install is the copy the user
    /// chose to have; Apple's own roots come last.
    #[test]
    fn v0_11_the_system_roots_are_walked_last() {
        let roots = roots();
        assert!(roots.contains(&PathBuf::from("/Applications")));
        let n = roots.len();
        assert_eq!(roots[n - 2], PathBuf::from("/System/Applications"));
        assert_eq!(
            roots[n - 1],
            PathBuf::from("/System/Library/CoreServices/Applications")
        );
    }

    /// Temp tree, removed on drop. `common::TempDir` is integration-test only.
    struct Tree(PathBuf);

    impl Tree {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("takyon-bundles-{tag}-{nanos}"));
            std::fs::create_dir_all(&root).unwrap();
            Tree(root)
        }

        fn app(&self, rel: &str) -> PathBuf {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.join("Contents")).unwrap();
            path
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn names(bundles: &[Bundle]) -> Vec<String> {
        let mut names: Vec<String> = bundles
            .iter()
            .map(|b| bundle_name(&b.path).unwrap())
            .collect();
        names.sort();
        names
    }

    /// Three folders deep is found; a fourth is past the cap.
    #[test]
    fn v0_12_nested_folders_are_walked_to_the_depth_cap() {
        let tree = Tree::new("depth");
        tree.app("Top.app");
        tree.app("Chrome Apps.localized/Gmail.app");
        tree.app("Suite/Tools/Inner.app");
        tree.app("a/b/c/Deep.app");
        tree.app("a/b/c/d/TooDeep.app");
        let found = discover_in(std::slice::from_ref(&tree.0));
        assert_eq!(names(&found), ["Deep", "Gmail", "Inner", "Top"]);
    }

    /// Helpers inside a bundle are never offered.
    #[test]
    fn v0_12_a_bundle_is_never_descended_into() {
        let tree = Tree::new("leaf");
        tree.app("Xcode.app");
        tree.app("Xcode.app/Contents/Applications/Instruments.app");
        let found = discover_in(std::slice::from_ref(&tree.0));
        assert_eq!(names(&found), ["Xcode"]);
    }

    /// One bundle reached through a symlink in another root is one Entry, named
    /// by the root walked first.
    #[test]
    fn v0_12_a_symlinked_bundle_is_found_once() {
        let real = Tree::new("real");
        let links = Tree::new("links");
        let target = real.app("Tool.app");
        std::os::unix::fs::symlink(&target, links.0.join("Tool.app")).unwrap();
        let found = discover_in(&[links.0.clone(), real.0.clone()]);
        assert_eq!(found.len(), 1);
        assert!(found[0].path.starts_with(&links.0));
    }

    /// Two separate copies are two installs and both stay.
    #[test]
    fn v0_12_two_real_copies_are_both_kept() {
        let a = Tree::new("copy-a");
        let b = Tree::new("copy-b");
        a.app("Tool.app");
        b.app("Tool.app");
        assert_eq!(discover_in(&[a.0.clone(), b.0.clone()]).len(), 2);
    }
}
