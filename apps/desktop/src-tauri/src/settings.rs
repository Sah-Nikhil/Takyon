//! The Settings window.
//!
//! Placeholder content until v0.6, but a genuine second window with its own label
//! and its own capability file. Two reasons it is not a disabled menu item:
//!
//! - The Palette must never hold a permission it has no use for. Autostart lives
//!   in Settings, so `autostart:default` belongs to Settings' capability and
//!   nowhere else. Discovering that split at v0.6, when Settings is already a
//!   large piece of work, is worse than paying for it now.
//! - It is created **lazily**. A second WebView2 instance at startup would cost
//!   both the login budget and a large share of the 150 MB idle ceiling, for a
//!   window most sessions never open.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::identity::DISPLAY_NAME;

pub const LABEL: &str = "settings";

/// Open Settings, or focus it if it is already open.
///
/// Unlike the Palette, this window is destroyed when closed. Nothing about it is
/// latency-sensitive, so keeping a second webview warm would be paying ADR-0003's
/// price without ADR-0003's reason.
pub fn open(app: &AppHandle) {
    if let Some(existing) = app.get_webview_window(LABEL) {
        let _ = existing.show();
        let _ = existing.unminimize();
        let _ = existing.set_focus();
        return;
    }

    let built = WebviewWindowBuilder::new(
        app,
        LABEL,
        WebviewUrl::App("index.html?window=settings".into()),
    )
    .title(format!("{DISPLAY_NAME} Settings"))
    .inner_size(760.0, 560.0)
    .min_inner_size(560.0, 420.0)
    .resizable(true)
    .build();

    if let Err(e) = built {
        // Reported rather than swallowed: the tray item appearing to do nothing is
        // the exact failure this is most likely to produce.
        eprintln!("[takyon] could not open the Settings window: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The label is what `capabilities/settings.json` is scoped to. If they drift,
    /// the window opens with no permissions and the autostart switch fails at
    /// runtime with a permission error rather than at build time.
    #[test]
    fn v0_1_the_settings_capability_is_scoped_to_this_label() {
        let cap: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("capabilities/settings.json"),
            )
            .unwrap(),
        )
        .unwrap();

        assert!(cap["windows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str() == Some(LABEL)));
    }
}
