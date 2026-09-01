//! Epic games, from the `.item` JSON manifests under `Data\Manifests`.
//!
//! **A manifest outlives its game.** Uninstalling leaves the `.item` file behind:
//! all seven on this machine point into an empty directory, and Raycast lists all
//! seven as launchable anyway. So the executable is existence-checked and a stale
//! manifest is dropped — which here means the whole Source contributes nothing,
//! correctly.
//!
//! DLC falls out of the same check rather than needing a rule: a DLC manifest
//! carries an empty `LaunchExecutable`, so it names no file.

use std::path::{Path, PathBuf};

use super::{Game, GameLibrary};
use crate::entry::GameLauncher;

/// Epic's installed library.
pub struct EpicLibrary {
    manifests: PathBuf,
}

impl EpicLibrary {
    /// Present only if the manifest directory is. Machine-wide, not per-user.
    pub fn detect() -> Option<Self> {
        let root = std::env::var_os("ProgramData")?;
        let manifests = PathBuf::from(root).join(r"Epic\EpicGamesLauncher\Data\Manifests");
        manifests.is_dir().then_some(EpicLibrary { manifests })
    }

    /// A library rooted anywhere, for a test that writes its own manifests.
    pub fn at(manifests: impl Into<PathBuf>) -> Self {
        EpicLibrary {
            manifests: manifests.into(),
        }
    }
}

impl GameLibrary for EpicLibrary {
    fn launcher(&self) -> GameLauncher {
        GameLauncher::Epic
    }

    fn games(&self) -> Vec<Game> {
        discover(&self.manifests)
    }
}

/// One manifest, reduced to what a launch needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub app_name: String,
    pub display_name: String,
    pub install_location: PathBuf,
    pub launch_executable: String,
}

impl Manifest {
    /// Where the game's executable would be, if this manifest names one.
    ///
    /// `None` is what a DLC entry looks like. Joining an empty name would yield
    /// the install directory, which is a real path and would pass an existence
    /// check as a directory.
    pub fn executable(&self) -> Option<PathBuf> {
        if self.launch_executable.is_empty() {
            return None;
        }
        Some(self.install_location.join(&self.launch_executable))
    }
}

/// Read one `.item` manifest.
///
/// `None` for anything missing its id or its name. Epic writes a manifest before
/// the download finishes, and a half-written one means "not installed yet" rather
/// than "corrupt".
pub fn parse_manifest(input: &str) -> Option<Manifest> {
    let doc: serde_json::Value = serde_json::from_str(input).ok()?;
    let field = |key: &str| {
        doc.get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim()
    };

    let app_name = field("AppName");
    let display_name = field("DisplayName");
    if app_name.is_empty() || display_name.is_empty() {
        return None;
    }
    Some(Manifest {
        app_name: app_name.to_string(),
        display_name: display_name.to_string(),
        install_location: PathBuf::from(field("InstallLocation")),
        launch_executable: field("LaunchExecutable").to_string(),
    })
}

/// Every installed Epic game in a manifest directory.
///
/// An unreadable or malformed manifest costs that one game, not the library —
/// Epic rewrites these during an update, so reading a half-written one is routine.
pub fn discover(manifests: &Path) -> Vec<Game> {
    let Ok(dir) = std::fs::read_dir(manifests) else {
        return Vec::new();
    };

    let mut games = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in dir.flatten() {
        let path = item.path();
        if path.extension().and_then(|e| e.to_str()) != Some("item") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(manifest) = parse_manifest(&text) else {
            continue;
        };
        // The whole reason this Source is not a directory listing: the file has to
        // still be there. See the module doc.
        let Some(exe) = manifest.executable() else {
            continue;
        };
        if !exe.is_file() || !seen.insert(manifest.app_name.clone()) {
            continue;
        }
        games.push(Game {
            launcher: GameLauncher::Epic,
            id: manifest.app_name,
            name: manifest.display_name,
        });
    }
    games
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real manifest off this machine, trimmed of the fields nothing reads.
    /// Note `InstallLocation` mixing both separators — Epic writes it that way.
    const REAL_GAME: &str = r#"{
        "FormatVersion": 0,
        "bIsIncompleteInstall": false,
        "LaunchExecutable": "RunFallGuys.exe",
        "AppCategories": ["games", "applications"],
        "DisplayName": "Fall Guys",
        "InstallLocation": "C:\\GG\\EpicLibrary/FallGuys",
        "AppName": "0a2d9f6403244d12969e11da6713137b",
        "InstallSize": 6765621047
    }"#;

    /// Also real. A Dying Light add-on: named, sized, installed — and it launches
    /// nothing, because the base game is what launches.
    const REAL_DLC: &str = r#"{
        "LaunchExecutable": "",
        "DisplayName": "Dying Light The Following",
        "InstallLocation": "C:\\GG\\EpicLibrary/DyingLight",
        "AppName": "f653266870894ee1acbb5250e3b04bd1"
    }"#;

    #[test]
    fn v0_3_a_manifest_yields_its_app_name_and_display_name() {
        let manifest = parse_manifest(REAL_GAME).expect("a complete manifest parses");
        assert_eq!(manifest.app_name, "0a2d9f6403244d12969e11da6713137b");
        assert_eq!(manifest.display_name, "Fall Guys");
        assert_eq!(
            manifest.executable(),
            Some(PathBuf::from(r"C:\GG\EpicLibrary/FallGuys").join("RunFallGuys.exe"))
        );
    }

    #[test]
    fn v0_3_dlc_names_no_executable_so_it_can_never_be_launched() {
        let manifest = parse_manifest(REAL_DLC).expect("a DLC manifest still parses");
        assert_eq!(manifest.display_name, "Dying Light The Following");
        assert_eq!(manifest.executable(), None);
    }

    #[test]
    fn v0_3_a_manifest_without_an_id_or_a_name_is_not_a_game() {
        assert!(parse_manifest(r#"{"DisplayName": "Half a download"}"#).is_none());
        assert!(parse_manifest(r#"{"AppName": "abc"}"#).is_none());
        assert!(parse_manifest(r#"{"AppName": "abc", "DisplayName": "  "}"#).is_none());
        assert!(parse_manifest("not json at all").is_none());
    }

    /// The directory is absent on a machine with no Epic, and `read_dir` failing
    /// must cost nothing — this runs on the discovery thread at every login.
    #[test]
    fn v0_3_a_missing_manifest_directory_yields_no_games() {
        assert!(discover(Path::new(r"C:\no\such\epic\manifests")).is_empty());
    }
}
