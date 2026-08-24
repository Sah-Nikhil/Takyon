//! Takyon's Rust core.
//!
//! v0.1 is the warm shell: a Palette window that exists before it is needed, a
//! global hotkey, a tray icon and a login registration. No Sources, no ranking, no
//! Bangs. The phase exists to test the ADR-0003 bet with numbers, not to ship
//! features.
//!
//! **Startup order is load-bearing.** The "login -> hotkey responsive < 500 ms"
//! budget is met by ordering rather than by speed: the hotkey is registered before
//! anything else, and every other piece of startup runs behind it on another
//! thread. Adding work above `hotkey::register` is how that budget quietly breaks.

pub mod bench;
pub mod firstrun;
pub mod hotkey;
pub mod identity;
pub mod settings;
pub mod tray;
pub mod uiaccess;
pub mod window;

use std::time::Instant;
use tauri::{Manager, WindowEvent};

use bench::Bench;
use hotkey::{HotkeyState, HotkeyStatus};

/// Hide the Palette. Called by Escape, which is handled in the frontend because
/// that is where the keystroke lands.
#[tauri::command]
fn dismiss(app: tauri::AppHandle) {
    window::hide(&app, "frontend dismiss (Escape)");
}

#[tauri::command]
fn open_settings(app: tauri::AppHandle) {
    settings::open(&app);
}

#[tauri::command]
fn hotkey_status(state: tauri::State<'_, HotkeyState>) -> HotkeyStatus {
    state.get()
}

/// The frontend reporting that a show's frame has been painted. See `bench.rs` for
/// why the timing lives entirely on this side.
#[tauri::command]
fn report_first_pixel(show_id: u64, bench: tauri::State<'_, Bench>) {
    bench.first_pixel(show_id);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Taken first, before anything Tauri does, so the login budget measures the
    // whole process and not just the part after the runtime was ready.
    let started = Instant::now();

    tauri::Builder::default()
        // Must be registered first, per the plugin's own guidance. It is required
        // *because of* autostart: "one from login, one double-clicked" goes from
        // rare to routine, and a second Takyon means a second global-shortcut
        // registration that silently loses the race for Alt+Space.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            let bench = app.state::<Bench>();
            window::show(app, &bench);
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                // ADR-0011. Without this the plugin keys the `Run` value off
                // `productName`, i.e. "Takyon" — and then renaming the product
                // orphans the registry entry, which is the exact migration the ADR
                // exists to avoid. The NSIS uninstall hook deletes this same name.
                .app_name(identity::IDENTITY)
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            dismiss,
            open_settings,
            hotkey_status,
            report_first_pixel
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            // Managed before the hotkey, because the hotkey handler reaches for it
            // on the very first press.
            app.manage(Bench::from_env(started));

            // Everything above this line is plugin registration that Tauri has
            // already done. This is the first thing that makes the app useful, and
            // it stays first.
            hotkey::register(&handle);
            app.state::<Bench>().startup_ready();

            if let Err(e) = tray::build(&handle) {
                // Not fatal, but close to it: the Palette has no taskbar button, so
                // with no tray icon and a dead hotkey there is no way to quit
                // except Task Manager. Say so loudly.
                eprintln!("[takyon] the tray icon could not be created: {e}");
            }

            // Deferred init. Nothing below is on any latency budget, and two of the
            // three can block: `blocking_show` deadlocks if called on the main
            // thread, and the autostart registry write is disk-bound.
            let deferred = handle.clone();
            std::thread::spawn(move || {
                tray::self_heal_autostart(&deferred);
                uiaccess::start(&deferred);
                firstrun::maybe_prompt(&deferred);
            });

            Ok(())
        })
        .on_window_event(|win, event| {
            if win.label() != window::PALETTE {
                return;
            }
            match event {
                // ROADMAP v0.1: dismiss on focus loss. The guard is not just the
                // debug flag — see `window::should_hide_on_focus_loss`, which also
                // ignores the stray focus event WebView2 emits during its own
                // focus handover immediately after a show.
                WindowEvent::Focused(false) => {
                    let app = win.app_handle().clone();
                    if window::should_hide_on_focus_loss(&app) {
                        window::hide(&app, "focus loss");
                    }
                }
                // ADR-0003: the Palette is hidden, never destroyed. The window has
                // no close button, but Alt+F4 still asks — and honouring it would
                // throw away the warm WebView2 instance this whole phase is about,
                // turning every subsequent show into a cold start.
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    window::hide(&win.app_handle().clone(), "close requested");
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Takyon");
}
