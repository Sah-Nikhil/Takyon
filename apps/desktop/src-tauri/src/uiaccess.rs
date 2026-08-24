//! The main app's half of the UIAccess arrangement.
//!
//! **The problem.** Windows gives every process an integrity level. Takyon runs at
//! Medium; anything started "as Administrator" runs at High. UIPI forbids a
//! lower-integrity process from taking foreground from a higher-integrity window,
//! and it refuses *silently*. So with an elevated terminal focused, the Palette
//! either does not appear or appears behind it and never receives a keystroke.
//!
//! **The only sanctioned escape** is a manifest carrying `uiAccess="true"`, which
//! Windows honours only if the binary is Authenticode-signed **and** sits in a
//! directory a standard user cannot write to (`%ProgramFiles%`, `System32`). This
//! is the same mechanism screen readers use, and it is why code signing is a v0.1
//! requirement rather than a shipping-time one, and why a portable build of this
//! product is impossible.
//!
//! **Why a separate executable.** A `uiAccess` process pays real costs — drag and
//! drop from Explorer breaks on the integrity mismatch, for one — and running the
//! entire WebView2 surface at a raised privilege to solve a foreground problem is
//! a bad trade. The helper does exactly one thing and is the only signed-critical
//! binary in the product.
//!
//! **Failure is expected and non-fatal.** Unsigned builds, dev builds, and any
//! install outside a trusted location simply do not get the helper. The Palette
//! still works everywhere except over an elevated window, which is precisely the
//! limitation the plan accepts.

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager, WebviewWindow};

/// The helper's rendezvous point. Named after the ADR-0011 slug like everything
/// else the OS keys off, so a rename of the product does not orphan it.
pub const PIPE_NAME: &str = r"\\.\pipe\com.v3sper.launcher.uiaccess";

/// Point this at a helper binary to use one that is not beside the executable.
/// Exists so a self-signed helper can be tested during development without
/// installing the app.
pub const HELPER_ENV: &str = "TAKYON_UIACCESS_HELPER";

const HELPER_EXE: &str = "takyon-uiaccess-helper.exe";

static HELPER_RUNNING: AtomicBool = AtomicBool::new(false);

/// Where the helper is, if it is anywhere.
///
/// The helper is deliberately **not** a Tauri bundle resource. It is only useful
/// when signed and installed somewhere a standard user cannot write, which is a
/// step outside the normal build — so `scripts/install-uiaccess-helper.ps1` puts
/// it in place, and a build that skipped that step simply has no helper rather
/// than shipping one that Windows will refuse to start.
fn helper_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    if let Some(explicit) = std::env::var_os(HELPER_ENV) {
        let p = std::path::PathBuf::from(explicit);
        return p.exists().then_some(p);
    }

    let candidates = [
        app.path().resource_dir().ok().map(|d| d.join(HELPER_EXE)),
        std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|d| d.join(HELPER_EXE))),
    ];
    candidates.into_iter().flatten().find(|p| p.exists())
}

/// Start the helper, if there is one to start.
///
/// Called from deferred init, never from the hot path. A failure here is logged
/// once and then forgotten: it means the elevated-window case will not work, and
/// nothing else.
pub fn start(app: &AppHandle) {
    let Some(path) = helper_path(app) else {
        return;
    };

    match std::process::Command::new(&path).spawn() {
        Ok(_) => {
            HELPER_RUNNING.store(true, Ordering::Relaxed);
        }
        Err(e) => {
            // ERROR_ELEVATION_REQUIRED (740) is the interesting one: it means the
            // manifest asked for uiAccess and Windows refused, which is always
            // either an unsigned binary or one outside a trusted location. Saying
            // so beats "spawn failed", because those are the only two fixes.
            eprintln!(
                "[takyon] the UIAccess helper at {} would not start: {e}\n\
                 The Palette will not appear over elevated windows. This means the \
                 helper is unsigned, or is installed somewhere a standard user can \
                 write to. See docs/plans/uiaccess-signing.md.",
                path.display()
            );
        }
    }
}

/// Ask the helper to bring `win` to the foreground.
///
/// Fire and forget, on its own thread. The show path must not block on a pipe: if
/// the helper has died or is wedged, the cost of finding that out synchronously
/// would be paid on the one code path the entire product is optimised around.
/// Foreground arriving a millisecond late is invisible; a show that stalls is not.
#[cfg(windows)]
pub fn request_foreground(win: &WebviewWindow) {
    if !HELPER_RUNNING.load(Ordering::Relaxed) {
        return;
    }
    let Ok(hwnd) = win.hwnd() else { return };
    let raw = hwnd.0 as usize as u64;

    std::thread::spawn(move || {
        if let Err(e) = send(raw) {
            eprintln!("[takyon] the UIAccess helper did not answer: {e}");
            // One failure is enough to stop trying. A helper that has exited will
            // not come back, and retrying on every show would spawn a thread per
            // keypress for the rest of the session.
            HELPER_RUNNING.store(false, Ordering::Relaxed);
        }
    });
}

#[cfg(not(windows))]
pub fn request_foreground(_win: &WebviewWindow) {}

#[cfg(windows)]
fn send(hwnd: u64) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, WriteFile, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_WRITE, FILE_SHARE_MODE,
        OPEN_EXISTING,
    };

    let wide: Vec<u16> = std::ffi::OsStr::new(PIPE_NAME)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let handle = CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_GENERIC_WRITE.0,
            FILE_SHARE_MODE(0),
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
        .map_err(std::io::Error::other)?;

        let bytes = hwnd.to_le_bytes();
        let mut written = 0u32;
        let result = WriteFile(handle, Some(&bytes), Some(&mut written), None);
        let _ = CloseHandle(handle);

        result.map_err(std::io::Error::other)?;
        if written as usize != bytes.len() {
            return Err(std::io::Error::other("short write to the UIAccess helper"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ADR-0011 again: the pipe is something the OS namespace keys off, so it uses
    /// the slug. A pipe named after the display name would break on a rename and
    /// would collide with nothing helpfully.
    #[test]
    fn v0_1_the_pipe_is_named_after_the_slug() {
        assert!(PIPE_NAME.contains(crate::identity::IDENTITY));
        assert!(
            !PIPE_NAME
                .to_lowercase()
                .contains(&crate::identity::DISPLAY_NAME.to_lowercase())
        );
    }

    /// Both sides have to agree on the name, and they are separate crates with no
    /// shared types. This is the cheapest way to keep them from drifting apart.
    #[test]
    fn v0_1_the_helper_agrees_on_the_pipe_name() {
        let helper = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("uiaccess/src/main.rs"),
        )
        .expect("uiaccess/src/main.rs");
        assert!(
            helper.contains(PIPE_NAME),
            "the helper and the client disagree about the pipe name"
        );
    }
}
