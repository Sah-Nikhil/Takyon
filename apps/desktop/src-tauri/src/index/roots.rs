//! Which locations get walked, and which are skipped inside them (TBC-0005).
//!
//! A root is a folder Takyon descends recursively. A file under no root does not
//! exist as far as `!e` is concerned — there is no live disk search behind the
//! index, which is the whole point of ADR-0007. That makes this list a product
//! decision rather than a constant, and TBC-0005 is the least-evidenced call in
//! the design.
//!
//! Shell folders come from Windows, so they are right wherever the user moved
//! them. The code root is **probed**, not hardcoded: TBC-0005's amendment carries
//! why, and the Windows Search measurement that ruled out letting the OS cover it.

use std::path::{Path, PathBuf};

/// Folder and file names never walked into, matched case-insensitively against
/// one path segment.
///
/// Skipped **during** the walk. Walking 400,000 `node_modules` entries and then
/// discarding them spends the whole 60 s budget on files nobody searches for.
pub const DEFAULT_EXCLUDES: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "dist",
    "build",
    ".next",
    "venv",
    ".venv",
    "__pycache__",
    "AppData",
    "$Recycle.Bin",
    "Program Files",
];

/// Where people keep code, in probe order.
///
/// Windows has no "where does this user keep code" API, so this is a guess — but
/// a guess that costs nothing when wrong, since only paths that exist become
/// roots. `source\repos` is Visual Studio's own default.
pub fn code_candidates(home: &Path) -> Vec<PathBuf> {
    vec![
        PathBuf::from(r"C:\Programming"),
        home.join("source").join("repos"),
        home.join("dev"),
        home.join("code"),
        home.join("projects"),
        home.join("git"),
        home.join("repos"),
    ]
}

/// The shell folders every machine has, resolved through Windows.
///
/// Through the shell, never `%USERPROFILE%\Documents`: OneDrive redirects these
/// on a great many machines, and an env-var path then points at a real but empty
/// directory that indexes nothing and reports no error. Same trap as `lnk.rs`.
#[cfg(windows)]
pub fn shell_folders() -> Vec<PathBuf> {
    use windows::Win32::UI::Shell::{
        FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Pictures,
        FOLDERID_Videos, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
    };

    let mut roots = Vec::new();
    for id in [
        FOLDERID_Desktop,
        FOLDERID_Documents,
        FOLDERID_Downloads,
        FOLDERID_Pictures,
        FOLDERID_Videos,
    ] {
        // SAFETY: `id` is a static GUID and the returned buffer is freed below.
        unsafe {
            let Ok(raw) = SHGetKnownFolderPath(&id, KF_FLAG_DEFAULT, None) else {
                continue;
            };
            if let Ok(path) = raw.to_string() {
                roots.push(PathBuf::from(path));
            }
            windows::Win32::System::Com::CoTaskMemFree(Some(raw.as_ptr().cast()));
        }
    }
    // OneDrive has no KNOWNFOLDERID worth relying on, and the client sets this
    // variable itself. `subsume` then folds away whichever shell folders live
    // inside it, which on a redirected profile is most of them.
    if let Some(onedrive) = std::env::var_os("OneDrive") {
        roots.push(PathBuf::from(onedrive));
    }
    roots
}

#[cfg(not(windows))]
pub fn shell_folders() -> Vec<PathBuf> {
    Vec::new()
}

/// The roots and exclusions a machine gets before anyone edits them (TBC-0005).
pub fn defaults() -> Roots {
    let home = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_default();

    let mut include = shell_folders();
    include.extend(probe(&code_candidates(&home), |p| p.is_dir()));

    Roots {
        include: subsume(include),
        exclude: DEFAULT_EXCLUDES.iter().map(|s| s.to_string()).collect(),
    }
}

/// What gets walked, and what is skipped inside it. Both halves user-editable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Roots {
    pub include: Vec<PathBuf>,
    pub exclude: Vec<String>,
}

/// Keep the candidates that exist, in order, without duplicates.
///
/// `exists` is passed rather than called directly so the choice is testable
/// without a filesystem — the real caller hands it [`Path::is_dir`].
pub fn probe(candidates: &[PathBuf], exists: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    let mut kept: Vec<PathBuf> = Vec::new();
    for candidate in candidates {
        if exists(candidate) && !kept.iter().any(|k| same_path(k, candidate)) {
            kept.push(candidate.clone());
        }
    }
    kept
}

/// Drop any root inside another. Order preserved, ancestor wins.
///
/// Not hygiene: OneDrive redirection makes `Documents` a child of `OneDrive` on
/// many machines, and overlapping roots means every file under both walked
/// twice, indexed twice and shown twice.
pub fn subsume(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut kept: Vec<PathBuf> = Vec::new();
    for root in roots {
        if kept.iter().any(|k| contains(k, &root)) {
            continue;
        }
        // A later root can be the *ancestor* of one already kept, so the pass has
        // to work in both directions or order decides correctness.
        kept.retain(|k| !contains(&root, k));
        kept.push(root);
    }
    kept
}

/// Whether one path segment is excluded. Case-insensitive, whole segment only.
///
/// Whole-segment: `dist` must not also exclude `distribution`, and a substring
/// rule quietly removes far more than the list appears to say.
pub fn is_excluded(name: &str, exclude: &[String]) -> bool {
    exclude.iter().any(|e| e.eq_ignore_ascii_case(name))
}

/// Case-insensitive path equality, which is what Windows means by equality.
fn same_path(a: &Path, b: &Path) -> bool {
    a.as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&b.as_os_str().to_string_lossy())
}

/// Whether `ancestor` contains `descendant`, comparing whole segments.
///
/// Segment-wise, not string prefix: `C:\code` is not an ancestor of
/// `C:\code-old`, and a `starts_with` on the text says it is.
fn contains(ancestor: &Path, descendant: &Path) -> bool {
    let mut a = ancestor.components();
    let mut d = descendant.components();
    loop {
        match (a.next(), d.next()) {
            (None, _) => return true,
            (Some(_), None) => return false,
            (Some(x), Some(y)) => {
                if !x
                    .as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&y.as_os_str().to_string_lossy())
                {
                    return false;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The amendment's rule: a candidate that is not on disk never becomes a
    /// root, so no machine ships with a default pointing at nothing.
    #[test]
    fn v0_7_probing_keeps_only_candidates_that_exist() {
        let home = PathBuf::from(r"C:\Users\sahni");
        let candidates = code_candidates(&home);
        let kept = probe(&candidates, |p| p == Path::new(r"C:\Programming"));
        assert_eq!(kept, vec![PathBuf::from(r"C:\Programming")]);
    }

    /// Nothing on disk is not an error. The shell folders still make a usable
    /// index, and the roots editor is how a code folder gets added by hand.
    #[test]
    fn v0_7_probing_nothing_yields_no_code_root() {
        let candidates = code_candidates(Path::new(r"C:\Users\sahni"));
        assert!(probe(&candidates, |_| false).is_empty());
    }

    /// `C:\Programming` is first, so this machine keeps the root it already has
    /// rather than picking up a stray `~\code` alongside it.
    #[test]
    fn v0_7_the_candidate_list_is_ordered_and_deduplicated() {
        let home = PathBuf::from(r"C:\Users\sahni");
        let kept = probe(&code_candidates(&home), |_| true);
        assert_eq!(kept.first(), Some(&PathBuf::from(r"C:\Programming")));
        assert_eq!(kept.len(), code_candidates(&home).len());

        let doubled = vec![
            PathBuf::from(r"C:\Programming"),
            PathBuf::from(r"c:\programming"),
        ];
        assert_eq!(probe(&doubled, |_| true).len(), 1);
    }

    /// OneDrive redirection is the real case: `Documents` inside `OneDrive` walked
    /// as its own root means every document indexed twice.
    #[test]
    fn v0_7_a_root_inside_another_root_is_dropped() {
        let roots = vec![
            PathBuf::from(r"C:\Users\sahni\OneDrive"),
            PathBuf::from(r"C:\Users\sahni\OneDrive\Documents"),
            PathBuf::from(r"C:\Programming"),
        ];
        assert_eq!(
            subsume(roots),
            vec![
                PathBuf::from(r"C:\Users\sahni\OneDrive"),
                PathBuf::from(r"C:\Programming"),
            ]
        );
    }

    /// The ancestor wins whichever order the two arrive in, or the result depends
    /// on how the defaults happen to be listed.
    #[test]
    fn v0_7_the_ancestor_wins_regardless_of_order() {
        let roots = vec![
            PathBuf::from(r"C:\Users\sahni\OneDrive\Documents"),
            PathBuf::from(r"C:\Users\sahni\OneDrive"),
        ];
        assert_eq!(
            subsume(roots),
            vec![PathBuf::from(r"C:\Users\sahni\OneDrive")]
        );
    }

    /// Sibling directories with a shared textual prefix are not nested. A
    /// `starts_with` on the string says they are, and silently drops one.
    #[test]
    fn v0_7_a_shared_name_prefix_is_not_containment() {
        let roots = vec![
            PathBuf::from(r"C:\code"),
            PathBuf::from(r"C:\code-old"),
            PathBuf::from(r"C:\CODE\inner"),
        ];
        assert_eq!(
            subsume(roots),
            vec![PathBuf::from(r"C:\code"), PathBuf::from(r"C:\code-old")]
        );
    }

    /// Windows paths are case-insensitive, so exclusions have to be too — the
    /// list says `node_modules` and the disk may say `Node_Modules`.
    #[test]
    fn v0_7_exclusions_match_a_whole_segment_case_insensitively() {
        let exclude: Vec<String> = DEFAULT_EXCLUDES.iter().map(|s| s.to_string()).collect();
        assert!(is_excluded("node_modules", &exclude));
        assert!(is_excluded("Node_Modules", &exclude));
        assert!(is_excluded(".git", &exclude));
        assert!(is_excluded("AppData", &exclude));
    }

    /// Whole segment, never a substring: `dist` must not take `distribution`
    /// with it, and `build` must not take `buildings`.
    #[test]
    fn v0_7_an_exclusion_does_not_match_a_longer_name() {
        let exclude: Vec<String> = DEFAULT_EXCLUDES.iter().map(|s| s.to_string()).collect();
        assert!(!is_excluded("distribution", &exclude));
        assert!(!is_excluded("buildings", &exclude));
        assert!(!is_excluded("targets", &exclude));
        assert!(!is_excluded("my-node_modules", &exclude));
    }
}
