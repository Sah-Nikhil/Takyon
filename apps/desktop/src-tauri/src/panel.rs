//! The macOS half of the Palette window: float over everything, join every
//! Space, and dismiss on a click that lands anywhere else (ADR-0028).
//!
//! Two things Windows gets for free and macOS does not. `alwaysOnTop` in
//! `tauri.conf.json` is a window level, not a Space policy, so without
//! `canJoinAllSpaces` the Palette vanishes when the user switches desktop and
//! without `fullScreenAuxiliary` it cannot draw over a full-screen editor.
//!
//! Dismiss-on-click-away is rebuilt rather than ported: it hangs on Tauri's
//! `Focused(false)` on Windows, and this window is configured not to deactivate.
//! What the ADR's `becomesKeyOnlyIfNeeded` needs is in `docs/tbd/v0.12.md` §9.

use objc2_app_kit::{
    NSEvent, NSEventMask, NSFloatingWindowLevel, NSWindow, NSWindowCollectionBehavior,
    NSWindowStyleMask,
};
use tauri::AppHandle;

/// Apply the window flags. Main thread only, and once, at startup.
///
/// Safe to call before the window has ever been shown; none of these take effect
/// on a schedule or need the window visible.
pub fn configure(app: &AppHandle) {
    let Some(win) = crate::window::palette(app) else {
        eprintln!("[takyon] no Palette window to configure as a panel");
        return;
    };
    let ptr = match win.ns_window() {
        Ok(p) => p as *mut NSWindow,
        Err(e) => {
            eprintln!("[takyon] could not reach the NSWindow: {e}");
            return;
        }
    };
    if ptr.is_null() {
        eprintln!("[takyon] the NSWindow handle was null");
        return;
    }

    // SAFETY: Tauri owns the window and outlives this borrow, and `setup` runs on
    // the main thread, which every NSWindow setter below requires.
    let ns: &NSWindow = unsafe { &*ptr };

    ns.setLevel(NSFloatingWindowLevel);
    ns.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );

    // Without this the Palette disappears the moment the app it is floating over
    // takes focus back, which is every summon.
    ns.setHidesOnDeactivate(false);

    // Keeps the window from taking activation away from the app underneath. Only
    // fully honoured by an NSPanel — see the module doc's pointer.
    ns.setStyleMask(ns.styleMask() | NSWindowStyleMask::NonactivatingPanel);
}

/// Hide the Palette when a click lands in any other application.
///
/// A global monitor only sees events delivered elsewhere, so anything it
/// receives is outside our window by definition and no frame test is needed.
/// It observes and cannot consume, so the click still reaches its target.
pub fn watch_clicks(app: &AppHandle) {
    let handle = app.clone();
    let block = block2::RcBlock::new(move |_event: std::ptr::NonNull<NSEvent>| {
        if crate::window::should_hide_on_focus_loss(&handle) {
            crate::window::hide(&handle, "click outside");
        }
    });

    let monitor = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
        NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown,
        &block,
    );

    // Leaked deliberately: releasing the token unregisters the monitor, and this
    // one is wanted for the life of the process. `Retained` is neither Send nor
    // Sync, so a static cannot hold it and there is nothing to unregister from.
    match monitor {
        Some(token) => std::mem::forget(token),
        None => {
            eprintln!("[takyon] the global mouse monitor was refused; click-away will not dismiss")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_12_the_collection_behaviour_is_both_flags() {
        // Either alone is a different product: without CanJoinAllSpaces the
        // Palette is stranded on one desktop, without FullScreenAuxiliary it
        // cannot draw over a full-screen app.
        let want = NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary;
        assert!(want.contains(NSWindowCollectionBehavior::CanJoinAllSpaces));
        assert!(want.contains(NSWindowCollectionBehavior::FullScreenAuxiliary));
    }

    #[test]
    fn v0_12_the_monitor_watches_both_mouse_buttons() {
        let mask = NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown;
        assert!(mask.contains(NSEventMask::LeftMouseDown));
        assert!(mask.contains(NSEventMask::RightMouseDown));
    }
}
