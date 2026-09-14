//! First-run autostart.
//!
//! **On by default since v0.6, no longer a question.** A launcher not running
//! cannot answer its hotkey. Through v0.5 a modal asked, since Settings had never
//! rendered (v0.6 found); General → "Start Takyon when I log in" now reads the
//! registry on mount and turns it off in one click.
//!
//! Guards unchanged: never from a debug build, while benchmarking, or from a
//! `target\` directory. Marker file records first run, so a later "off" is never
//! undone on next launch.

use std::path::PathBuf;
use tauri::AppHandle;

/// Presence means first run already happened. Contents are irrelevant.
const MARKER: &str = "first-run-complete";

pub fn marker_path() -> Option<PathBuf> {
    crate::identity::data_dir().map(|d| d.join(MARKER))
}

pub fn already_ran() -> bool {
    marker_path().map(|p| p.exists()).unwrap_or(false)
}

// Only `maybe_enable` calls this, and that is compiled out of debug builds — so in
// a dev build it is genuinely dead, on purpose.
#[cfg_attr(debug_assertions, allow(dead_code))]
fn mark_asked() {
    let Ok(dir) = crate::identity::ensure_data_dir() else {
        // Without the marker the prompt reappears next launch, which is annoying
        // but harmless — and strictly better than suppressing it by assuming.
        eprintln!("[takyon] could not create the data directory; first run will be asked again");
        return;
    };
    if let Err(e) = std::fs::write(dir.join(MARKER), b"") {
        eprintln!("[takyon] could not write the first-run marker: {e}");
    }
}

/// Register autostart on first run, once.
///
/// Runs well after the hotkey is live: it is a registry write, and nothing about
/// it belongs inside the login-to-responsive budget.
///
#[cfg(not(debug_assertions))]
pub fn maybe_enable(app: &AppHandle) {
    use tauri_plugin_autostart::ManagerExt;

    // Once only. The marker is what stops a later "off" in Settings being undone
    // on the next launch — "first run happened" and "autostart is on" are
    // different facts, and the registry owns the second one (ADR-0015).
    if already_ran() || !may_register() {
        return;
    }

    mark_asked();

    if let Err(e) = app.autolaunch().enable() {
        eprintln!("[takyon] could not enable autostart on first run: {e}");
    }
}

/// Is this a launch that may claim a startup slot at all?
///
/// Release cfg alone let a bench-run binary in `target\release\` leave a `Run` key
/// into build output. So never with `TAKYON_BENCH_LOG` set, never from `target\`.
// Reached only from `maybe_enable`, which is compiled out of debug builds.
#[cfg_attr(debug_assertions, allow(dead_code))]
fn may_register() -> bool {
    if std::env::var_os(crate::bench::LOG_ENV).is_some() {
        return false;
    }
    match std::env::current_exe() {
        Ok(exe) => !is_build_output(&exe),
        // If we cannot tell where we are, do not claim a startup slot.
        Err(_) => false,
    }
}

/// Does this path sit inside a Cargo build output directory?
///
/// Path-taking, so testable without moving a binary. Compares *components*, not
/// substrings: a folder named `targeted` is not build output.
// Reached only from `maybe_enable`, which is compiled out of debug builds.
#[cfg_attr(debug_assertions, allow(dead_code))]
fn is_build_output(exe: &std::path::Path) -> bool {
    let parts: Vec<String> = exe
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    parts
        .windows(2)
        .any(|w| w[0] == "target" && (w[1] == "debug" || w[1] == "release"))
}

/// Never in a dev build.
///
/// `enable()` would write a `Run` key into `target\debug\` that outlives the real
/// app. Gating the prompt too keeps `bun run dev` from spending the one question.
#[cfg(debug_assertions)]
pub fn maybe_enable(_app: &AppHandle) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// The guard that would have prevented `bun run bench` from registering
    /// autostart against the repo's own build output.
    #[test]
    fn v0_1_a_build_output_binary_never_claims_a_startup_slot() {
        assert!(is_build_output(std::path::Path::new(
            r"C:\repo\apps\desktop\src-tauri\target\release\takyon.exe"
        )));
        assert!(is_build_output(std::path::Path::new(
            r"C:\repo\apps\desktop\src-tauri\target\debug\takyon.exe"
        )));
    }

    /// A real install must still be able to ask. Matching on components rather
    /// than substrings is what keeps these three out of the net.
    #[test]
    fn v0_1_an_installed_binary_may_still_ask() {
        assert!(!is_build_output(std::path::Path::new(
            r"C:\Program Files\Takyon\takyon.exe"
        )));
        assert!(!is_build_output(std::path::Path::new(
            r"C:\target\takyon.exe"
        )));
        assert!(!is_build_output(std::path::Path::new(
            r"C:\Users\targeted\release notes\takyon.exe"
        )));
    }

    /// The marker belongs inside the ADR-0011 data directory, not beside the
    /// executable. A marker next to the binary would be lost on every update and
    /// re-ask the question forever.
    #[test]
    fn v0_1_the_marker_lives_in_the_data_directory() {
        if let (Some(marker), Some(dir)) = (marker_path(), crate::identity::data_dir()) {
            assert!(marker.starts_with(&dir));
            assert!(marker.ends_with(MARKER));
        }
    }
}
