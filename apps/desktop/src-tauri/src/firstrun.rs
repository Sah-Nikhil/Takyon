//! First-run autostart.
//!
//! **On by default since v0.6, no longer a question.** A launcher not running
//! cannot answer its hotkey. Through v0.5 a modal asked, since Settings had never
//! rendered (v0.6 found); General → "Start Takyon when I log in" now reads the
//! registry on mount and turns it off in one click.
//!
//! Guards unchanged: never from a debug build, while benchmarking, or from Cargo
//! build output. Marker file records first run, so a later "off" is never
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
/// into build output. So never with `TAKYON_BENCH_LOG` set, never from build output
/// wherever `CARGO_TARGET_DIR` put it. Only `maybe_enable` calls this (release only).
#[cfg_attr(debug_assertions, allow(dead_code))]
fn may_register() -> bool {
    if std::env::var_os(crate::bench::LOG_ENV).is_some() {
        return false;
    }
    // If we cannot tell where we are, do not claim a startup slot.
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    !is_build_output(&exe) && !built_into().is_some_and(|dir| is_under(&exe, &dir))
}

/// Profile dir this binary was compiled into, from build.rs. Canonical.
///
/// Catches a `CARGO_TARGET_DIR` of any name. `None` off the build machine,
/// where the dir does not exist, so nothing can be under it.
#[cfg_attr(debug_assertions, allow(dead_code))]
fn built_into() -> Option<PathBuf> {
    option_env!("TAKYON_BUILT_INTO")
        .filter(|p| !p.is_empty())
        .and_then(|p| std::fs::canonicalize(p).ok())
}

/// Is `path` strictly inside `dir`? Per component, case-insensitive like NTFS.
#[cfg_attr(debug_assertions, allow(dead_code))]
fn is_under(path: &std::path::Path, dir: &std::path::Path) -> bool {
    let (path, dir) = (lowered(path), lowered(dir));
    path.len() > dir.len() && path.starts_with(&dir)
}

#[cfg_attr(debug_assertions, allow(dead_code))]
fn lowered(path: &std::path::Path) -> Vec<String> {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect()
}

/// Does this path look like Cargo build output, wherever it was built?
///
/// `<..>target\[<triple>\]debug|release\`, any name ending `target` (`cargo-target`).
/// Per component, not substring: a folder named `targeted` is not build output.
// Reached only from `maybe_enable`, which is compiled out of debug builds.
#[cfg_attr(debug_assertions, allow(dead_code))]
fn is_build_output(exe: &std::path::Path) -> bool {
    let parts = lowered(exe);
    let profile = |p: Option<&String>| p.is_some_and(|p| p == "debug" || p == "release");
    parts.iter().enumerate().any(|(i, p)| {
        p.ends_with("target") && (profile(parts.get(i + 1)) || profile(parts.get(i + 2)))
    })
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

    /// `CARGO_TARGET_DIR` moves output out of `target\`. A shared dir named
    /// `cargo-target` (this machine, tesseract too) must still be caught.
    #[test]
    fn v0_11_a_relocated_target_dir_is_still_build_output() {
        assert!(is_build_output(std::path::Path::new(
            r"D:\cargo-target\release\takyon.exe"
        )));
        assert!(is_build_output(std::path::Path::new(
            r"D:\Cargo_Target\Debug\takyon.exe"
        )));
    }

    /// `--target <triple>` adds a level: `target\<triple>\release\`. Release CI
    /// builds macOS exactly so.
    #[test]
    fn v0_11_a_cross_compiled_binary_is_build_output() {
        assert!(is_build_output(std::path::Path::new(
            r"C:\repo\target\aarch64-apple-darwin\release\takyon"
        )));
    }

    /// `CARGO_TARGET_DIR` may be named anything. The dir this build wrote its
    /// binary into is build output whatever its name, in any case.
    #[test]
    fn v0_11_the_dir_cargo_built_into_is_build_output_whatever_its_name() {
        let built = std::path::Path::new(r"D:\builds\takyon\release");
        assert!(is_under(
            std::path::Path::new(r"D:\builds\takyon\release\takyon.exe"),
            built
        ));
        assert!(is_under(
            std::path::Path::new(r"d:\BUILDS\Takyon\Release\bundle\nsis\takyon.exe"),
            built
        ));
        assert!(!is_under(
            std::path::Path::new(r"D:\Takyon\takyon.exe"),
            built
        ));
        assert!(!is_under(
            std::path::Path::new(r"D:\builds\takyon\release-notes\takyon.exe"),
            built
        ));
    }

    /// build.rs's guess at that dir, checked against where cargo really put
    /// this test binary (`<profile>\deps\`).
    #[test]
    fn v0_11_the_compile_time_build_dir_holds_this_test_binary() {
        let built = built_into().expect("build.rs sets TAKYON_BUILT_INTO");
        let exe = std::fs::canonicalize(std::env::current_exe().unwrap()).unwrap();
        assert!(
            is_under(&exe, &built),
            "{} not under {}",
            exe.display(),
            built.display()
        );
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
