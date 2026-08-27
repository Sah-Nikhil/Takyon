//! Start Menu shortcuts, from the per-user and machine-wide trees.
//!
//! The path that finds what a person thinks of as "my programs", and the one with
//! the trap in it: **`IShellLinkW::Resolve` is never called.** It searches the
//! volume, blocks on UNC targets, and can trigger an MSI repair dialog from a
//! background thread. ADR-0013 has the reasoning and what is given up.
//!
//! Instead: `SLGP_RAWPATH`, expand the environment variables here, check the
//! target exists.

use std::path::{Path, PathBuf};

/// How deep the Start Menu walk goes.
///
/// The real tree is two or three levels ("Programs\Vendor\App.lnk"). The cap is
/// here because a directory symlink pointing at an ancestor turns the walk into an
/// infinite one, and the Start Menu is a place users can and do create folders.
const MAX_DEPTH: usize = 8;

/// One shortcut, already read and verified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shortcut {
    /// The shortcut's filename without `.lnk` — this is the name the user sees in
    /// the Start Menu, and the one they will type.
    pub name: String,
    /// The resolved, environment-expanded target. Guaranteed to exist at the
    /// moment discovery ran.
    pub target: PathBuf,
    pub args: Option<String>,
    pub working_dir: Option<PathBuf>,
    /// The `.lnk` itself, kept because it is the better icon source: a shortcut
    /// may carry its own icon that differs from the target executable's.
    pub link: PathBuf,
}

/// The two Start Menu roots.
///
/// Both, always. Which tree an app lands in is decided by its installer, not by
/// the user, so reading one silently loses about a third of the machine — 41
/// user against 112 machine-wide here.
pub fn start_menu_roots() -> Vec<PathBuf> {
    ["APPDATA", "PROGRAMDATA"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|base| {
            PathBuf::from(base)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
        })
        .collect()
}

/// Expand `%VAR%` references in a stored shortcut target.
///
/// Hand-rolled, so it is testable without a process environment and so an unset
/// variable keeps its reference: `%NOPE%\app.exe` collapsing to `\app.exe` turns
/// a broken shortcut into a path that might accidentally exist.
pub fn expand_env(raw: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;

    while let Some(start) = rest.find('%') {
        let (before, after) = rest.split_at(start);
        out.push_str(before);
        let after = &after[1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match lookup(name) {
                    Some(value) => out.push_str(&value),
                    None => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                // A lone `%` with no closing partner. Literal.
                out.push('%');
                out.push_str(after);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Moved to `noise.rs` at v0.3 so `appsfolder.rs` reaches it too. Re-exported
/// because this module's callers and tests already name it here.
pub use super::noise::is_noise;

/// Every `.lnk` under `root`, recursively.
///
/// Split from the COM half so the walk can be tested with ordinary files.
pub fn find_links(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(root, 0, &mut out);
    out
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        let path = entry.path();
        if kind.is_dir() {
            walk(&path, depth + 1, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("lnk"))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
}

/// Is this target inside a Squirrel version directory (`...\\app-1.0.9253\\`)?
///
/// Squirrel installs — Discord, Slack, Teams classic, GitHub Desktop — ship two
/// Start Menu shortcuts with the same name: one at the versioned executable, one
/// at an `Update.exe` stub that always launches the current version. The versioned
/// path is the wrong one to keep: it dies at the next update, and since it is also
/// the [`crate::entry::EntryId`], everything v0.3 learned about the app dies with
/// it (§2 requires that id to be stable).
pub fn is_versioned_target(target: &Path) -> bool {
    target.components().any(|c| {
        let Some(name) = c.as_os_str().to_str() else {
            return false;
        };
        let Some(rest) = name.strip_prefix("app-") else {
            return false;
        };
        // `app-1.0.9253`, not `app-data` — digits and dots only, and at least one
        // digit, or an ordinary directory called `app-something` is thrown away.
        !rest.is_empty()
            && rest.chars().any(|c| c.is_ascii_digit())
            && rest.chars().all(|c| c.is_ascii_digit() || c == '.')
    })
}

/// Collapse shortcuts that share a display name.
///
/// Two `.lnk` files with one name are two rows for one application, and the user
/// cannot tell which to press. Where the collision is Squirrel's (see
/// [`is_versioned_target`]) the stable stub wins; otherwise the first found wins,
/// which is the per-user tree before the machine-wide one.
///
/// Pure, so the rule is testable without a Start Menu.
pub fn collapse_by_name(shortcuts: Vec<Shortcut>) -> Vec<Shortcut> {
    let mut out: Vec<Shortcut> = Vec::new();
    for sc in shortcuts {
        match out
            .iter()
            .position(|kept| kept.name.eq_ignore_ascii_case(&sc.name))
        {
            None => out.push(sc),
            Some(i) => {
                if is_versioned_target(&out[i].target) && !is_versioned_target(&sc.target) {
                    out[i] = sc;
                }
            }
        }
    }
    out
}

/// The display name for a shortcut path: its filename without `.lnk`.
pub fn display_name(link: &Path) -> Option<String> {
    link.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
}

#[cfg(windows)]
mod com {
    use super::*;
    use windows::core::{Interface, PCWSTR};
    use windows::Win32::System::Com::{
        CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink, SLGP_RAWPATH};

    /// Read one `.lnk` without resolving it. See the module docs for why.
    ///
    /// `None` for anything not a launchable file: a UWP shortcut (no stored path),
    /// a control-panel applet, or a target that is gone. The last is the plan's
    /// "drop dead ones at index time", so no row can exist that only fails on Enter.
    pub fn read(link: &Path) -> Option<Shortcut> {
        let name = display_name(link)?;
        if is_noise(&name) {
            return None;
        }

        unsafe {
            let shell_link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
            let persist: IPersistFile = shell_link.cast().ok()?;

            let wide = to_wide(link);
            persist.Load(PCWSTR(wide.as_ptr()), STGM_READ).ok()?;

            // SLGP_RAWPATH returns what is stored, unexpanded and unresolved. The
            // expansion below is ours; the resolution deliberately never happens.
            let mut buf = [0u16; 1024];
            shell_link.GetPath(&mut buf, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32).ok()?;
            let raw = from_wide(&buf);
            if raw.is_empty() {
                return None;
            }

            let expanded = expand_env(&raw, &|name| std::env::var(name).ok());
            let target = PathBuf::from(&expanded);
            // The existence check *is* the resolution. A shortcut whose target has
            // gone is a row that can only fail.
            if !target.is_file() {
                return None;
            }

            let mut arg_buf = [0u16; 1024];
            let args = shell_link
                .GetArguments(&mut arg_buf)
                .ok()
                .map(|_| from_wide(&arg_buf))
                .filter(|s| !s.is_empty());

            let mut dir_buf = [0u16; 1024];
            let working_dir = shell_link
                .GetWorkingDirectory(&mut dir_buf)
                .ok()
                .map(|_| from_wide(&dir_buf))
                .filter(|s| !s.is_empty())
                .map(|s| PathBuf::from(expand_env(&s, &|n| std::env::var(n).ok())));

            Some(Shortcut {
                name,
                target,
                args,
                working_dir,
                link: link.to_path_buf(),
            })
        }
    }

    fn to_wide(path: &Path) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
    }

    fn from_wide(buf: &[u16]) -> String {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len])
    }
}

/// Read every shortcut under both Start Menu roots.
///
/// The caller is responsible for having initialised COM on this thread — see
/// `sources/apps.rs`, which does it once for the whole discovery pass rather than
/// once per shortcut.
#[cfg(windows)]
pub fn discover() -> Vec<Shortcut> {
    let found: Vec<Shortcut> = start_menu_roots()
        .iter()
        .flat_map(|root| find_links(root))
        .filter_map(|link| com::read(&link))
        .collect();
    collapse_by_name(found)
}

#[cfg(not(windows))]
pub fn discover() -> Vec<Shortcut> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_2_environment_variables_in_a_target_are_expanded() {
        let lookup = |name: &str| match name {
            "ProgramFiles" => Some(r"C:\Program Files".to_string()),
            _ => None,
        };
        assert_eq!(
            expand_env(r"%ProgramFiles%\App\app.exe", &lookup),
            r"C:\Program Files\App\app.exe"
        );
    }

    /// An unset variable must leave its reference intact. Expanding it to nothing
    /// would turn `%NOPE%\app.exe` into `\app.exe`, which is a real path on the
    /// current drive and might well exist.
    #[test]
    fn v0_2_an_unset_variable_is_left_alone_rather_than_erased() {
        let none = |_: &str| None;
        assert_eq!(expand_env(r"%NOPE%\app.exe", &none), r"%NOPE%\app.exe");
    }

    #[test]
    fn v0_2_a_target_with_no_variables_is_unchanged() {
        let none = |_: &str| None;
        assert_eq!(
            expand_env(r"C:\Windows\notepad.exe", &none),
            r"C:\Windows\notepad.exe"
        );
        // A lone percent sign is literal, not the start of anything.
        assert_eq!(expand_env(r"C:\100% Orange Juice\game.exe", &none), r"C:\100% Orange Juice\game.exe");
    }



    #[test]
    fn v0_2_both_start_menu_roots_are_walked() {
        // Which root an app lands in is decided by its installer, not by the user.
        // Reading one loses roughly a third of this machine (41 user, 112 machine).
        let roots = start_menu_roots();
        let joined = roots
            .iter()
            .map(|r| r.to_string_lossy().to_lowercase())
            .collect::<Vec<_>>()
            .join("|");
        if std::env::var_os("APPDATA").is_some() && std::env::var_os("PROGRAMDATA").is_some() {
            assert_eq!(roots.len(), 2);
            assert!(joined.contains("appdata") || joined.contains("roaming"));
            assert!(joined.contains("programdata"));
        }
        for root in &roots {
            assert!(root.ends_with(r"Start Menu\Programs"));
        }
    }

    #[test]
    fn v0_2_the_walk_finds_nested_shortcuts_and_ignores_other_files() {
        let dir = std::env::temp_dir().join("takyon-lnk-walk");
        let nested = dir.join("Vendor").join("Suite");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(dir.join("Top.lnk"), b"").unwrap();
        std::fs::write(nested.join("Deep.lnk"), b"").unwrap();
        std::fs::write(nested.join("notes.txt"), b"").unwrap();
        std::fs::write(nested.join("Mixed.LNK"), b"").unwrap();

        let found = find_links(&dir);
        let names: Vec<String> = found.iter().filter_map(|p| display_name(p)).collect();
        assert!(names.contains(&"Top".to_string()));
        assert!(names.contains(&"Deep".to_string()));
        assert!(names.contains(&"Mixed".to_string()), "extension match is case-insensitive");
        assert!(!names.contains(&"notes".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn shortcut(name: &str, target: &str) -> Shortcut {
        Shortcut {
            name: name.into(),
            target: PathBuf::from(target),
            args: None,
            working_dir: None,
            link: PathBuf::from(format!(r"C:\menu\{name}.lnk")),
        }
    }

    /// Squirrel's version directory, which is what makes one of the two Discord
    /// shortcuts a bad Entry: the path dies at the next update, and it is the id.
    #[test]
    fn v0_2_a_squirrel_version_directory_is_recognised() {
        assert!(is_versioned_target(Path::new(
            r"C:\Users\me\AppData\Local\Discord\app-1.0.9253\Discord.exe"
        )));
        assert!(is_versioned_target(Path::new(r"C:\x\app-2.14\a.exe")));
    }

    /// The check has to be narrow. A directory merely starting with `app-` is
    /// ordinary, and treating it as versioned would discard the wrong shortcut.
    #[test]
    fn v0_2_an_ordinary_app_directory_is_not_versioned() {
        for path in [
            r"C:\Users\me\AppData\Local\Discord\Update.exe",
            r"C:\Program Files\Thing\app-data\thing.exe",
            r"C:\Program Files\app\thing.exe",
            r"C:\Program Files\app-\thing.exe",
        ] {
            assert!(!is_versioned_target(Path::new(path)), "{path}");
        }
    }

    /// The real pair from this machine: two Start Menu shortcuts, both "Discord",
    /// one at the versioned exe and one at the stub. The stub survives updates, so
    /// it wins regardless of which was found first.
    #[test]
    fn v0_2_the_squirrel_stub_wins_over_the_versioned_executable() {
        let versioned = r"C:\Users\me\AppData\Local\Discord\app-1.0.9253\Discord.exe";
        let stub = r"C:\Users\me\AppData\Local\Discord\Update.exe";

        for pair in [
            vec![shortcut("Discord", versioned), shortcut("Discord", stub)],
            vec![shortcut("Discord", stub), shortcut("Discord", versioned)],
        ] {
            let kept = collapse_by_name(pair);
            assert_eq!(kept.len(), 1, "one application, one row");
            assert_eq!(kept[0].target, PathBuf::from(stub));
        }
    }

    /// Only the name collides. Two genuinely different applications keep both rows.
    #[test]
    fn v0_2_different_names_are_never_collapsed() {
        let kept = collapse_by_name(vec![
            shortcut("Discord", r"C:\a\Discord.exe"),
            shortcut("Discord Canary", r"C:\b\DiscordCanary.exe"),
        ]);
        assert_eq!(kept.len(), 2);
    }

    /// With no Squirrel stub to prefer, the first found wins — the per-user tree
    /// before the machine-wide one. Arbitrary, but stable between runs, which is
    /// what stops the list reordering itself.
    #[test]
    fn v0_2_a_plain_name_collision_keeps_the_first() {
        let kept = collapse_by_name(vec![
            shortcut("Thing", r"C:\user\thing.exe"),
            shortcut("Thing", r"C:\machine\thing.exe"),
        ]);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].target, PathBuf::from(r"C:\user\thing.exe"));
    }

    #[test]
    fn v0_2_the_display_name_is_the_filename_without_lnk() {
        assert_eq!(
            display_name(Path::new(r"C:\x\Visual Studio Code.lnk")),
            Some("Visual Studio Code".to_string())
        );
    }
}
