//! `.app` bundles, macOS's answer to the Start Menu walk (`docs/plans/macos.md`).
//!
//! An application on macOS is a directory whose name ends `.app`, in one of a
//! few known places. No COM, no shortcut parsing, no shell namespace — a bounded
//! directory listing finds every installed application, which is why this file
//! is a tenth the size of `lnk.rs`.
//!
//! Depth is capped rather than recursive. Bundles nest one level in practice
//! (`/System/Applications/Utilities/Terminal.app`) and a bundle's *own* contents
//! hold dozens more that are helpers, not applications: descending into
//! `Xcode.app` would offer someone `Instruments` and forty things they have never
//! heard of.

#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};

/// How far below a root to look. 1 is `/Applications/Utilities/Terminal.app`.
const MAX_DEPTH: usize = 1;

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
    /// for almost everything and wrong quietly rather than badly — `docs/plans/macos.md`.
    pub name: String,
    pub path: PathBuf,
}

/// Where applications live. Order is quality of metadata, like `lnk.rs`'s.
///
/// `~/Applications` first because a per-user install is the one the user chose;
/// `/System/Applications` last because it is Apple's own and never a surprise.
pub fn roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }
    roots.push(PathBuf::from("/Applications"));
    roots.push(PathBuf::from("/System/Applications"));
    roots
}

/// Every bundle under [`roots`], nearest root first.
///
/// Duplicates are left in: a bundle present in two roots is two real installs,
/// and `discover_all` already dedupes by [`crate::entry::EntryId`], which is the
/// path.
pub fn discover() -> Vec<Bundle> {
    let mut found = Vec::new();
    for root in roots() {
        walk(&root, 0, &mut found);
    }
    found
}

/// One directory, then its subdirectories while `depth` allows.
///
/// A `.app` is a leaf and is never descended into. An unreadable directory is
/// skipped rather than reported: a root that does not exist is the normal case
/// on a machine with no `~/Applications`.
fn walk(dir: &Path, depth: usize, found: &mut Vec<Bundle>) {
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

        if let Some(name) = bundle_name(&path) {
            found.push(Bundle { name, path });
        } else if depth < MAX_DEPTH {
            walk(&path, depth + 1, found);
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

    /// The roots are the three Finder shows, and `~/Applications` leads because a
    /// per-user install is the copy the user chose to have.
    #[test]
    fn v0_11_the_system_roots_are_walked_last() {
        let roots = roots();
        assert!(roots.contains(&PathBuf::from("/Applications")));
        assert_eq!(roots.last(), Some(&PathBuf::from("/System/Applications")));
    }
}
