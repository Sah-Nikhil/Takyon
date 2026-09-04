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
pub mod bang;
pub mod bench;
pub mod clips;
pub mod com;
pub mod crashlog;
pub mod entry;
pub mod firstrun;
pub mod frecency;
pub mod hotkey;
pub mod icons;
pub mod identity;
pub mod index;
pub mod launch;
pub mod prefs;
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
use clips::{Blocklist, Clip, ClipStore, Retention};
use entry::{Action, EntryId};
use hotkey::{HotkeyState, HotkeyStatus};
use icons::IconStore;
use index::live::WalkIndex;
use query::{Pipeline, QueryResult};
use sources::apps::AppSource;
use sources::calc::Policy as CalcPolicy;

/// One clipboard row as the history surface draws it.
///
/// Declared here rather than in `clips/store.rs` because it is wire shape, not
/// storage: `Clip` is what the database holds, this is what crosses IPC.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ClipRow {
    id: i64,
    created_at: i64,
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_exe: Option<String>,
    len: usize,
    preview: String,
}

impl From<Clip> for ClipRow {
    fn from(clip: Clip) -> Self {
        ClipRow {
            id: clip.id,
            created_at: clip.created_at,
            kind: clip.kind.as_str(),
            source_exe: clip.source_exe,
            len: clip.len,
            preview: clip.preview,
        }
    }
}

/// How often the retention sweep runs after the one at startup.
///
/// An hour, because the shortest window is a day: a clip lives at most an hour
/// past its window, and a timer that fires more often only wakes a process whose
/// premise is being idle (ADR-0003).
const SWEEP_EVERY: std::time::Duration = std::time::Duration::from_secs(3600);

/// Retention as stored, or the default where nothing has been chosen.
fn stored_retention(prefs: &prefs::Prefs) -> Retention {
    prefs
        .get(prefs::CLIPS_RETENTION)
        .map_or_else(Retention::default, |v| Retention::parse(&v))
}

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
    // Rust already holds the Entries, so the window learns the list's shape
    // without a second `invoke` — a calculation is drawn as a card, not a row.
    let calc_card = result
        .entries
        .first()
        .is_some_and(|e| e.kind == entry::EntryKind::Calc);
    window::set_rows(&app, result.entries.len(), result.indexing, calc_card);
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
    // Deleting a clip is the exception: it is a change to the list you are
    // looking at, so hiding the window would end the session you are in the
    // middle of (v0.5).
    if query::hides_palette(&action_id) {
        window::hide(&app, "activation");
    }

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

/// When the calculator may answer (v0.4).
///
/// Stored **and** pushed, since v0.6 gave it a home: the pipeline reads the
/// policy on the keystroke path and must not go to SQLite for it, and the stored
/// copy is what startup reads before any window exists to push one.
#[tauri::command]
fn set_calc_policy(
    policy: String,
    prefs: tauri::State<'_, Arc<prefs::Prefs>>,
    pipeline: tauri::State<'_, Arc<Pipeline>>,
) {
    let policy = CalcPolicy::parse(&policy);
    if let Err(e) = prefs.set(prefs::CALC_POLICY, policy.as_str()) {
        eprintln!("[takyon] the calculator setting could not be saved: {e}");
    }
    pipeline.calc.set_policy(policy);
}

/// Every action id and its label, fetched once on mount (v0.4.5 task 4).
///
/// The footer names what Enter will do on the selected row, and labels live in
/// `actions.rs` (ADR-0009). Sent once rather than per arrow key.
#[tauri::command]
fn action_labels() -> Vec<Action> {
    actions::all()
}

/// How long clipboard history is kept, as its stored spelling.
#[tauri::command]
fn clip_retention(prefs: tauri::State<'_, Arc<prefs::Prefs>>) -> String {
    stored_retention(&prefs).as_str().to_string()
}

/// How many clips a retention change would destroy.
///
/// Read *before* the change, so the confirmation can name the real number rather
/// than "some items" (v0.5 traps). Zero for a window that removes nothing.
#[tauri::command]
fn clip_retention_impact(
    value: String,
    clips: tauri::State<'_, Option<Arc<ClipStore>>>,
) -> usize {
    let Some(store) = clips.inner() else {
        return 0;
    };
    match Retention::parse(&value).seconds() {
        Some(seconds) => store.count_older_than(clips::store::unix_now() - seconds),
        None => 0,
    }
}

/// Set retention and sweep immediately. Returns how many clips were destroyed.
///
/// Destructive by design: ADR-0006 says expiry deletes rather than hides. The
/// caller is expected to have confirmed with [`clip_retention_impact`] first.
#[tauri::command]
fn set_clip_retention(
    value: String,
    prefs: tauri::State<'_, Arc<prefs::Prefs>>,
    clips: tauri::State<'_, Option<Arc<ClipStore>>>,
) -> usize {
    let retention = Retention::parse(&value);
    if let Err(e) = prefs.set(prefs::CLIPS_RETENTION, retention.as_str()) {
        eprintln!("[takyon] retention could not be saved: {e}");
    }
    clips.inner().as_ref().map_or(0, |s| s.sweep(retention))
}

/// Destroy the whole history. Returns how many clips went.
#[tauri::command]
fn clip_clear(clips: tauri::State<'_, Option<Arc<ClipStore>>>) -> usize {
    clips.inner().as_ref().map_or(0, |s| s.clear())
}

/// Open or close a full-window View (v0.5).
///
/// The Palette grows into the surface rather than opening a window: a third
/// WebView2 would cost the login budget and a large share of the 150 MB ceiling,
/// for something opened many times a day.
#[tauri::command]
fn set_view(app: tauri::AppHandle, view: Option<String>) {
    let view = match view.as_deref() {
        Some("clipboard-history") => Some(window::View::ClipboardHistory),
        _ => None,
    };
    window::set_view(&app, view);
}

/// The clipboard history page for the surface: newest first, filtered.
///
/// Previews only — full content never travels with a list, or a search response
/// ships every matching secret into the webview (`clips/store.rs`).
#[tauri::command]
fn clip_page(
    query: String,
    limit: Option<usize>,
    clips: tauri::State<'_, Option<Arc<ClipStore>>>,
) -> Vec<ClipRow> {
    let Some(store) = clips.inner() else {
        return Vec::new();
    };
    store
        .search(&query, limit.unwrap_or(clips::store::PAGE))
        .into_iter()
        .map(ClipRow::from)
        .collect()
}

/// Whether `!v` reaches clipboard history.
#[tauri::command]
fn clip_bang(prefs: tauri::State<'_, Arc<prefs::Prefs>>) -> bool {
    prefs::flag(&prefs, prefs::CLIPS_BANG, true)
}

/// Turn the `!v` Bang on or off. The command stays either way.
#[tauri::command]
fn set_clip_bang(
    on: bool,
    prefs: tauri::State<'_, Arc<prefs::Prefs>>,
    pipeline: tauri::State<'_, Arc<Pipeline>>,
) {
    if let Err(e) = prefs.set(prefs::CLIPS_BANG, if on { "1" } else { "0" }) {
        eprintln!("[takyon] the clipboard Bang setting could not be saved: {e}");
    }
    // Pushed as well as stored: the pipeline reads this on the keystroke path and
    // must not go to SQLite for it.
    pipeline.set_bang_enabled(on);
}

/// Executables whose clipboard is never recorded (ADR-0006).
#[tauri::command]
fn clip_blocklist(blocklist: tauri::State<'_, Option<Arc<Blocklist>>>) -> Vec<String> {
    blocklist.inner().as_ref().map_or_else(Vec::new, |b| b.all())
}

/// Add or remove one executable. The store reloads its own cache on write, so the
/// change applies to the next capture rather than the next launch (tbd v0.5 §6).
#[tauri::command]
fn set_clip_blocked(
    exe: String,
    blocked: bool,
    blocklist: tauri::State<'_, Option<Arc<Blocklist>>>,
) -> Result<Vec<String>, String> {
    let Some(b) = blocklist.inner().as_ref() else {
        return Err("the blocklist could not be opened".into());
    };
    let exe = exe.trim().to_ascii_lowercase();
    if exe.is_empty() {
        return Err("an executable name is required".into());
    }
    if blocked { b.add(&exe) } else { b.remove(&exe) }.map_err(|e| e.to_string())?;
    Ok(b.all())
}

/// One alias and what it points at, for the Applications page.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AliasRow {
    alias: String,
    target: String,
    /// The Entry's title today, or `None` when it no longer resolves.
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
}

#[tauri::command]
fn aliases(
    store: tauri::State<'_, Arc<aliases::AliasStore>>,
    apps: tauri::State<'_, Arc<AppSource>>,
) -> Vec<AliasRow> {
    let mut rows: Vec<AliasRow> = store
        .by_target()
        .into_iter()
        .flat_map(|(target, names)| {
            // `None` when the alias outlived its application — an uninstall, or a
            // rename. The row still lists so it can be deleted.
            let title = apps.find(&target).map(|a| a.title);
            names.into_iter().map(move |alias| AliasRow {
                alias,
                target: target.0.clone(),
                title: title.clone(),
            })
        })
        .collect();
    rows.sort_by(|a, b| a.alias.cmp(&b.alias));
    rows
}

/// Create or delete an alias, then re-apply the table (v0.3 tbd §3).
///
/// `apply_aliases` is in-place and needs no re-walk, which is why the editor can
/// be apply-on-change rather than restart-to-take-effect.
#[tauri::command]
fn set_alias(
    alias: String,
    target: Option<String>,
    store: tauri::State<'_, Arc<aliases::AliasStore>>,
    apps: tauri::State<'_, Arc<AppSource>>,
) -> Result<(), String> {
    let alias = alias.trim().to_string();
    if alias.is_empty() {
        return Err("an alias needs a name".into());
    }
    match target {
        Some(id) => store.set(&alias, &EntryId(id)),
        None => store.remove(&alias),
    }
    .map_err(|e| e.to_string())?;
    apps.apply_aliases(&store);
    Ok(())
}

/// Open the crash-log folder in Explorer (ADR-0010).
///
/// It opens the folder. **It does not upload anything**, and there is no code
/// path in Takyon that would.
#[tauri::command]
fn open_crash_logs() -> Result<(), String> {
    let dir = crashlog::dir().ok_or("there is nowhere to write logs")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // Through `ShellExecuteW` like every other launch, so Explorer does not
    // inherit our handles (v0.2 task 7).
    launch::open(&entry::LaunchTarget::Exe {
        path: dir,
        args: None,
        working_dir: None,
    })
    .map(|_| ())
}

#[tauri::command]
fn open_settings(app: tauri::AppHandle) {
    settings::open(&app);
}

#[tauri::command]
fn hotkey_status(state: tauri::State<'_, HotkeyState>) -> HotkeyStatus {
    state.get()
}

/// The bindings the Keyboard page offers, and which one is the default.
#[tauri::command]
fn hotkey_choices() -> Vec<&'static str> {
    hotkey::CHOICES.to_vec()
}

/// Rebind the hotkey (v0.6). Returns what is live afterwards, refused or not.
///
/// `async` deliberately: registering touches the shortcut manager, and a
/// synchronous command would run this on the main thread.
#[tauri::command(async)]
fn set_hotkey(
    app: tauri::AppHandle,
    accelerator: String,
    prefs: tauri::State<'_, Arc<prefs::Prefs>>,
) -> HotkeyStatus {
    hotkey::rebind(&app, &accelerator, &prefs)
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

    // Before anything that could panic. A release build has no console, so
    // without this a panic is completely silent (ADR-0010 — written, never sent).
    crashlog::install();

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
            index::file_index_status,
            actions_for,
            activate,
            set_action_menu,
            set_banner_height,
            set_calc_policy,
            action_labels,
            clip_retention,
            clip_retention_impact,
            set_clip_retention,
            clip_clear,
            clip_page,
            clip_bang,
            set_clip_bang,
            set_view,
            settings::settings_snapshot,
            settings::set_reduce_motion,
            settings::migrate_local_prefs,
            settings::set_recents,
            settings::set_tray,
            settings::set_placement,
            settings::set_theme,
            settings::set_ui_size,
            hotkey_choices,
            set_hotkey,
            clip_blocklist,
            set_clip_blocked,
            aliases,
            set_alias,
            open_crash_logs
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
            // Managed so the settings window can resolve an alias target back to a
            // title, and re-apply the table without a re-walk (v0.6).
            app.manage(apps.clone());

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

            // Clipboard history (v0.5). Unlike the databases above there is **no
            // in-memory fallback**: an unwritable `clips.db` or an unreadable key
            // must stop capture, not quietly record secrets somewhere that looks
            // like history and is not (ADR-0006).
            let clip_store = match ClipStore::open(identity::data_dir()) {
                Ok(store) => Some(Arc::new(store)),
                Err(e) => {
                    eprintln!("[takyon] clipboard history is off: {e}");
                    None
                }
            };
            let prefs = Arc::new(
                prefs::Prefs::open(identity::data_dir())
                    .or_else(|e| {
                        eprintln!("[takyon] settings.db could not be opened: {e}");
                        prefs::Prefs::open(None)
                    })
                    .expect("an in-memory database always opens"),
            );
            app.manage(prefs.clone());
            app.manage(clip_store.clone());

            // Opened here rather than beside the capture thread so the settings
            // window can edit it even when capture is off. Its own writes reload
            // the cache, so an edit takes effect without a restart (tbd v0.5 §6).
            let blocklist = match Blocklist::open(identity::data_dir()) {
                Ok(b) => Some(Arc::new(b)),
                Err(e) => {
                    eprintln!("[takyon] the clipboard blocklist could not be opened: {e}");
                    None
                }
            };
            app.manage(blocklist.clone());

            let mut pipeline = Pipeline::new(
                apps.clone(),
                recents.clone(),
                system.clone(),
                icons.clone(),
                frecency.clone(),
            );
            if let Some(store) = clip_store.clone() {
                pipeline = pipeline.with_clips(store);
            }
            // Read once at startup rather than waiting for the frontend to push:
            // a keystroke can arrive before the Palette has mounted, and `!v`
            // silently falling through would look like a broken Bang.
            pipeline.set_bang_enabled(prefs::flag(&prefs, prefs::CLIPS_BANG, true));
            // Same reason, and it was the gap v0.6 closed: the policy was pushed
            // from the frontend only, so every keystroke before the Palette
            // mounted answered under Automatic whatever the user had chosen.
            pipeline.calc.set_policy(CalcPolicy::parse(
                prefs.get(prefs::CALC_POLICY).as_deref().unwrap_or_default(),
            ));
            pipeline.set_recents_enabled(prefs::flag(&prefs, prefs::RECENTS, true));
            // Interface size and placement into atomics, before the first show:
            // both sit on latency paths and must never reach SQLite there.
            window::cache_layout_prefs(&prefs);
            app.manage(Arc::new(pipeline));

            // The first thing that makes the app useful, and it stays first.
            // Reads `settings.db` so a rebound hotkey survives a restart: one
            // indexed lookup on an open connection, which is what that costs.
            hotkey::register(&handle, hotkey::accelerator(&prefs));

            // The file index, mapped from disk if it exists. **Below the hotkey
            // deliberately**: resolving the roots costs 3.5 ms of shell calls, and
            // nothing joins the queue above registration. Managed here, not on the
            // walk thread, so `file_index_status` cannot outrun the state.
            let file_index = Arc::new(WalkIndex::load(
                identity::data_dir()
                    .map(|d| d.join("index"))
                    .unwrap_or_default(),
                index::roots::defaults(),
            ));
            app.manage(file_index.clone());
            let bench = app.state::<Bench>();
            bench.startup_ready();
            // Every span the harness measures starts at a hotkey press, so a taken
            // Alt+Space means the run can produce nothing. Said here rather than
            // left to time out as "no painted frame", which reads as a rendering
            // bug and is not one.
            if !app.state::<HotkeyState>().get().registered {
                bench.hotkey_unavailable();
            }

            // Built either way, then hidden if that is the stored choice: a tray
            // icon that is never created cannot be brought back by a setting.
            let tray_wanted = prefs::flag(&prefs, prefs::TRAY, true);
            if let Err(e) = tray::build(&handle) {
                // Not fatal, but close to it: the Palette has no taskbar button, so
                // with no tray icon and a dead hotkey there is no way to quit
                // except Task Manager. Say so loudly.
                eprintln!("[takyon] the tray icon could not be created: {e}");
            } else if !tray_wanted {
                if let Err(e) = tray::set_visible(&handle, false) {
                    eprintln!("[takyon] the tray icon stays visible: {e}");
                }
            }

            // Deferred init. Nothing below is on any latency budget, and two of the
            // three can block: `blocking_show` deadlocks if called on the main
            // thread, and the autostart registry write is disk-bound.
            let deferred = handle.clone();
            std::thread::spawn(move || {
                tray::self_heal_autostart(&deferred);
                uiaccess::start(&deferred);
                firstrun::maybe_enable(&deferred);
            });

            // Clipboard capture and the retention sweep, off the startup path.
            // A blocklist that will not open is fatal to capture for the same
            // reason `clips.db` is: without it there is no second exclusion
            // mechanism (ADR-0006).
            match (clip_store, blocklist) {
                (Some(store), Some(blocklist)) => {
                    let sweeper = store.clone();
                    let prefs = prefs.clone();
                    std::thread::spawn(move || loop {
                        let retention = stored_retention(&prefs);
                        let gone = sweeper.sweep(retention);
                        if gone > 0 {
                            eprintln!("[takyon] retention swept {gone} clips");
                        }
                        std::thread::sleep(SWEEP_EVERY);
                    });
                    // The same `Arc` the settings window edits, so an added
                    // executable is excluded from the next capture, not the next
                    // launch.
                    clips::watch::spawn(store, blocklist);
                }
                _ => eprintln!("[takyon] clipboard capture is off"),
            }

            // The file index, on its own thread and after everything above.
            //
            // **Never re-walk at startup** (§5): a mapped index serves at once and
            // only a missing one costs a walk. Watching starts either way, so a
            // mapped index is current from the first second.
            let file_walk = file_index.clone();
            std::thread::spawn(move || {
                if !file_walk.is_loaded() {
                    if let Err(e) = file_walk.rebuild() {
                        eprintln!("[takyon] the file index could not be written: {e}");
                    }
                }
                file_walk.watch();
                // A rebuild folds the overlay into the file. Checked on a timer
                // rather than per event: the threshold is about how big the delta
                // has grown, which one more event never decides.
                loop {
                    std::thread::sleep(index::live::REBUILD_CHECK_EVERY);
                    if file_walk.wants_rebuild() {
                        if let Err(e) = file_walk.rebuild() {
                            eprintln!("[takyon] the file index could not be rebuilt: {e}");
                        }
                    }
                }
            });

            // The application walk, on its own thread: `firstrun::maybe_enable` can sit
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
