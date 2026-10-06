//! Which running process a Block list app item means. Pure; the OS walk
//! that fills [`Process`] lives elsewhere.
//!
//! Keys (plan § Identity): `entry:<EntryId>` for picks from the app list,
//! `exe:<name>` and `package:<family>` for built-in Focus categories, whose
//! paths differ per machine.

use std::path::{Path, PathBuf};

/// What Focus knows of one running process.
#[derive(Clone, Debug, Default)]
pub struct Process {
    /// Full image path, long form (`QueryFullProcessImageNameW`).
    pub image: PathBuf,
    /// Store apps only.
    pub aumid: Option<String>,
    pub package_family: Option<String>,
    pub session_id: u32,
}

/// A key resolved against this machine, ready to match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// Exact image path, lowercased.
    Path(String),
    /// Any image under this directory (a game's install dir).
    Dir(String),
    ExeName(String),
    Package(String),
    Aumid(String),
}

/// Resolve a Block list key. `game_dir` maps a Game launcher slug and id to
/// its install directory; None when the key is not an app or the game is gone.
pub fn resolve(key: &str, game_dir: &dyn Fn(&str, &str) -> Option<PathBuf>) -> Option<Target> {
    let nonempty = |s: &str| (!s.is_empty()).then(|| s.to_lowercase());
    let (kind, rest) = key.split_once(':')?;
    match kind {
        "exe" => nonempty(rest).map(Target::ExeName),
        "package" => nonempty(rest).map(Target::Package),
        "entry" => entry(rest, game_dir),
        _ => None,
    }
}

/// An `EntryId` (`entry.rs`): lowercased path, `aumid:`, or `<launcher>:<id>`.
/// Settings pages, URIs and shell items are not processes.
fn entry(id: &str, game_dir: &dyn Fn(&str, &str) -> Option<PathBuf>) -> Option<Target> {
    let b = id.as_bytes();
    let is_path =
        (b.len() > 2 && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')) || id.starts_with(r"\\");
    if is_path {
        let path = id.split('|').next()?;
        return Some(Target::Path(norm(Path::new(path))));
    }
    match id.split_once(':')? {
        ("aumid", aumid) if !aumid.is_empty() => Some(Target::Aumid(aumid.to_lowercase())),
        (launcher @ ("steam" | "epic"), game) => {
            game_dir(launcher, game).map(|dir| Target::Dir(norm(&dir)))
        }
        _ => None,
    }
}

/// Lowercase, backslashes: one spelling per path.
pub(crate) fn norm(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\").to_lowercase()
}

/// Case-insensitive compare against an optional process field.
fn same(field: &Option<String>, want: &str) -> bool {
    field.as_deref().is_some_and(|f| f.to_lowercase() == want)
}

impl Target {
    pub fn matches(&self, process: &Process) -> bool {
        match self {
            Target::Path(path) => norm(&process.image) == *path,
            Target::Dir(dir) => norm(&process.image)
                .strip_prefix(dir.as_str())
                .is_some_and(|rest| rest.starts_with('\\')),
            Target::ExeName(name) => process
                .image
                .file_name()
                .is_some_and(|f| f.to_string_lossy().to_lowercase() == *name),
            Target::Package(family) => same(&process.package_family, family),
            Target::Aumid(aumid) => same(&process.aumid, aumid),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_games(_: &str, _: &str) -> Option<PathBuf> {
        None
    }

    fn exe(path: &str) -> Process {
        Process {
            image: PathBuf::from(path),
            session_id: 1,
            ..Default::default()
        }
    }

    fn matches(key: &str, process: &Process) -> bool {
        resolve(key, &no_games).is_some_and(|t| t.matches(process))
    }

    /// `EntryId`s are lowercased paths; arguments after `|` are not part of
    /// which program it is.
    #[test]
    fn v0_16_an_app_entry_matches_its_image_path_whatever_the_case_or_arguments() {
        let discord = exe(r"C:\Users\Me\AppData\Local\Discord\app-1.0\Discord.exe");
        assert!(matches(
            r"entry:c:\users\me\appdata\local\discord\app-1.0\discord.exe",
            &discord
        ));
        assert!(matches(
            r"entry:c:\users\me\appdata\local\discord\app-1.0\discord.exe|--start-minimized",
            &discord
        ));
        assert!(!matches(
            r"entry:c:\program files\slack\slack.exe",
            &discord
        ));
    }

    #[test]
    fn v0_16_built_in_keys_match_by_exe_name_or_package_family() {
        let discord = exe(r"D:\Apps\Discord\Discord.exe");
        assert!(matches("exe:discord.exe", &discord));
        assert!(matches("exe:Discord.EXE", &discord));
        assert!(!matches("exe:discord", &discord));

        let whatsapp = Process {
            image: PathBuf::from(
                r"C:\Program Files\WindowsApps\5319275A.WhatsAppDesktop_2.0_x64__cv1g1gvanyjgm\WhatsApp.exe",
            ),
            aumid: Some("5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App".into()),
            package_family: Some("5319275A.WhatsAppDesktop_cv1g1gvanyjgm".into()),
            session_id: 1,
        };
        assert!(matches(
            "package:5319275a.whatsappdesktop_cv1g1gvanyjgm",
            &whatsapp
        ));
        assert!(matches(
            "entry:aumid:5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App",
            &whatsapp
        ));
        assert!(!matches(
            "package:5319275a.whatsappdesktop_cv1g1gvanyjgm",
            &discord
        ));
    }

    /// A game Entry blocks every process under its install directory: most
    /// games run a launcher stub and a separate engine binary.
    #[test]
    fn v0_16_a_game_entry_matches_anything_under_its_install_dir() {
        let games = |launcher: &str, id: &str| {
            (launcher == "steam" && id == "730").then(|| {
                PathBuf::from(r"D:\Steam\steamapps\common\Counter-Strike Global Offensive")
            })
        };
        let target = resolve("entry:steam:730", &games).expect("installed game");
        assert!(target.matches(&exe(
            r"D:\Steam\steamapps\common\Counter-Strike Global Offensive\game\bin\win64\cs2.exe"
        )));
        assert!(!target.matches(&exe(
            r"D:\Steam\steamapps\common\Counter-Strike Global Offensive 2\x.exe"
        )));
        assert_eq!(resolve("entry:epic:fortnite", &games), None);
    }

    #[test]
    fn v0_16_keys_that_are_not_apps_resolve_to_nothing() {
        for key in [
            "site:youtube.com",
            "entry:system:bluetooth",
            "entry:ms-settings:bluetooth",
            "entry:https://example.com",
            "entry:shell-item",
            "exe:",
            "package:",
            "nonsense",
        ] {
            assert_eq!(resolve(key, &no_games), None, "{key}");
        }
    }
}
