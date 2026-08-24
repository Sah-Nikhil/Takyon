//! The first-run autostart prompt.
//!
//! ROADMAP v0.1 says autostart is "on by default via first-run prompt", and the
//! two halves of that phrase are in tension on purpose. A launcher that is not
//! running cannot answer its hotkey, so the useful default is on. But silently
//! registering something at login on someone's behalf is exactly the behaviour
//! that makes people distrust a tool. So: asked once, with on as the pre-selected
//! answer, and never asked again.
//!
//! The "asked" flag is a marker file rather than a row in `settings.db`, because
//! `settings.db` is v0.6 work and inventing a schema here would mean writing a
//! migration for it later. The **answer** is never stored at all — it lives in the
//! registry, where the OS owns it.

use std::path::PathBuf;
use tauri::AppHandle;

/// Presence means the question has been asked. Contents are irrelevant.
const MARKER: &str = "first-run-complete";

pub fn marker_path() -> Option<PathBuf> {
    crate::identity::data_dir().map(|d| d.join(MARKER))
}

pub fn already_asked() -> bool {
    marker_path().map(|p| p.exists()).unwrap_or(false)
}

// Only `maybe_prompt` calls this, and that is compiled out of debug builds — so in
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

/// Ask once, then act on the answer.
///
/// Runs well after the hotkey is live — a modal dialog during startup would sit
/// inside the login-to-responsive budget, and the first thing a new user would
/// meet is a launcher that cannot yet launch anything.
#[cfg(not(debug_assertions))]
pub fn maybe_prompt(app: &AppHandle) {
    use tauri_plugin_autostart::ManagerExt;
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

    if already_asked() || !should_ask() {
        return;
    }

    let wants = app
        .dialog()
        .message(format!(
            "{} answers its hotkey only while it is running.\n\nStart it automatically when you log in?",
            crate::identity::DISPLAY_NAME
        ))
        .title(crate::identity::DISPLAY_NAME)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Start at login".into(),
            "Not now".into(),
        ))
        .blocking_show();

    // The marker is written whatever the answer is. "Asked" and "said yes" are
    // different facts, and conflating them would re-ask everyone who declined.
    mark_asked();

    if wants {
        if let Err(e) = app.autolaunch().enable() {
            eprintln!("[takyon] could not enable autostart: {e}");
        }
    }
}


/// Is this a launch that should be allowed to ask about autostart at all?
///
/// `#[cfg(not(debug_assertions))]` is not sufficient, and this was learned the
/// expensive way: a **release** build run straight out of `target\release\` passes
/// that check, and `bun run bench` launches exactly such a binary. The bench then
/// injects Alt+Space thirty times, one of which activated the prompt's default
/// button, leaving a real `Run` entry pointing into the repo's build output. That
/// entry survives `cargo clean`, deleting the repo, and installing the actual
/// product.
///
/// So two further conditions:
///
/// 1. **Not while benchmarking.** `TAKYON_BENCH_LOG` being set means synthetic
///    input is about to arrive, and a modal dialog in front of synthetic input is
///    a machine answering a question on the user's behalf.
/// 2. **Not from a build output directory.** A binary in `target\debug\` or
///    `target\release\` is never an installed application, whatever profile it was
///    compiled with, and it has no business claiming a startup slot.
// Reached only from `maybe_prompt`, which is compiled out of debug builds.
#[cfg_attr(debug_assertions, allow(dead_code))]
fn should_ask() -> bool {
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
/// Pure and path-taking so it is testable without moving a binary around.
/// Compares path *components* rather than substrings: someone whose folder is
/// named `targeted` is not running a build output, and a substring match would
/// decide that they were.
// Reached only from `maybe_prompt`, which is compiled out of debug builds.
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
/// The prompt is harmless, but the `enable()` behind it is not: it would write a
/// `Run` key pointing at `target\debug\`, which launches a dev build every login
/// and survives uninstalling the real app. Gating the prompt rather than only the
/// write also means `bun run dev` does not consume the one-time question.
#[cfg(debug_assertions)]
pub fn maybe_prompt(_app: &AppHandle) {}

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
        assert!(!is_build_output(std::path::Path::new(r"C:\target\takyon.exe")));
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
