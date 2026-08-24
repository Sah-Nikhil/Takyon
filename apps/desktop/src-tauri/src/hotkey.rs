//! The global hotkey.
//!
//! `Alt+Space` is the default and it is a *contested* default: PowerToys Run uses
//! it, and Windows itself has used it for the window system menu since 3.0. So
//! registration failing is an ordinary Tuesday, not an edge case, and the one
//! behaviour that is not acceptable is failing quietly — a launcher whose hotkey
//! does nothing is indistinguishable from a launcher that crashed at login.
//!
//! Rebinding is v0.6 work (it needs a settings UI). What v0.1 owes is that the
//! failure is *visible* and that the tray still opens the Palette.

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// The default binding. Rebindable from v0.6; until then this is it.
pub const DEFAULT_ACCELERATOR: &str = "Alt+Space";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyStatus {
    pub accelerator: String,
    pub registered: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub struct HotkeyState(pub Mutex<HotkeyStatus>);

impl HotkeyState {
    pub fn get(&self) -> HotkeyStatus {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// Register the hotkey and record what happened.
///
/// Runs as early as possible in `setup` — before the tray, before autostart
/// self-heal, before anything touching disk. The "login -> hotkey responsive
/// < 500 ms" budget is met by ordering, not by speed: everything that is not this
/// is deferred behind it.
pub fn register(app: &AppHandle) {
    let accelerator = DEFAULT_ACCELERATOR.to_string();

    let status = match accelerator.parse::<Shortcut>() {
        Ok(shortcut) => {
            let handler = |app: &AppHandle, _shortcut: &Shortcut, event: tauri_plugin_global_shortcut::ShortcutEvent| {
                // The handler fires for press *and* release. Without this filter
                // the Palette opens on the way down and closes on the way up,
                // which reads as the hotkey not working at all.
                if event.state() != ShortcutState::Pressed {
                    return;
                }
                let bench = app.state::<crate::bench::Bench>();
                crate::window::toggle(app, &bench);
            };

            match app.global_shortcut().on_shortcut(shortcut, handler) {
                Ok(()) => HotkeyStatus {
                    accelerator,
                    registered: true,
                    error: None,
                },
                Err(e) => HotkeyStatus {
                    accelerator,
                    registered: false,
                    error: Some(explain(&e.to_string())),
                },
            }
        }
        Err(e) => HotkeyStatus {
            accelerator,
            registered: false,
            error: Some(format!("{DEFAULT_ACCELERATOR} is not a valid shortcut: {e}")),
        },
    };

    if !status.registered {
        report(app, &status);
    }

    app.manage(HotkeyState(Mutex::new(status)));
}

/// Turn the plugin's error into something worth reading.
///
/// Pure and string-in/string-out so it can be tested without registering
/// anything. The default case passes the original through verbatim rather than
/// flattening it into a friendly lie — an unrecognised failure the user can quote
/// in a bug report beats a reassuring sentence that says nothing.
pub fn explain(err: &str) -> String {
    let lower = err.to_lowercase();
    if lower.contains("already") || lower.contains("registered") || lower.contains("hot key") {
        format!(
            "Another application is already holding it. PowerToys Run uses \
             {DEFAULT_ACCELERATOR} by default, and so does the classic window menu."
        )
    } else {
        err.to_string()
    }
}

/// Say so, in a dialog.
///
/// It has to be a native dialog and not something drawn in the Palette: if the
/// hotkey is dead the user has no way to open the Palette to read the message,
/// which is precisely the state being reported. The Palette also carries a banner
/// (for whoever arrives via the tray), but this is the one that reaches someone
/// who does not yet know anything is wrong.
///
/// On its own thread, and not for tidiness: `blocking_show` waits on a channel the
/// event loop is responsible for feeding, so calling it from `setup` would
/// deadlock the app at startup — with a dialog that never appears, reporting a
/// hotkey that never worked.
fn report(app: &AppHandle, status: &HotkeyStatus) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

    let message = format!(
        "{} could not be registered.\n\n{}\n\n{} is running: open it from the tray icon in the notification area. A rebindable hotkey arrives in v0.6.",
        status.accelerator,
        status.error.as_deref().unwrap_or("Reason unknown."),
        crate::identity::DISPLAY_NAME,
    );

    let app = app.clone();
    std::thread::spawn(move || {
        app.dialog()
            .message(message)
            .title(crate::identity::DISPLAY_NAME)
            .kind(MessageDialogKind::Warning)
            .blocking_show();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_1_the_default_accelerator_parses() {
        assert!(DEFAULT_ACCELERATOR.parse::<Shortcut>().is_ok());
    }

    /// The collision is the expected failure, so it gets the explanation that
    /// actually helps: naming the two things most likely to be holding it.
    #[test]
    fn v0_1_a_taken_hotkey_names_the_usual_culprits() {
        let msg = explain("HotKey already registered");
        assert!(msg.contains("PowerToys Run"));
        assert!(msg.contains(DEFAULT_ACCELERATOR));
    }

    /// Anything unrecognised passes through unchanged. A catch-all reassurance
    /// would turn a real failure into a shrug.
    #[test]
    fn v0_1_an_unknown_failure_is_quoted_verbatim() {
        assert_eq!(explain("XGrabKey returned BadAccess"), "XGrabKey returned BadAccess");
    }

    /// The status serialises to the shape `packages/shared/src/ipc.ts` declares:
    /// camelCase, and `error` absent rather than null when registration worked.
    #[test]
    fn v0_1_status_serialises_to_the_declared_contract() {
        let ok = HotkeyStatus {
            accelerator: DEFAULT_ACCELERATOR.into(),
            registered: true,
            error: None,
        };
        let v: serde_json::Value = serde_json::to_value(&ok).unwrap();
        assert_eq!(v["accelerator"].as_str(), Some(DEFAULT_ACCELERATOR));
        assert_eq!(v["registered"].as_bool(), Some(true));
        assert!(
            v.get("error").is_none(),
            "`error?: string` means absent, not null"
        );

        let failed = HotkeyStatus {
            accelerator: DEFAULT_ACCELERATOR.into(),
            registered: false,
            error: Some("taken".into()),
        };
        let v: serde_json::Value = serde_json::to_value(&failed).unwrap();
        assert_eq!(v["error"].as_str(), Some("taken"));
    }
}
