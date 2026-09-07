//! Spotlight as the file index on macOS (ADR-0027, superseding ADR-0007 here).
//!
//! No walk, no watcher, no blob. `MDQuery` with `kMDQuerySynchronous` asks the
//! index the OS already maintains, which is why this is ~150 lines against the
//! walker's ~1,100 and why there is no overflow case to get right.
//!
//! Hand-declared `extern "C"` rather than a crate: these are six CoreServices
//! functions and ADR-0026 keeps the dependency list closed. The CF types
//! themselves come from `objc2-core-foundation`, which is already in the lock.
//!
//! Not asked on the Bangless path — `!e` only, exactly as on Windows.

use std::ffi::c_void;
use std::path::PathBuf;

use objc2_core_foundation::{CFArray, CFIndex, CFRetained, CFString, CFType};

use super::{roots, FileHit, FileIndex, IndexStatus};
use crate::rank;

/// Ask the index rather than start a live query and wait for it to settle.
const KMD_QUERY_SYNCHRONOUS: u32 = 1;

#[link(name = "CoreServices", kind = "framework")]
extern "C" {
    fn MDQueryCreate(
        allocator: *const c_void,
        query_string: &CFString,
        value_list_attrs: *const c_void,
        sorting_attrs: *const c_void,
    ) -> *mut c_void;
    fn MDQuerySetSearchScope(query: *mut c_void, scope_directories: &CFArray, scope_options: u32);
    fn MDQuerySetMaxCount(query: *mut c_void, size: CFIndex);
    fn MDQueryExecute(query: *mut c_void, option_flags: u32) -> u8;
    fn MDQueryGetResultCount(query: *mut c_void) -> CFIndex;
    fn MDQueryGetResultAtIndex(query: *mut c_void, idx: CFIndex) -> *const c_void;
    fn MDItemCopyAttribute(item: *const c_void, name: &CFString) -> *mut CFType;
    fn CFRelease(cf: *const c_void);
}

/// The `FileIndex` backed by Spotlight.
///
/// Holds the user's roots and exclusions and nothing else: there is no index of
/// ours to keep, which is the whole point of ADR-0027.
pub struct SpotlightIndex {
    roots: std::sync::RwLock<roots::Roots>,
}

impl SpotlightIndex {
    pub fn new(roots: roots::Roots) -> Self {
        Self {
            roots: std::sync::RwLock::new(roots),
        }
    }

    /// Replace the scopes. Takes effect on the next query and needs no rebuild.
    ///
    /// The whole difference from `WalkIndex::set_roots`, which has to walk what
    /// it was just handed: scope is a query predicate here, not a walk boundary.
    pub fn set_roots(&self, roots: roots::Roots) {
        *self.roots.write().unwrap_or_else(|e| e.into_inner()) = roots;
    }

    fn roots(&self) -> roots::Roots {
        self.roots
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl FileIndex for SpotlightIndex {
    fn search(&self, q: &str, limit: usize) -> Vec<FileHit> {
        if q.trim().is_empty() {
            return Vec::new();
        }
        // Over-fetch, because exclusions are a post-filter: asking for exactly
        // `limit` and then dropping some returns short for no reason.
        let asked = limit.saturating_mul(4).min(400);
        let roots = self.roots();
        run_query(&predicate(q), &roots.include, asked)
            .into_iter()
            .filter(|path| !is_excluded(path, &roots.exclude))
            .take(limit)
            .map(hit_of)
            .collect()
    }

    /// Constant: there is no index of ours, so no rescan and nothing to bump.
    ///
    /// ADR-0027's stated consequence — the caller uses this only to tell two
    /// index *states* apart, and Spotlight has exactly one from here.
    fn generation(&self) -> u64 {
        0
    }

    /// Always ready. Spotlight's own first indexing pass is the OS's business
    /// and predates us; there is no build phase Takyon owns or can report.
    fn status(&self) -> IndexStatus {
        IndexStatus::Ready
    }
}

/// The `MDQuery` predicate for a name substring.
///
/// `kMDItemFSName` rather than `kMDItemDisplayName`: the display name is
/// localized, so "Documents" would not match a folder the user sees under
/// another language. `cd` is case- and diacritic-insensitive.
fn predicate(q: &str) -> String {
    format!(r#"kMDItemFSName == "*{}*"cd"#, escape(q))
}

/// Escape a needle for a quoted `MDQuery` literal.
///
/// `*` and `?` are wildcards inside the quotes, so a query containing one would
/// otherwise widen the search rather than narrow it, and an unescaped `"` ends
/// the literal and makes the whole predicate invalid.
fn escape(q: &str) -> String {
    q.chars()
        .filter(|c| !c.is_control())
        .flat_map(|c| match c {
            '\\' | '"' | '*' | '?' => vec!['\\', c],
            other => vec![other],
        })
        .collect()
}

/// Whether a path falls under a user exclusion.
///
/// Matches on any component, not just the file name: an exclusion of
/// `node_modules` means the tree, which is what makes it worth setting.
fn is_excluded(path: &str, exclude: &[String]) -> bool {
    std::path::Path::new(path)
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .any(|part| roots::is_excluded(part, exclude))
}

/// One hit. `is_dir` is a stat, which is cheap at these counts and is the only
/// way to know: Spotlight reports a bundle as both a file and a directory.
fn hit_of(path: String) -> FileHit {
    let path = PathBuf::from(path);
    FileHit {
        score: rank::TIER_EXE_PREFIX,
        is_dir: path.is_dir(),
        path,
    }
}

/// Execute one synchronous query and read `kMDItemPath` from each result.
fn run_query(predicate: &str, scopes: &[PathBuf], limit: usize) -> Vec<String> {
    let query_string = CFString::from_str(predicate);
    // SAFETY: every pointer below is either null or owned for the call, and the
    // query is released before returning on every path.
    let query = unsafe {
        MDQueryCreate(
            std::ptr::null(),
            &query_string,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if query.is_null() {
        return Vec::new();
    }

    // Kept alive until the call returns: the array carries no callbacks, so it
    // does not retain its members and they must outlive it.
    let scope_strings: Vec<CFRetained<CFString>> = scopes
        .iter()
        .map(|p| CFString::from_str(&p.to_string_lossy()))
        .collect();
    if !scope_strings.is_empty() {
        let mut pointers: Vec<*const c_void> = scope_strings
            .iter()
            .map(|s| CFRetained::as_ptr(s).as_ptr().cast_const().cast())
            .collect();
        let array = unsafe {
            CFArray::new(
                None,
                pointers.as_mut_ptr(),
                pointers.len() as CFIndex,
                std::ptr::null(),
            )
        };
        if let Some(array) = array {
            unsafe { MDQuerySetSearchScope(query, &array, 0) };
        }
    }

    unsafe { MDQuerySetMaxCount(query, limit as CFIndex) };
    if unsafe { MDQueryExecute(query, KMD_QUERY_SYNCHRONOUS) } == 0 {
        unsafe { CFRelease(query) };
        return Vec::new();
    }

    let attribute = CFString::from_str("kMDItemPath");
    let count = unsafe { MDQueryGetResultCount(query) };
    let mut out = Vec::new();
    for i in 0..count {
        let item = unsafe { MDQueryGetResultAtIndex(query, i) };
        if item.is_null() {
            continue;
        }
        let value = unsafe { MDItemCopyAttribute(item, &attribute) };
        if value.is_null() {
            continue;
        }
        // `MDItemCopyAttribute` is a Copy: ours to release either way.
        if let Some(text) = unsafe { (*value).downcast_ref::<CFString>() } {
            out.push(text.to_string());
        }
        unsafe { CFRelease(value.cast()) };
    }
    unsafe { CFRelease(query) };
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_12_a_needle_becomes_a_contains_predicate() {
        assert_eq!(predicate("notes"), r#"kMDItemFSName == "*notes*"cd"#);
    }

    #[test]
    fn v0_12_wildcards_in_a_query_are_escaped_rather_than_honoured() {
        // Unescaped, `*` would widen the search and `"` would end the literal
        // and make the predicate invalid — a query that silently returns junk.
        assert_eq!(escape(r#"a*b?c"d\e"#), r#"a\*b\?c\"d\\e"#);
        assert_eq!(escape("plain"), "plain");
    }

    #[test]
    fn v0_12_a_control_character_cannot_reach_the_predicate() {
        assert_eq!(escape("a\nb\tc"), "abc");
    }

    #[test]
    fn v0_12_an_exclusion_matches_any_component_not_just_the_name() {
        let exclude = vec!["node_modules".to_string()];
        assert!(is_excluded("/Users/me/app/node_modules/x/index.js", &exclude));
        assert!(!is_excluded("/Users/me/app/src/index.js", &exclude));
    }
}
