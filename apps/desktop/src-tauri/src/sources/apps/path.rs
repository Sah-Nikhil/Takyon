//! Executables reachable on `PATH`.
//!
//! A separate path rather than a nicety: `node`, `ffmpeg`, `gh`, `rg` and `bun`
//! install no Start Menu shortcut, and "it can't find the thing I use forty times
//! a day" is how a launcher stops being used.
//!
//! These score on the executable-basename rung (650), below every rung a real
//! display name reaches. `System32` alone contributes a thousand executables, so
//! without that ordering `co` returns `comp.exe` before Google Chrome.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Extensions treated as launchable.
///
/// A deliberate subset of `PATHEXT`, which also lists `.VBS`, `.JS` and friends.
/// Those are scripts Windows will run, not applications, and offering to execute
/// a stray `.js` on one keystroke is a footgun wearing a hat.
#[cfg(windows)]
const LAUNCHABLE: &[&str] = &["exe", "com", "bat", "cmd"];

/// Most executables to take from any one directory.
///
/// Not a real limit for a sane `PATH` — `System32` is the biggest at roughly
/// fifteen hundred — but a `PATH` entry pointed at a directory of a hundred
/// thousand files would otherwise stall discovery with no way to tell why.
const MAX_PER_DIR: usize = 4000;

/// Windows-directory exes the shell already offers as an app.
///
/// `calc`/`notepad` stub into packaged apps; `explorer` is File Explorer's real
/// binary. All appear in `AppsFolder`, so the bare exe duplicates a row no path
/// match can catch (an `AppsFolder` app has no path). Curated; `docs/tbd/v0.3.md` §7.
#[cfg(windows)]
const WINDOWS_DIR_APP_DUPLICATES: &[&str] = &["calc", "notepad", "explorer"];

/// Is this a Windows-directory exe the shell already surfaces as its own app?
///
/// Under the Windows dir, not just `System32` (`notepad.exe` ships in both). The
/// directory is checked, so someone's own `calc.exe` elsewhere on `PATH` stays.
#[cfg(windows)]
pub fn is_windows_app_duplicate(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    if !WINDOWS_DIR_APP_DUPLICATES
        .iter()
        .any(|s| s.eq_ignore_ascii_case(stem))
    {
        return false;
    }
    is_under_windows_dir(path)
}

/// Is this path inside the Windows directory?
///
/// From `%SystemRoot%` rather than a hardcoded `C:\Windows`, because Windows can be
/// installed on another volume and the stubs move with it.
#[cfg(windows)]
fn is_under_windows_dir(path: &Path) -> bool {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    let norm = |s: &str| s.to_lowercase().replace('/', "\\");
    norm(&path.to_string_lossy()).starts_with(&format!("{}\\", norm(&root)))
}

/// One executable found on `PATH`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathExe {
    /// The basename without its extension — `code` for `Code.exe`. This is both
    /// the display title and what the basename rung matches against.
    pub stem: String,
    pub path: PathBuf,
}

/// Split a raw `PATH` value into usable directories, in resolution order.
///
/// `split_paths` carries the platform separator and Windows' quoting rule. What
/// is added is dropping empty segments (the current directory, `System32` for a
/// login-launched process) and relative ones — `\Windows` as well as `..\bin`.
pub fn split_path_var<S: AsRef<OsStr>>(raw: S) -> Vec<PathBuf> {
    std::env::split_paths(raw.as_ref())
        .filter(|dir| dir.is_absolute())
        .collect()
}

/// Is this filename something worth offering to launch?
#[cfg(windows)]
pub fn is_launchable(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| LAUNCHABLE.iter().any(|l| e.eq_ignore_ascii_case(l)))
        .unwrap_or(false)
}

/// Walk the given directories, one [`PathExe`] per basename.
///
/// **First occurrence wins** — what the shell itself does. If two `PATH`
/// directories hold `python.exe`, the launcher must offer the one a terminal
/// would run, not a different program under the same word.
pub fn discover_in(dirs: &[PathBuf]) -> Vec<PathExe> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();

    for dir in dedupe_dirs(dirs) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            // A `PATH` entry pointing at a removed drive or a directory that never
            // existed is completely ordinary. Skip it silently.
            continue;
        };
        let mut taken = 0usize;
        for entry in entries.flatten() {
            if taken >= MAX_PER_DIR {
                break;
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !offerable(&entry, name) {
                continue;
            }
            let full = entry.path();
            #[cfg(windows)]
            if is_windows_app_duplicate(&full) {
                continue;
            }
            let stem = stem_of(name);
            if !seen.insert(stem.to_lowercase()) {
                continue;
            }
            taken += 1;
            out.push(PathExe { stem, path: full });
        }
    }
    out
}

/// Drop `PATH` directories already walked, comparing case-insensitively.
///
/// Not a micro-optimisation: this machine lists `C:\Windows\system32` and
/// `C:\WINDOWS\system32` and repeats the whole Windows set four times, so the walk
/// read 627 files four times over. Order is preserved, because
/// first-occurrence-wins is what makes the result match the shell.
fn dedupe_dirs(dirs: &[PathBuf]) -> Vec<&PathBuf> {
    let mut seen = HashSet::new();
    dirs.iter().filter(|d| seen.insert(dir_key(d))).collect()
}

#[cfg(windows)]
fn dir_key(dir: &Path) -> String {
    dir.to_string_lossy()
        .to_lowercase()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_string()
}

/// No case folding: two unix directories differing in case are two directories,
/// whatever the volume happens to do about it.
#[cfg(not(windows))]
fn dir_key(dir: &Path) -> String {
    dir.to_string_lossy().trim_end_matches('/').to_string()
}

/// Is this directory entry a program worth offering?
///
/// Windows answers from the name; `file_type` comes free with the entry. Unix
/// has to `metadata` each candidate for the exec bit, which follows symlinks —
/// every Homebrew binary is one, and a dangling link is not a program.
#[cfg(windows)]
fn offerable(entry: &std::fs::DirEntry, name: &str) -> bool {
    is_launchable(name) && !entry.file_type().map(|t| t.is_dir()).unwrap_or(false)
}

#[cfg(unix)]
fn offerable(entry: &std::fs::DirEntry, name: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    // A dotfile in `/usr/local/bin` is configuration, never a command.
    if name.starts_with('.') {
        return false;
    }
    entry
        .metadata()
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// The name this executable is typed under.
///
/// Windows strips the extension — nobody types `code.cmd`. Unix keeps the whole
/// name: `python3.11` is the command, and `file_stem` would offer `python3`.
#[cfg(windows)]
fn stem_of(name: &str) -> String {
    Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name)
        .to_string()
}

#[cfg(not(windows))]
fn stem_of(name: &str) -> String {
    name.to_string()
}

/// Everything launchable on `PATH`, hydrated where `shellenv` could hydrate it.
///
/// The hydrated value is the whole reason a `bun add -g` shows up without a
/// reboot; before v0.11 this read the `PATH` that existed at login.
pub fn discover() -> Vec<PathExe> {
    let raw = crate::agents::shellenv::hydrated_path()
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_default();
    discover_in(&split_path_var(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn v0_2_a_plain_path_splits_in_order() {
        let dirs = split_path_var(r"C:\Windows\System32;C:\Program Files\Git\cmd");
        assert_eq!(
            dirs,
            vec![
                PathBuf::from(r"C:\Windows\System32"),
                PathBuf::from(r"C:\Program Files\Git\cmd"),
            ]
        );
    }

    /// An empty segment means "the current directory". Searching it would make the
    /// results depend on where the process happened to be started, and for a
    /// login-launched process that is `System32`.
    #[cfg(windows)]
    #[test]
    fn v0_2_empty_path_segments_are_dropped() {
        let dirs = split_path_var(r";;C:\bin;;");
        assert_eq!(dirs, vec![PathBuf::from(r"C:\bin")]);
    }

    #[cfg(windows)]
    #[test]
    fn v0_2_a_quoted_segment_may_contain_a_semicolon() {
        let dirs = split_path_var(r#""C:\odd;dir";C:\bin"#);
        assert_eq!(
            dirs,
            vec![PathBuf::from(r"C:\odd;dir"), PathBuf::from(r"C:\bin")]
        );
    }

    #[cfg(windows)]
    #[test]
    fn v0_2_relative_and_drive_relative_entries_are_dropped() {
        let dirs = split_path_var(r"..\bin;.;C:\real;bin");
        assert_eq!(dirs, vec![PathBuf::from(r"C:\real")]);
    }

    #[cfg(windows)]
    #[test]
    fn v0_2_only_real_executables_are_launchable() {
        assert!(is_launchable("code.exe"));
        assert!(is_launchable("CODE.EXE"));
        assert!(is_launchable("build.cmd"));
        assert!(is_launchable("setup.bat"));
        // Libraries are not programs.
        assert!(!is_launchable("vcruntime140.dll"));
        assert!(!is_launchable("readme"));
        // On PATHEXT but deliberately excluded: offering to run a stray script on
        // one keystroke is a footgun.
        assert!(!is_launchable("install.vbs"));
        assert!(!is_launchable("tool.ps1"));
    }

    /// `calc.exe` in System32 exits immediately having started the packaged
    /// Calculator, which `AppsFolder` already lists under its real name. Verified
    /// by running it: the stub exits, `CalculatorApp.exe` appears.
    #[cfg(windows)]
    #[test]
    fn v0_2_a_windows_dir_app_duplicate_is_dropped() {
        // Both copies: `notepad.exe` ships in `C:\Windows` as well as in
        // `System32`, and both are on `PATH`.
        for exe in ["calc", "notepad"] {
            for dir in [r"C:\Windows\System32", r"C:\Windows"] {
                let p = format!(r"{dir}\{exe}.exe");
                assert!(is_windows_app_duplicate(Path::new(&p)), "{p}");
            }
        }
        // Case-insensitive on the name and the directory.
        assert!(is_windows_app_duplicate(Path::new(r"c:\windows\system32\CALC.EXE")));
    }

    /// `explorer.exe` is File Explorer's binary, and File Explorer is already a
    /// shell app (the `AppsFolder` AUMID). The bare exe is the duplicate the user
    /// sees as a second "File Explorer".
    #[cfg(windows)]
    #[test]
    fn v0_3_the_bare_explorer_exe_is_a_shell_app_duplicate() {
        for p in [r"C:\Windows\explorer.exe", r"c:\windows\EXPLORER.EXE"] {
            assert!(is_windows_app_duplicate(Path::new(p)), "{p}");
        }
        // A shortcut that runs explorer.exe with arguments is a different Entry
        // (task 0) and is not touched here — this predicate only sees the bare
        // PATH walk, never the `.lnk` walk.
    }

    /// The list has to stay short. These are real programs that live in the same
    /// directory, and hiding one would be far worse than showing a duplicate.
    #[cfg(windows)]
    #[test]
    fn v0_2_real_system32_tools_are_not_treated_as_shims() {
        // `charmap` and `msinfo32` were checked by running them: both stay up, so
        // both are real. `bash`, `wsl` and `wslconfig` share a name with a Store
        // alias but are the launchers people actually type.
        for exe in ["charmap", "msinfo32", "cmd", "regedit", "where", "wsl", "bash"] {
            let p = format!(r"C:\Windows\System32\{exe}.exe");
            assert!(!is_windows_app_duplicate(Path::new(&p)), "{exe}");
        }
    }

    /// Only Windows' own copies are shims. Someone's `calc.exe` elsewhere on
    /// `PATH` is a program they installed, and stays.
    #[cfg(windows)]
    #[test]
    fn v0_2_a_shim_name_outside_the_windows_directory_is_kept() {
        for path in [
            r"C:\tools\calc.exe",
            r"C:\Users\me\bin\notepad.exe",
            r"D:\Windows-Utils\calc.exe",
        ] {
            assert!(!is_windows_app_duplicate(Path::new(path)), "{path}");
        }
    }

    /// `PATH` here repeats the Windows directories four times, in two casings.
    /// Walking each once is the difference between reading 627 files and 2,508.
    #[cfg(windows)]
    #[test]
    fn v0_2_repeated_path_directories_are_walked_once() {
        let dirs = split_path_var(
            r"C:\Windows\system32;C:\Windows;C:\WINDOWS\system32;C:\Windows\system32\;C:\bin",
        );
        let unique = dedupe_dirs(&dirs);
        assert_eq!(unique.len(), 3, "system32, Windows, bin");
        // Order survives: first-occurrence-wins is what matches the shell.
        assert!(unique[0].to_string_lossy().to_lowercase().ends_with("system32"));
        assert_eq!(unique[2], &PathBuf::from(r"C:\bin"));
    }

    /// Shell resolution order, as a test. If two `PATH` directories both hold
    /// `python.exe`, the launcher must offer the same one a terminal would run.
    #[cfg(windows)]
    #[test]
    fn v0_2_the_first_directory_on_path_wins_a_name_collision() {
        let dir = std::env::temp_dir().join("takyon-path-test");
        let first = dir.join("first");
        let second = dir.join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(first.join("python.exe"), b"").unwrap();
        std::fs::write(second.join("python.exe"), b"").unwrap();

        let found = discover_in(&[first.clone(), second]);
        let pythons: Vec<_> = found.iter().filter(|e| e.stem == "python").collect();
        assert_eq!(pythons.len(), 1, "one basename, one Entry");
        assert!(pythons[0].path.starts_with(&first));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A `PATH` entry pointing at a drive that is not mounted is completely
    /// ordinary, and must cost nothing.
    #[test]
    fn v0_2_a_missing_path_directory_is_skipped_silently() {
        let found = discover_in(&[PathBuf::from(r"Z:\nope\nothing\here")]);
        assert!(found.is_empty());
    }

    /// The unix separator, and relative entries dropped for the same reason
    /// they are on Windows.
    #[cfg(unix)]
    #[test]
    fn v0_11_a_unix_path_splits_on_colons() {
        let dirs = split_path_var("/opt/homebrew/bin:/usr/bin::../bin:.:/sbin");
        assert_eq!(
            dirs,
            vec![
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/sbin"),
            ]
        );
    }

    /// Unix has no launchable extension: the exec bit is the whole test, and a
    /// `README` sitting in a `bin` directory must not become an Entry.
    #[cfg(unix)]
    #[test]
    fn v0_11_only_executable_files_are_offered_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join("takyon-unix-path-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, mode) in [("ripgrep", 0o755), ("README", 0o644), (".keep", 0o755)] {
            let file = dir.join(name);
            std::fs::write(&file, b"").unwrap();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
        }
        let found = discover_in(std::slice::from_ref(&dir));
        let names: Vec<&str> = found.iter().map(|e| e.stem.as_str()).collect();
        assert_eq!(names, vec!["ripgrep"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `python3.11` is the command. Stripping what looks like an extension
    /// would offer `python3`, which on this machine may be a different binary.
    #[cfg(unix)]
    #[test]
    fn v0_11_a_unix_command_keeps_its_whole_name() {
        assert_eq!(stem_of("python3.11"), "python3.11");
        assert_eq!(stem_of("rg"), "rg");
    }

    /// The `PATH` walked is the hydrated one where there is one. Asserted by
    /// shape, never by which executables this machine happens to hold.
    #[test]
    fn v0_11_discovery_reads_the_hydrated_path() {
        let _guard = crate::agents::shellenv::TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::agents::shellenv::invalidate();
        crate::agents::shellenv::hydrate();
        let hydrated = crate::agents::shellenv::hydrated_path();
        let walked = split_path_var(
            hydrated
                .clone()
                .or_else(|| std::env::var_os("PATH"))
                .unwrap_or_default(),
        );
        // Every directory `discover` reports from is one of the walked ones.
        for exe in discover() {
            let parent = exe.path.parent().map(|p| p.to_path_buf());
            assert!(
                parent.is_some_and(|p| walked.iter().any(|d| dir_key(d) == dir_key(&p))),
                "{} came from outside the walked PATH",
                exe.path.display()
            );
        }
        crate::agents::shellenv::invalidate();
    }
}
