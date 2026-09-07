//! The `PATH` the user's own environment has, not the one we were launched with.
//!
//! A GUI process inherits the `PATH` that existed at login: on macOS that is
//! `launchd`'s four directories, on Windows a snapshot taken before the last
//! `bun add -g`. Every Agent CLI installs outside it, so `probe::resolve`
//! reports three Agents missing on a machine that has all three.
//!
//! Two mechanisms, one per platform. Unix asks the user's login shell, because
//! `nvm`, `asdf` and `mise` live in `.zshrc` and nothing else knows them.
//! Windows reads the registry: faster, cannot hang, cannot prompt. Both are off
//! every latency budget — `hydrate` runs on the deferred-init thread.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;

/// Markers the shell script prints around the value.
///
/// An interactive login shell also prints a motd, an oh-my-zsh banner, `direnv`
/// chatter and escape codes, so reading stdout whole gives garbage.
const SENTINEL_START: &str = "__TAKYON_ENV_PATH_START__";
const SENTINEL_END: &str = "__TAKYON_ENV_PATH_END__";

/// What hydration found, for Settings to show.
///
/// Not decoration: when someone reports "`!c` says Claude isn't installed",
/// which mechanism answered and how many entries it recovered is the first
/// thing worth knowing, and without it the failure looks like a probe bug.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// What answered: a shell's own name, `launchctl`, or `registry`. Absent
    /// when nothing did and the inherited `PATH` is all there is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Directories on the hydrated `PATH`.
    pub entries: usize,
    /// How many of those the inherited `PATH` did not have.
    pub added: usize,
}

struct Hydrated {
    /// `None` when discovery found nothing: callers fall back to inherited.
    path: Option<OsString>,
    report: Report,
}

static CACHE: Mutex<Option<Hydrated>> = Mutex::new(None);

/// Held by every test that hydrates or invalidates. The cache is process-wide,
/// and `cargo test` runs the lib tests in parallel.
#[cfg(test)]
pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());

/// The `PATH` to search, hydrated from the user's own environment.
///
/// `None` until `hydrate` has run, and after it if nothing answered. Never
/// blocks on discovery: callers that arrive first use the inherited `PATH`.
pub fn hydrated_path() -> Option<OsString> {
    let guard = CACHE.lock().ok()?;
    guard.as_ref()?.path.clone()
}

/// What the last `hydrate` found, or `None` before one has run.
pub fn report() -> Option<Report> {
    let guard = CACHE.lock().ok()?;
    Some(guard.as_ref()?.report.clone())
}

/// Discover and cache. Runs on the deferred-init thread, never on startup.
///
/// Idempotent: a second call while the cache is warm does nothing, so Settings
/// and a re-probe can both ask without paying for another shell. The lock is
/// never held across discovery, which spawns a process.
pub fn hydrate() {
    if matches!(CACHE.lock(), Ok(guard) if guard.is_some()) {
        return;
    }
    fill(false);
}

/// Discover again, asking the sources that cost a process.
///
/// Windows only: the PowerShell profile, where `fnm` puts a per-session node the
/// registry cannot see. Runs **after a probe has already failed**, never on the
/// deferred-init thread (`docs/tbd/v0.11.md` §4).
#[cfg(windows)]
pub fn hydrate_deep() {
    invalidate();
    fill(true);
}

/// Unix has nothing deeper: `-i` already sourced the rc files, which is where
/// `nvm`, `asdf` and `mise` install themselves.
#[cfg(not(windows))]
pub fn hydrate_deep() {
    hydrate();
}

fn fill(deep: bool) {
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let (discovered, source) = discover(deep);
    let hydrated = match discovered {
        Some(found) => {
            let merged = merge_path_entries(&found, &inherited);
            let entries = std::env::split_paths(&merged).count();
            let before = std::env::split_paths(&inherited)
                .filter(|dir| !dir.as_os_str().is_empty())
                .count();
            Hydrated {
                path: Some(merged),
                report: Report {
                    source,
                    entries,
                    added: entries.saturating_sub(before),
                },
            }
        }
        None => Hydrated {
            path: None,
            report: Report::default(),
        },
    };
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some(hydrated);
    }
}

/// Discard the cache so the next `hydrate` re-runs.
///
/// Called when a probe that previously succeeded starts failing: an Agent
/// uninstalled, or a Node version switched under `nvm`.
pub fn invalidate() {
    if let Ok(mut guard) = CACHE.lock() {
        *guard = None;
    }
}

/// Union `discovered` with `inherited`, discovered first.
///
/// Order survives and duplicates collapse, case-insensitively on Windows where
/// `C:\Windows\System32` and `C:\WINDOWS\system32` are one directory listed
/// twice. `split_paths` is quote-aware, so a quoted entry stays one entry.
pub fn merge_path_entries(discovered: &OsStr, inherited: &OsStr) -> OsString {
    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<PathBuf> = Vec::new();
    for dir in std::env::split_paths(discovered).chain(std::env::split_paths(inherited)) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let key = dedupe_key(&dir);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(dir);
    }
    std::env::join_paths(out).unwrap_or_else(|_| discovered.to_os_string())
}

#[cfg(windows)]
fn dedupe_key(dir: &std::path::Path) -> String {
    dir.to_string_lossy()
        .to_lowercase()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_string()
}

#[cfg(not(windows))]
fn dedupe_key(dir: &std::path::Path) -> String {
    dir.to_string_lossy().trim_end_matches('/').to_string()
}

/// Strip wrapping double quotes. A registry `Path` is sometimes stored quoted
/// whole, and the quotes are not part of the first or last directory name.
pub fn strip_wrapping_quotes(raw: &str) -> &str {
    let trimmed = raw.trim();
    trimmed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(trimmed)
}

/// Remove CSI and OSC escape sequences from one line of shell output.
///
/// A themed prompt colours everything it prints, and a coloured `PATH` is not a
/// `PATH`. Only the two forms a prompt actually emits are handled.
pub fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: parameters and intermediates, then one final byte.
            Some('[') => {
                for next in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&next) {
                        break;
                    }
                }
            }
            // OSC: runs to BEL, or to ESC followed by a backslash.
            Some(']') => {
                while let Some(next) = chars.next() {
                    if next == '\u{7}' {
                        break;
                    }
                    if next == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// The `PATH` the shell printed between the markers, or `None`.
///
/// The last start marker wins: an rc with `set -v` echoes the script first, so
/// the marker appears twice. A candidate must also hold `/` or `\`, which
/// rejects that echo — both, because a shell and a profile disagree on which.
pub fn extract_between_sentinels(stdout: &str) -> Option<String> {
    let lines: Vec<String> = stdout.lines().map(strip_ansi).collect();
    let start = lines.iter().rposition(|l| l.contains(SENTINEL_START))?;
    let end = lines
        .iter()
        .skip(start + 1)
        .position(|l| l.contains(SENTINEL_END))
        .map(|offset| start + 1 + offset)
        .unwrap_or(lines.len());
    lines[start + 1..end]
        .iter()
        .map(|line| line.trim())
        .find(|line| !line.is_empty() && (line.contains('/') || line.contains('\\')))
        .map(|line| line.to_string())
}

/// Login shells to try, best first, deduplicated.
///
/// `$SHELL`, then the account record, then a hard fallback. Anything not
/// starting `/` drops: resolving whatever `sh` names would defeat the point of
/// asking. The leading slash is the test, not host-dependent `is_absolute`.
pub fn login_shell_candidates(env_shell: Option<&str>, account_shell: Option<&str>) -> Vec<PathBuf> {
    let fallback = if cfg!(target_os = "macos") {
        "/bin/zsh"
    } else {
        "/bin/bash"
    };
    let mut out: Vec<PathBuf> = Vec::new();
    for raw in [env_shell, account_shell, Some(fallback)].into_iter().flatten() {
        let trimmed = raw.trim();
        let path = PathBuf::from(trimmed);
        if !trimmed.starts_with('/') || out.contains(&path) {
            continue;
        }
        out.push(path);
    }
    out
}

/// The script the login shell runs. One `printenv` between two markers.
///
/// `|| true` so an rc with `set -e` cannot abort before the end marker prints.
#[cfg(unix)]
fn probe_script() -> String {
    format!(
        "printf '%s\\n' '{SENTINEL_START}'\nprintenv PATH || true\nprintf '%s\\n' '{SENTINEL_END}'\n"
    )
}

/// Ask the user's login shell, then `launchctl`. First answer wins.
///
/// Returns the raw discovered `PATH` and the name of whatever answered.
#[cfg(unix)]
fn discover(deep: bool) -> (Option<OsString>, Option<String>) {
    // Nothing costs extra here: `-i` already sources every rc file.
    let _ = deep;
    let env_shell = std::env::var("SHELL").ok();
    let account = account_shell();
    for shell in login_shell_candidates(env_shell.as_deref(), account.as_deref()) {
        if let Some(path) = path_from_login_shell(&shell) {
            let name = shell
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| shell.to_string_lossy().to_string());
            return (Some(path), Some(name));
        }
    }
    #[cfg(target_os = "macos")]
    if let Some(path) = path_from_launchctl() {
        return (Some(path), Some("launchctl".to_string()));
    }
    (None, None)
}

/// The shell on the user's account record, through `getpwuid_r`.
///
/// Not `/etc/passwd` and not `dscl`: the first is wrong under a directory
/// service, the second is a process spawn for something libc already answers.
#[cfg(unix)]
fn account_shell() -> Option<String> {
    let mut record: libc::passwd = unsafe { std::mem::zeroed() };
    let mut buf = vec![0 as libc::c_char; 4096];
    let mut found: *mut libc::passwd = std::ptr::null_mut();
    let rc = unsafe {
        libc::getpwuid_r(
            libc::getuid(),
            &mut record,
            buf.as_mut_ptr(),
            buf.len(),
            &mut found,
        )
    };
    if rc != 0 || found.is_null() || record.pw_shell.is_null() {
        return None;
    }
    unsafe { std::ffi::CStr::from_ptr(record.pw_shell) }
        .to_str()
        .ok()
        .map(|shell| shell.to_string())
}

/// `PATH` as one interactive login shell computes it.
///
/// `-i` is load-bearing: `nvm`, `asdf` and `mise` install themselves into
/// `.zshrc` and `.bashrc`, which a non-interactive `-lc` never sources. The
/// environment is passed through untouched, so nothing we hold biases it.
#[cfg(unix)]
fn path_from_login_shell(shell: &std::path::Path) -> Option<OsString> {
    let mut cmd = crate::agents::probe::bare_command(shell);
    cmd.arg("-ilc").arg(probe_script());
    let out = crate::agents::probe::run_isolated(cmd, SHELL_TIMEOUT).ok()?;
    extract_between_sentinels(&out.stdout).map(OsString::from)
}

/// The fallback, and usually empty: `launchctl` only answers if something has
/// called `launchctl setenv`. It is a last resort, not the mechanism.
#[cfg(target_os = "macos")]
fn path_from_launchctl() -> Option<OsString> {
    let mut cmd = crate::agents::probe::bare_command("/bin/launchctl");
    cmd.arg("getenv").arg("PATH");
    let out = crate::agents::probe::run_isolated(cmd, LAUNCHCTL_TIMEOUT).ok()?;
    out.stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && line.contains('/'))
        .map(OsString::from)
}

/// Long enough for a cold `nvm` sourcing itself, short enough that a wedged rc
/// does not hold the deferred-init thread for the session.
#[cfg(unix)]
const SHELL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

#[cfg(target_os = "macos")]
const LAUNCHCTL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Read `Path` out of the registry, user then machine.
///
/// No shell and no PowerShell profile: a profile can be slow, can prompt and
/// can print, and the registry already carries every `PATH` edit an installer
/// makes. Merge order matches how Windows itself composes the variable.
#[cfg(windows)]
fn discover(deep: bool) -> (Option<OsString>, Option<String>) {
    use windows::core::w;
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    let user = registry_path(HKEY_CURRENT_USER, w!("Environment"));
    let machine = registry_path(
        HKEY_LOCAL_MACHINE,
        w!("SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment"),
    );
    // Below the registry, so a profile can add a directory but never reorder one.
    let profile = if deep { path_from_powershell_profile() } else { None };
    if user.is_none() && machine.is_none() && profile.is_none() {
        return (None, None);
    }
    let registry = merge_path_entries(&user.unwrap_or_default(), &machine.unwrap_or_default());
    let source = match &profile {
        Some(_) => "registry+profile",
        None => "registry",
    };
    let merged = merge_path_entries(&registry, &profile.unwrap_or_default());
    (Some(merged), Some(source.to_string()))
}

/// `PATH` as the user's PowerShell profile computes it.
///
/// The profile is loaded on purpose — that is the whole point, and it is what
/// `hydrate` refuses to pay for. `-NonInteractive` turns a prompting profile
/// into an error rather than a hang, and stdin is closed underneath it too.
#[cfg(windows)]
fn path_from_powershell_profile() -> Option<OsString> {
    let script = format!(
        "Write-Output '{SENTINEL_START}'; Write-Output $env:PATH; Write-Output '{SENTINEL_END}'"
    );
    for shell in ["pwsh.exe", "powershell.exe"] {
        let mut cmd = crate::agents::probe::bare_command(shell);
        cmd.arg("-NonInteractive").arg("-Command").arg(&script);
        let Ok(out) = crate::agents::probe::run_isolated(cmd, PROFILE_TIMEOUT) else {
            continue;
        };
        if let Some(path) = extract_between_sentinels(&out.stdout) {
            return Some(OsString::from(path));
        }
    }
    None
}

/// t3code's own figure. A profile that sources a version manager is doing real
/// work, and this only ever runs once, after an Agent was already not found.
#[cfg(windows)]
const PROFILE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// One `Path` value, read raw and expanded here.
///
/// `RRF_NOEXPAND` then `ExpandEnvironmentStringsW` rather than letting
/// `RegGetValueW` expand: the flag combination that restricts the type to
/// `REG_EXPAND_SZ` is only legal alongside `RRF_NOEXPAND`.
#[cfg(windows)]
fn registry_path(root: windows::Win32::System::Registry::HKEY, subkey: windows::core::PCWSTR) -> Option<OsString> {
    use windows::core::w;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegGetValueW, RegOpenKeyExW, HKEY, KEY_READ, RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ,
        RRF_RT_REG_SZ,
    };

    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(root, subkey, Some(0), KEY_READ, &mut key).is_err() {
            return None;
        }
        let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
        // Sized first: a `Path` grows past any fixed buffer worth writing down.
        let mut size = 0u32;
        let sized = RegGetValueW(key, None, w!("Path"), flags, None, None, Some(&mut size));
        if sized.is_err() || size == 0 {
            let _ = RegCloseKey(key);
            return None;
        }
        let mut buf = vec![0u16; size as usize / 2 + 1];
        let mut got = (buf.len() * 2) as u32;
        let read = RegGetValueW(
            key,
            None,
            w!("Path"),
            flags,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut got),
        );
        let _ = RegCloseKey(key);
        if read.is_err() {
            return None;
        }
        // `got` counts bytes and includes the terminating NUL.
        let len = (got as usize / 2).saturating_sub(1);
        let raw = String::from_utf16_lossy(&buf[..len.min(buf.len())]);
        let expanded = expand_env(strip_wrapping_quotes(&raw));
        (!expanded.trim().is_empty()).then(|| OsString::from(expanded))
    }
}

/// Expand `%SystemRoot%`-style references. A `REG_EXPAND_SZ` `Path` is full of
/// them, and an unexpanded one names no directory that exists.
#[cfg(windows)]
fn expand_env(raw: &str) -> String {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::System::Environment::ExpandEnvironmentStringsW;

    let src = HSTRING::from(raw);
    unsafe {
        let needed = ExpandEnvironmentStringsW(PCWSTR(src.as_ptr()), None);
        if needed == 0 {
            return raw.to_string();
        }
        let mut buf = vec![0u16; needed as usize];
        let written = ExpandEnvironmentStringsW(PCWSTR(src.as_ptr()), Some(&mut buf));
        if written == 0 || written as usize > buf.len() {
            return raw.to_string();
        }
        String::from_utf16_lossy(&buf[..(written as usize).saturating_sub(1)])
    }
}

/// Neither Windows nor unix: nothing to ask, and the inherited `PATH` stands.
#[cfg(not(any(windows, unix)))]
fn discover(_deep: bool) -> (Option<OsString>, Option<String>) {
    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An interactive login shell prints a banner, a coloured prompt and
    /// `direnv` chatter around the value. All of it has to go.
    #[test]
    fn v0_11_the_path_is_read_from_between_the_markers_through_the_noise() {
        let noisy = format!(
            "Last login: Tue Sep  2 09:14:22\n\
             \u{1b}]0;nikhil@mac\u{7}oh-my-zsh: 3 plugins loaded\n\
             direnv: loading ~/.envrc\n\
             {SENTINEL_START}\n\
             \u{1b}[32m/opt/homebrew/bin:/usr/bin:/bin\u{1b}[0m\n\
             {SENTINEL_END}\n\
             \u{1b}[1m\u{1b}[0m"
        );
        assert_eq!(
            extract_between_sentinels(&noisy).as_deref(),
            Some("/opt/homebrew/bin:/usr/bin:/bin")
        );
    }

    /// A shell that printed nothing between the markers has told us nothing,
    /// which must read as `None` rather than as an empty `PATH`.
    #[test]
    fn v0_11_markers_with_nothing_between_them_yield_nothing() {
        let empty = format!("{SENTINEL_START}\n\n   \n{SENTINEL_END}");
        assert_eq!(extract_between_sentinels(&empty), None);
        assert_eq!(extract_between_sentinels("no markers at all"), None);
    }

    /// A shell killed at the deadline after printing the value has still told
    /// us the answer, and discarding it would waste the five seconds spent.
    #[test]
    fn v0_11_a_missing_end_marker_still_yields_the_value() {
        let truncated = format!("{SENTINEL_START}\n/usr/bin:/bin\n");
        assert_eq!(
            extract_between_sentinels(&truncated).as_deref(),
            Some("/usr/bin:/bin")
        );
    }

    /// `set -v` in an rc echoes the script before running it, so the marker
    /// appears twice and the echoed `printenv` line sits inside the first pair.
    #[test]
    fn v0_11_an_echoing_shell_does_not_yield_its_own_script() {
        let echoed = format!(
            "printf '%s\\n' '{SENTINEL_START}'\n\
             {SENTINEL_START}\n\
             printenv PATH || true\n\
             /usr/local/bin:/usr/bin\n\
             printf '%s\\n' '{SENTINEL_END}'\n\
             {SENTINEL_END}"
        );
        assert_eq!(
            extract_between_sentinels(&echoed).as_deref(),
            Some("/usr/local/bin:/usr/bin")
        );
    }

    /// The PowerShell profile answers in `\`, the unix shell in `/`. Both are
    /// paths; neither host's separator may be assumed.
    #[test]
    fn v0_11_a_windows_path_between_the_markers_is_read_too() {
        let windows = format!(
            "{SENTINEL_START}\nC:\\Users\\me\\.bun\\bin;C:\\Windows\\System32\n{SENTINEL_END}"
        );
        assert_eq!(
            extract_between_sentinels(&windows).as_deref(),
            Some(r"C:\Users\me\.bun\bin;C:\Windows\System32")
        );
        // Still rejects a line that names no path at all.
        let noise = format!("{SENTINEL_START}\nsome words\n{SENTINEL_END}");
        assert_eq!(extract_between_sentinels(&noise), None);
    }

    /// The deep pass is the one allowed to cost a process, so it must actually
    /// reach further — never fewer folders than the cheap one found.
    #[test]
    fn v0_11_deep_hydration_is_a_superset_of_the_cheap_one() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        invalidate();
        hydrate();
        let cheap = hydrated_path();
        let cheap_entries = report().map(|r| r.entries).unwrap_or(0);

        hydrate_deep();
        let deep = report().expect("deep hydration always leaves a report");
        if cheap.is_some() {
            assert!(deep.entries >= cheap_entries, "deep lost folders");
            // The label says which sources answered, and Settings reads it.
            let source = deep.source.unwrap_or_default();
            assert!(source.contains("registry") || !cfg!(windows), "{source}");
        }
        invalidate();
    }

    /// Discovered entries lead, inherited ones follow, and nothing appears
    /// twice. Order is what makes the resolved binary the one a terminal runs.
    #[test]
    fn v0_11_a_merged_path_prefers_the_discovered_order_and_dedupes() {
        let sep = if cfg!(windows) { ';' } else { ':' };
        let (a, b, c) = if cfg!(windows) {
            (r"C:\bun\bin", r"C:\Windows\System32", r"C:\extra")
        } else {
            ("/opt/homebrew/bin", "/usr/bin", "/sbin")
        };
        let discovered = OsString::from(format!("{a}{sep}{b}"));
        let inherited = OsString::from(format!("{b}{sep}{c}"));
        let merged = merge_path_entries(&discovered, &inherited);
        let dirs: Vec<String> = std::env::split_paths(&merged)
            .map(|d| d.to_string_lossy().to_string())
            .collect();
        assert_eq!(dirs, vec![a.to_string(), b.to_string(), c.to_string()]);
    }

    /// Empty inputs are ordinary: a `launchctl` that answered nothing, or a
    /// process started with no `PATH` at all.
    #[test]
    fn v0_11_merging_survives_empty_inputs_and_empty_segments() {
        let sep = if cfg!(windows) { ';' } else { ':' };
        let dir = if cfg!(windows) { r"C:\bin" } else { "/bin" };
        let ragged = OsString::from(format!("{sep}{sep}{dir}{sep}"));
        assert_eq!(
            merge_path_entries(&ragged, OsStr::new("")).to_string_lossy(),
            dir
        );
        assert!(merge_path_entries(OsStr::new(""), OsStr::new("")).is_empty());
    }

    /// Windows lists the same directory in two casings and with a trailing
    /// separator. Three spellings, one directory, one entry.
    #[cfg(windows)]
    #[test]
    fn v0_11_windows_path_entries_dedupe_case_insensitively() {
        let discovered = OsString::from(r"C:\Windows\System32;C:\WINDOWS\system32\");
        let inherited = OsString::from("c:/windows/system32");
        let merged = merge_path_entries(&discovered, &inherited);
        assert_eq!(std::env::split_paths(&merged).count(), 1);
    }

    /// `$SHELL` leads, the account record follows, and the hard fallback is
    /// always last so there is always something to try.
    #[test]
    fn v0_11_login_shells_are_tried_in_order_and_never_twice() {
        let both = login_shell_candidates(Some("/bin/zsh"), Some("/usr/local/bin/fish"));
        assert_eq!(both[0], PathBuf::from("/bin/zsh"));
        assert_eq!(both[1], PathBuf::from("/usr/local/bin/fish"));
        assert_eq!(both.len(), 3, "the fallback is always appended");

        // The common case: `$SHELL` is already the account shell.
        let same = login_shell_candidates(Some("/bin/zsh"), Some("/bin/zsh"));
        assert_eq!(same.len(), if cfg!(target_os = "macos") { 1 } else { 2 });

        // Nothing known still leaves the fallback.
        assert_eq!(login_shell_candidates(None, None).len(), 1);
    }

    /// A relative `$SHELL` is not a login shell, and resolving `sh` against a
    /// `PATH` we do not trust is the thing this module exists to avoid.
    #[test]
    fn v0_11_a_relative_shell_is_not_a_candidate() {
        let out = login_shell_candidates(Some("sh"), Some("  "));
        assert_eq!(out.len(), 1);
        assert!(out[0].to_string_lossy().starts_with('/'));
    }

    /// A quoted `Path` value is ordinary on Windows and the quotes are not part
    /// of the directory name.
    #[test]
    fn v0_11_wrapping_quotes_come_off_a_registry_value() {
        assert_eq!(
            strip_wrapping_quotes("\"C:\\Program Files\\bin\""),
            r"C:\Program Files\bin"
        );
        assert_eq!(strip_wrapping_quotes("  C:\\bin  "), r"C:\bin");
        // A lone quote is not a wrapper and must not be eaten.
        assert_eq!(strip_wrapping_quotes("\"C:\\bin"), "\"C:\\bin");
    }

    /// Escape sequences a themed prompt emits, and nothing else touched.
    #[test]
    fn v0_11_ansi_sequences_are_stripped_and_plain_text_is_not() {
        assert_eq!(strip_ansi("\u{1b}[1;32mgreen\u{1b}[0m"), "green");
        assert_eq!(strip_ansi("\u{1b}]0;title\u{7}after"), "after");
        assert_eq!(strip_ansi("\u{1b}]0;title\u{1b}\\after"), "after");
        assert_eq!(strip_ansi("/usr/bin:/bin"), "/usr/bin:/bin");
    }

    /// The cache is the contract: nothing before `hydrate`, something after,
    /// and nothing again after `invalidate`.
    #[test]
    fn v0_11_hydration_caches_and_invalidation_clears_it() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        invalidate();
        assert!(report().is_none());
        hydrate();
        let after = report().expect("hydrate always leaves a report");
        // Windows always answers through the registry; a unix CI box may have no
        // login shell at all, so only the report itself is guaranteed.
        if after.source.is_some() {
            assert!(after.entries > 0);
            assert!(hydrated_path().is_some());
        }
        invalidate();
        assert!(report().is_none());
        assert!(hydrated_path().is_none());
    }

    /// The whole point, on this platform: the hydrated `PATH` holds everything
    /// the inherited one did. Losing an entry would break `PATH` executables.
    #[test]
    fn v0_11_a_hydrated_path_never_loses_an_inherited_entry() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        invalidate();
        hydrate();
        let Some(hydrated) = hydrated_path() else {
            return;
        };
        let keys: Vec<String> = std::env::split_paths(&hydrated)
            .map(|d| dedupe_key(&d))
            .collect();
        let inherited = std::env::var_os("PATH").unwrap_or_default();
        for dir in std::env::split_paths(&inherited) {
            if dir.as_os_str().is_empty() {
                continue;
            }
            assert!(keys.contains(&dedupe_key(&dir)), "lost {}", dir.display());
        }
        invalidate();
    }
}
