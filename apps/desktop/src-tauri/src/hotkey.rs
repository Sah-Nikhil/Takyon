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

/// Override the accelerator for one run.
///
/// **Not the v0.6 rebinding feature**, which needs a settings UI and persistence.
/// This is a debug affordance in the same spirit as [`crate::window::NO_FOCUS_STEAL_ENV`]:
/// something that makes the app measurable on a machine whose state would
/// otherwise prevent it.
///
/// The specific need is `bun run bench`. Every span that harness measures starts
/// at a hotkey press, so on any machine already running PowerToys Run or Raycast —
/// both of which take `Alt+Space` by default — the benchmark cannot produce a
/// single number. That is most machines this will ever be developed on, and a
/// performance harness that only runs somewhere else is a performance harness
/// nobody runs.
pub const ACCELERATOR_ENV: &str = "TAKYON_HOTKEY";

/// What the Keyboard page offers, in the order it draws them.
///
/// Pinned rather than a raw capture field (ROADMAP v0.6): a capture field invites
/// chords Windows reserves and reports the failure only afterwards.
pub const CHOICES: [&str; 6] = [
    "Alt+Space",
    "Ctrl+Space",
    "Alt+Shift+Space",
    "Ctrl+Shift+Space",
    "Ctrl+Alt+Space",
    "Ctrl+Shift+P",
];

/// Pick the accelerator to register: env override, then stored, then default.
///
/// Pure so the precedence is testable without registering anything. Anything that
/// does not parse falls through to the next source rather than disabling the
/// hotkey, because a launcher whose hotkey is dead looks like one that crashed.
pub fn resolve(stored: Option<&str>, env: Option<&str>) -> String {
    for (label, candidate) in [("override", env), ("stored", stored)] {
        let Some(value) = candidate.map(str::trim).filter(|v| !v.is_empty()) else {
            continue;
        };
        if value.parse::<Shortcut>().is_ok() {
            return value.to_string();
        }
        eprintln!("[takyon] {label} hotkey {value:?} is not a valid accelerator; ignoring it");
    }
    DEFAULT_ACCELERATOR.to_string()
}

/// The accelerator this process will try to register, from storage and the env.
pub fn accelerator(prefs: &crate::prefs::Prefs) -> String {
    resolve(
        prefs.get(crate::prefs::HOTKEY).as_deref(),
        std::env::var(ACCELERATOR_ENV).ok().as_deref(),
    )
}

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
pub fn register(app: &AppHandle, accelerator: String) {
    let status = attempt(app, accelerator);
    if !status.registered {
        report(app, &status);
    }
    app.manage(HotkeyState(Mutex::new(status)));
}

/// Try one accelerator, reporting what happened. Registers nothing on failure.
fn attempt(app: &AppHandle, accelerator: String) -> HotkeyStatus {
    match accelerator.parse::<Shortcut>() {
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
            error: Some(format!("not a valid shortcut: {e}")),
        },
    }
}

/// Rebind the hotkey and remember the choice (v0.6).
///
/// Releases the old binding first, or two chords open the Palette. Restores it on
/// failure, so a refused chord cannot leave the app with nothing bound.
pub fn rebind(app: &AppHandle, accelerator: &str, prefs: &crate::prefs::Prefs) -> HotkeyStatus {
    let previous = app.state::<HotkeyState>().get();
    if let Ok(old) = previous.accelerator.parse::<Shortcut>() {
        let _ = app.global_shortcut().unregister(old);
    }

    let mut status = attempt(app, accelerator.to_string());
    if !status.registered {
        // Put the old one back rather than leaving nothing bound.
        let restored = attempt(app, previous.accelerator.clone());
        if restored.registered {
            status.error = Some(format!(
                "{}. Kept {}.",
                status.error.as_deref().unwrap_or("could not be registered"),
                previous.accelerator
            ));
        }
    }

    if status.registered {
        if let Err(e) = prefs.set(crate::prefs::HOTKEY, &status.accelerator) {
            eprintln!("[takyon] the hotkey could not be saved: {e}");
        }
    }

    let held = app.state::<HotkeyState>();
    let live = if status.registered {
        status.clone()
    } else {
        previous
    };
    *held.0.lock().unwrap_or_else(|e| e.into_inner()) = live;
    status
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

    /// Every chip the Keyboard page offers has to be registrable, or the control
    /// offers a choice that cannot be taken.
    #[test]
    fn v0_6_every_offered_binding_parses() {
        assert!(CHOICES.contains(&DEFAULT_ACCELERATOR));
        for choice in CHOICES {
            assert!(choice.parse::<Shortcut>().is_ok(), "{choice} does not parse");
        }
    }

    /// Precedence, and the order matters in both directions.
    ///
    /// The env override has to beat a stored binding or `bun run bench` cannot
    /// measure a machine whose user has rebound the hotkey. A stored binding has
    /// to beat the default or rebinding does not survive a restart.
    #[test]
    fn v0_6_the_override_beats_storage_which_beats_the_default() {
        assert_eq!(resolve(None, None), DEFAULT_ACCELERATOR);
        assert_eq!(resolve(Some("Ctrl+Space"), None), "Ctrl+Space");
        assert_eq!(resolve(None, Some("Ctrl+Alt+F9")), "Ctrl+Alt+F9");
        assert_eq!(resolve(Some("Ctrl+Space"), Some("Ctrl+Alt+F9")), "Ctrl+Alt+F9");
    }

    /// A binding that no longer parses must not leave the launcher with no hotkey
    /// at all. It falls through to the next source rather than failing.
    #[test]
    fn v0_6_an_unparseable_binding_falls_through_rather_than_disabling_the_hotkey() {
        assert_eq!(resolve(Some("Ctrl+Nonsense"), None), DEFAULT_ACCELERATOR);
        // A typo in the environment variable must not override a good stored one.
        assert_eq!(resolve(Some("Ctrl+Space"), Some("!!!")), "Ctrl+Space");
        assert_eq!(resolve(Some("  "), None), DEFAULT_ACCELERATOR);
    }

    /// The chord `bun run bench` uses when Alt+Space is taken. If this ever stops
    /// parsing, the benchmark silently falls back to the contested default and
    /// then reports that the hotkey is unavailable.
    #[test]
    fn v0_2_the_bench_override_chord_parses() {
        assert!("Ctrl+Alt+F9".parse::<Shortcut>().is_ok());
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
