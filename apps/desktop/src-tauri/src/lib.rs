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

pub mod actions;
pub mod aliases;
pub mod bench;
pub mod com;
pub mod entry;
pub mod firstrun;
pub mod frecency;
pub mod hotkey;
pub mod icons;
pub mod identity;
pub mod launch;
pub mod query;
pub mod rank;
pub mod settings;
pub mod sources;
pub mod tray;
pub mod uiaccess;
pub mod version;
pub mod window;

use std::sync::Arc;
use std::time::Instant;
use tauri::{Manager, WindowEvent};

use bench::Bench;
use entry::{Action, EntryId};
use hotkey::{HotkeyState, HotkeyStatus};
use icons::IconStore;
use query::{Pipeline, QueryResult};
use sources::apps::AppSource;

/// Hide the Palette. Called by Escape, which is handled in the frontend because
/// that is where the keystroke lands.
#[tauri::command]
fn dismiss(app: tauri::AppHandle) {
    window::hide(&app, "frontend dismiss (Escape)");
}

/// One keystroke. See `query.rs` — one `invoke` per keystroke (ADR-0009).
///
/// The window is resized here, before the response returns: the row count is
/// already known on this side, and resizing first means rows paint into a window
/// that is already the right shape rather than one catching up a frame later.
#[tauri::command]
fn query(
    app: tauri::AppHandle,
    q: String,
    seq: u64,
    pipeline: tauri::State<'_, Arc<Pipeline>>,
    bench: tauri::State<'_, Bench>,
) -> QueryResult {
    bench.mark_query(seq);
    let result = pipeline.query(&q, seq);
    window::set_rows(&app, result.entries.len(), result.indexing);
    result
}

/// The `Ctrl+K` menu for one Entry.
#[tauri::command]
fn actions_for(entry_id: String, pipeline: tauri::State<'_, Arc<Pipeline>>) -> Vec<Action> {
    pipeline.actions_for(&EntryId(entry_id))
}

/// Tell the window the action menu opened or closed, so it can make room.
///
/// Four actions need ~200px against a 120px window. The frontend cannot fix that
/// from inside the webview — it is the native window that is too short.
#[tauri::command]
fn set_action_menu(app: tauri::AppHandle, actions: Option<usize>) {
    window::set_menu(&app, actions);
}

/// Report the measured hotkey-banner height, 0 if there is none.
///
/// Rust owns `HotkeyState` so it knows *whether* the banner is drawn, but not how
/// tall wrapping text turned out — and height is the number that matters. A
/// constant guessed here was 16px short at 150% and clipped the list's last row.
#[tauri::command]
fn set_banner_height(app: tauri::AppHandle, height: u32) {
    window::set_banner(&app, height);
}

/// Perform an action on an Entry.
///
/// **Hides first, then launches** (v0.2 task 7): `ShellExecuteW` returns when the
/// shell accepts the request, not when a window exists. The launch then moves to a
/// background thread, so the IPC reply is not held open across a UAC prompt.
#[tauri::command]
fn activate(
    app: tauri::AppHandle,
    entry_id: String,
    action_id: String,
    pipeline: tauri::State<'_, Arc<Pipeline>>,
) -> Result<(), String> {
    window::hide(&app, "activation");

    let pipeline = pipeline.inner().clone();
    let id = EntryId(entry_id);
    std::thread::spawn(move || {
        if let Err(e) = pipeline.activate(&id, &action_id) {
            // Nowhere better to put this yet: the Palette is already hidden and
            // v0.2 has no toast surface. v0.6's crash-log folder is where this
            // ends up (ADR-0010 — written locally, never sent).
            eprintln!("[takyon] {e}");
        }
    });
    Ok(())
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

/// The frontend reporting that it has painted Entries for a query.
///
/// The second half of §10's "hotkey to first Entry" budget, and it becomes
/// measurable at v0.2 because that is when there is a Source to produce one.
#[tauri::command]
fn report_first_entry(seq: u64, bench: tauri::State<'_, Bench>) {
    bench.first_entry(seq);
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
        // Icons reach the webview as URLs rather than as bytes in the query
        // response (`icons.rs`). **Asynchronous**, not the synchronous form: a
        // cache miss extracts from the shell, and the synchronous handler runs on
        // a thread WebView2 needs, so a slow extraction there stalls rendering.
        .register_asynchronous_uri_scheme_protocol(icons::SCHEME, |ctx, request, responder| {
            let store = ctx.app_handle().state::<Arc<IconStore>>().inner().clone();
            // The key is the last path segment. Everything before it is the
            // scheme's synthetic host, which differs between platforms and is not
            // ours to interpret.
            let key = request
                .uri()
                .path()
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_string();

            std::thread::spawn(move || {
                let response = match store.get(&key) {
                    Some(bytes) => tauri::http::Response::builder()
                        .status(200)
                        .header("Content-Type", "image/png")
                        // The key already contains the source's mtime, so a given
                        // URL's bytes can never change. Caching it hard is what
                        // makes the second showing of a query cost nothing.
                        .header("Cache-Control", "public, max-age=31536000, immutable")
                        .body(bytes),
                    // A miss is cosmetic: the row is already on screen with its
                    // placeholder. Never a panic — this path is reachable from the
                    // webview, so it is reachable from anything the webview loads.
                    None => tauri::http::Response::builder().status(404).body(Vec::new()),
                };
                if let Ok(response) = response {
                    responder.respond(response);
                }
            });
        })
        .invoke_handler(tauri::generate_handler![
            dismiss,
            open_settings,
            hotkey_status,
            report_first_pixel,
            report_first_entry,
            query,
            actions_for,
            activate,
            set_action_menu,
            set_banner_height
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            // Managed before the hotkey, because the hotkey handler reaches for it
            // on the very first press.
            app.manage(Bench::from_env(started));

            // Managed before the hotkey too, and for the same reason: the first
            // keystroke after the very first show reaches for the pipeline, and
            // the walk that fills it has not started yet. An `AppSource` reports
            // `is_indexing` from construction precisely so this ordering is safe.
            let apps = Arc::new(AppSource::new());
            let icons = Arc::new(IconStore::default());
            app.manage(icons.clone());

            // A usage database that cannot be opened must not stop the launcher.
            // In memory it learns for this session and forgets at exit, which is
            // a worse Palette rather than no Palette.
            let frecency = Arc::new(
                frecency::Frecency::open(identity::data_dir())
                    .or_else(|e| {
                        eprintln!("[takyon] frecency.db could not be opened: {e}");
                        frecency::Frecency::open(None)
                    })
                    .expect("an in-memory database always opens"),
            );
            // Same fallback, same reason: a launcher with no aliases still
            // launches things.
            let aliases = Arc::new(
                aliases::AliasStore::open(identity::data_dir())
                    .or_else(|e| {
                        eprintln!("[takyon] settings.db could not be opened: {e}");
                        aliases::AliasStore::open(None)
                    })
                    .expect("an in-memory database always opens"),
            );
            app.manage(aliases.clone());
            let recents = Arc::new(sources::recents::RecentsSource::new());
            let system = Arc::new(sources::system::SystemSource::new());
            app.manage(Arc::new(Pipeline::new(
                apps.clone(),
                recents.clone(),
                system.clone(),
                icons.clone(),
                frecency.clone(),
            )));

            // Everything above this line is plugin registration that Tauri has
            // already done. This is the first thing that makes the app useful, and
            // it stays first.
            hotkey::register(&handle);
            let bench = app.state::<Bench>();
            bench.startup_ready();
            // Every span the harness measures starts at a hotkey press, so a taken
            // Alt+Space means the run can produce nothing. Said here rather than
            // left to time out as "no painted frame", which reads as a rendering
            // bug and is not one.
            if !app.state::<HotkeyState>().get().registered {
                bench.hotkey_unavailable();
            }

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

            // The application walk, on its own thread: `firstrun::maybe_prompt` can sit
            // on a modal dialog indefinitely, and queueing discovery behind it would mean
            // the launcher knows no applications until the prompt is answered. Nothing to
            // serve in the meantime, deliberately — ADR-0012.
            std::thread::spawn(move || {
                apps.refresh(&icons);
                // After the walk, because an alias attaches to an application
                // that has to exist first. Cheap enough to redo whenever the
                // alias table changes, which is what v0.6's editor will do.
                apps.apply_aliases(&aliases);

                // System entries: the curated settings table plus one COM walk of
                // the All Tasks folder. Static, so read once here rather than on a
                // timer. After the app walk because it is not on any budget and
                // the app list is what the first keystroke wants.
                system.refresh();

                // Recents are read on a timer rather than per keystroke: a few
                // hundred shortcuts through COM would blow the 20 ms budget many
                // times over. A query answers from the last snapshot.
                let recents_refresh = recents.clone();
                std::thread::spawn(move || loop {
                    recents_refresh.refresh();
                    std::thread::sleep(sources::recents::REFRESH_EVERY);
                });
                // Then persist icons on a debounce, forever. Extraction is lazy,
                // so v0.2's single flush here always wrote an empty blob (tbd
                // v0.2 §10). Failure costs one re-extraction, so it is dropped.
                loop {
                    std::thread::sleep(icons::FLUSH_DEBOUNCE);
                    if icons::should_flush(icons.pending(), icons.idle()) {
                        if let Err(e) = icons.flush() {
                            eprintln!("[takyon] could not write the icon cache: {e}");
                        }
                    }
                }
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
