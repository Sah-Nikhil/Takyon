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

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::identity::DISPLAY_NAME;
use crate::prefs::{self, Prefs};

pub const LABEL: &str = "settings";

/// Every preference a window reads on mount, in one response.
///
/// One `invoke` rather than one per control: the Palette mounts this too, for the
/// motion attribute, and it mounts on the startup path. Autostart is deliberately
/// **not** here — the OS owns that answer and it is re-read every mount (ADR-0015).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub reduce_motion: bool,
    pub calc_policy: String,
    pub recents: bool,
    pub tray: bool,
    pub placement: String,
    pub clip_retention: String,
    pub clip_bang: bool,
    pub theme: String,
    pub ui_size: String,
}

/// Appearance, as stored. Anything unrecognised follows the system.
pub fn theme(prefs: &Prefs) -> String {
    match prefs.get(prefs::THEME).as_deref() {
        Some("light") => "light".into(),
        Some("dark") => "dark".into(),
        _ => "system".into(),
    }
}

/// Interface size, as stored. Anything unrecognised is the default size.
pub fn ui_size(prefs: &Prefs) -> String {
    match prefs.get(prefs::UI_SIZE).as_deref() {
        Some("small") => "small".into(),
        Some("large") => "large".into(),
        _ => "default".into(),
    }
}

/// Where the Palette opens. Two pinned choices, not a monitor list: a saved
/// monitor index is wrong the moment a display is unplugged.
pub fn placement(prefs: &Prefs) -> String {
    match prefs.get(prefs::PLACEMENT).as_deref() {
        Some("primary") => "primary".into(),
        _ => "cursor".into(),
    }
}

impl Snapshot {
    /// Read the stored preferences, falling back to the documented defaults.
    pub fn read(prefs: &Prefs) -> Self {
        Snapshot {
            reduce_motion: prefs::flag(prefs, prefs::UI_REDUCE_MOTION, false),
            calc_policy: crate::sources::calc::Policy::parse(
                prefs.get(prefs::CALC_POLICY).as_deref().unwrap_or_default(),
            )
            .as_str()
            .to_string(),
            recents: prefs::flag(prefs, prefs::RECENTS, true),
            tray: prefs::flag(prefs, prefs::TRAY, true),
            placement: placement(prefs),
            clip_retention: prefs
                .get(prefs::CLIPS_RETENTION)
                .map_or_else(|| crate::clips::Retention::default().as_str().to_string(), |v| {
                    crate::clips::Retention::parse(&v).as_str().to_string()
                }),
            clip_bang: prefs::flag(prefs, prefs::CLIPS_BANG, true),
            theme: theme(prefs),
            ui_size: ui_size(prefs),
        }
    }
}

/// Every stored preference, read once on mount.
#[tauri::command]
pub fn settings_snapshot(prefs: tauri::State<'_, Arc<Prefs>>) -> Snapshot {
    Snapshot::read(&prefs)
}

/// Turn our own animations off, or back on.
///
/// No event is emitted to the other window: the Palette re-reads the snapshot on
/// every show, which is the guaranteed sync point and was already how the
/// `localStorage` version stayed honest.
#[tauri::command]
pub fn set_reduce_motion(on: bool, prefs: tauri::State<'_, Arc<Prefs>>) {
    if let Err(e) = prefs.set(prefs::UI_REDUCE_MOTION, if on { "1" } else { "0" }) {
        eprintln!("[takyon] the motion setting could not be saved: {e}");
    }
}

/// Whether the Recents Source contributes Entries.
///
/// Stored **and** pushed: the pipeline reads it on the keystroke path and must
/// not go to SQLite for it.
#[tauri::command]
pub fn set_recents(
    on: bool,
    prefs: tauri::State<'_, Arc<Prefs>>,
    pipeline: tauri::State<'_, Arc<crate::query::Pipeline>>,
) {
    if let Err(e) = prefs.set(prefs::RECENTS, if on { "1" } else { "0" }) {
        eprintln!("[takyon] the recents setting could not be saved: {e}");
    }
    pipeline.set_recents_enabled(on);
}

/// Show or hide the tray icon. Refused while the hotkey is unregistered.
#[tauri::command]
pub fn set_tray(
    app: AppHandle,
    on: bool,
    prefs: tauri::State<'_, Arc<Prefs>>,
) -> Result<(), String> {
    crate::tray::set_visible(&app, on)?;
    prefs
        .set(prefs::TRAY, if on { "1" } else { "0" })
        .map_err(|e| e.to_string())
}

/// Which monitor the Palette opens on. Read on every show, never cached.
#[tauri::command]
pub fn set_placement(value: String, prefs: tauri::State<'_, Arc<Prefs>>) -> Result<(), String> {
    let value = match value.as_str() {
        "primary" => "primary",
        "cursor" => "cursor",
        other => return Err(format!("{other} is not a placement")),
    };
    prefs
        .set(prefs::PLACEMENT, value)
        .map_err(|e| e.to_string())?;
    crate::window::cache_layout_prefs(&prefs);
    Ok(())
}

/// Follow the system, or override it (v0.6 Appearance).
#[tauri::command]
pub fn set_theme(value: String, prefs: tauri::State<'_, Arc<Prefs>>) -> Result<(), String> {
    let value = match value.as_str() {
        "system" | "light" | "dark" => value,
        other => return Err(format!("{other} is not an appearance")),
    };
    prefs.set(prefs::THEME, &value).map_err(|e| e.to_string())
}

/// Interface size. Resizes the Palette immediately, not on the next summon.
#[tauri::command]
pub fn set_ui_size(
    app: AppHandle,
    value: String,
    prefs: tauri::State<'_, Arc<Prefs>>,
) -> Result<(), String> {
    let value = match value.as_str() {
        "small" | "default" | "large" => value,
        other => return Err(format!("{other} is not an interface size")),
    };
    prefs.set(prefs::UI_SIZE, &value).map_err(|e| e.to_string())?;
    // The window is sized in Rust and zoomed in CSS. Both have to move together
    // or the Palette is exactly the zoom too short.
    crate::window::cache_layout_prefs(&prefs);
    crate::window::rescale(&app);
    Ok(())
}

/// Carry v0.1's `localStorage` preferences into `settings.db` (task 8b).
///
/// Runs on every mount because only a window can read `localStorage`, so it has
/// to be idempotent: a key already stored wins, or a stale legacy value would
/// undo a choice made after the first migration.
#[tauri::command]
pub fn migrate_local_prefs(
    reduce_motion: Option<bool>,
    calc_policy: Option<String>,
    prefs: tauri::State<'_, Arc<Prefs>>,
) -> Snapshot {
    if let Some(on) = reduce_motion {
        let _ = prefs.set_if_absent(prefs::UI_REDUCE_MOTION, if on { "1" } else { "0" });
    }
    if let Some(policy) = calc_policy {
        let _ = prefs.set_if_absent(prefs::CALC_POLICY, &policy);
    }
    Snapshot::read(&prefs)
}

/// Open Settings, or focus it if it is already open.
///
/// Unlike the Palette, this window is destroyed when closed. Nothing about it is
/// latency-sensitive, so keeping a second webview warm would be paying ADR-0003's
/// price without ADR-0003's reason.
pub fn open(app: &AppHandle) {
    // Never on the calling thread. Both callers are on the main thread and
    // `build()` blocks there waiting for the event loop, which deadlocks: the
    // frame appears and its webview never loads. Reasoning in CLAUDE.md gotchas.
    let app = app.clone();
    std::thread::spawn(move || build(&app));
}

fn build(app: &AppHandle) {
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
    // Wide enough for the sidebar plus a content column that does not wrap its
    // descriptions to three lines. The minimum still shows both.
    .inner_size(880.0, 620.0)
    .min_inner_size(680.0, 480.0)
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

    /// The snapshot is read once per window mount, so its field names are a
    /// contract with `packages/shared/src/ipc.ts`. A drift here fails at runtime
    /// in a window nobody opens twice, which is why it is asserted rather than
    /// reviewed.
    #[test]
    fn v0_6_the_snapshot_serialises_to_the_declared_contract() {
        let prefs = crate::prefs::Prefs::open(None).unwrap();
        let v: serde_json::Value = serde_json::to_value(Snapshot::read(&prefs)).unwrap();

        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "calcPolicy",
                "clipBang",
                "clipRetention",
                "placement",
                "recents",
                "reduceMotion",
                "theme",
                "tray",
                "uiSize"
            ],
            "SettingsSnapshot drifted from ipc.ts"
        );
        assert!(v.get("reduce_motion").is_none(), "camelCase, not snake_case");
    }

    /// Defaults are the shape a first launch reports: motion on, calculator
    /// automatic. Both agree with the Rust-side defaults the Bangless path uses,
    /// so a window that has never been opened cannot disagree with the pipeline.
    #[test]
    fn v0_6_an_untouched_install_reports_the_documented_defaults() {
        let prefs = crate::prefs::Prefs::open(None).unwrap();
        let snap = Snapshot::read(&prefs);

        assert!(!snap.reduce_motion);
        assert_eq!(snap.calc_policy, "automatic");
    }

    /// What `lib.rs` reads into the pipeline before any window exists.
    ///
    /// Through v0.5 the Policy was pushed from the frontend only, so every
    /// keystroke before the Palette mounted answered under Automatic whatever had
    /// been chosen — a restart silently reverting the setting.
    #[test]
    fn v0_6_a_stored_calculator_policy_is_what_startup_reads() {
        let prefs = crate::prefs::Prefs::open(None).unwrap();
        prefs.set(crate::prefs::CALC_POLICY, "explicit").unwrap();

        assert_eq!(Snapshot::read(&prefs).calc_policy, "explicit");
        assert_eq!(
            crate::sources::calc::Policy::parse(
                prefs
                    .get(crate::prefs::CALC_POLICY)
                    .as_deref()
                    .unwrap_or_default()
            ),
            crate::sources::calc::Policy::Explicit
        );
    }

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
