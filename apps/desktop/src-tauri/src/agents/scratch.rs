//! The Scratch directory — where a Turn runs when the user chose nowhere.
//!
//! Its job is to be uninteresting. An Agent with tools, pointed at a directory
//! the user did not pick, is a worse outcome than a signed-out one (ADR-0017), so
//! the default is an empty directory beside the rest of our data and the worst
//! case is that the Agent finds nothing.

use std::path::PathBuf;

/// Directory name under the ADR-0011 data directory.
const DIR: &str = "scratch";

/// The Scratch directory, created if absent.
///
/// Falls back to the OS temp directory when our own is unwritable: a Turn that
/// cannot start because a folder is missing is a worse failure than one that runs
/// somewhere equally empty.
pub fn dir() -> PathBuf {
    let path = crate::identity::data_dir()
        .map(|data| data.join(DIR))
        .unwrap_or_else(|| std::env::temp_dir().join("takyon-scratch"));
    let _ = std::fs::create_dir_all(&path);
    path
}

/// The directory a Turn should run in: the user's choice, or Scratch.
///
/// A configured path that no longer exists falls back rather than failing — an
/// unplugged drive must not stop `!c` from answering.
pub fn resolve(configured: Option<&str>) -> PathBuf {
    match configured.map(str::trim).filter(|p| !p.is_empty()) {
        Some(path) if PathBuf::from(path).is_dir() => PathBuf::from(path),
        _ => dir(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An empty or blank setting means Scratch, not a Turn in the process cwd —
    /// which for a launcher is wherever Windows happened to start it.
    #[test]
    fn v0_9_a_blank_working_directory_setting_means_scratch() {
        assert_eq!(resolve(None), dir());
        assert_eq!(resolve(Some("")), dir());
        assert_eq!(resolve(Some("   ")), dir());
    }

    /// A configured directory that has gone away falls back rather than failing.
    #[test]
    fn v0_9_a_missing_configured_directory_falls_back_to_scratch() {
        assert_eq!(resolve(Some(r"Z:\not\a\real\path")), dir());
    }

    /// A real directory is used as given.
    #[test]
    fn v0_9_an_existing_configured_directory_is_used() {
        let temp = std::env::temp_dir();
        let as_text = temp.to_string_lossy().to_string();
        assert_eq!(resolve(Some(&as_text)), temp);
    }

    /// Scratch exists after being asked for, or every Turn starts by failing.
    #[test]
    fn v0_9_the_scratch_directory_is_created_on_demand() {
        assert!(dir().is_dir());
    }
}
