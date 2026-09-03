//! The Palette window: created once at startup, hidden rather than destroyed,
//! and trimmed on hide (ADR-0003).
//!
//! Everything here serves one claim: the show path allocates nothing and creates
//! nothing. WebView2 initialisation costs hundreds of milliseconds to seconds, so
//! a window created on the hotkey loses the only race this product is trying to
//! win. What is left on the hot path is a move, a show, and an event.

use serde::Serialize;
use std::sync::Mutex;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::bench::Bench;

/// The Palette's window label. Also the key its capability file is scoped to.
pub const PALETTE: &str = "palette";

/// Empty Palette height, logical pixels. `tauri.conf.json` must agree.
///
/// Not cosmetic: the window is transparent and undecorated with `shadow: true`,
/// and Windows draws that shadow around the whole window rect. Too tall means a
/// shadowed empty box hanging below the input row.
pub const EMPTY_HEIGHT: u32 = 68;

/// One Entry row, logical pixels. Must match `ROW_HEIGHT` in
/// `packages/shared/src/ipc.ts`; a test below checks it does.
pub const ROW_HEIGHT: u32 = 44;

/// Rows visible before the list scrolls instead of the window growing (TBC-0006).
///
/// §3 still ranks twelve and all twelve stay arrow-reachable. Eight caps the
/// window at a shape the eye can anchor to; twelve would swing its height by
/// nearly 600px across one query.
pub const MAX_VISIBLE_ROWS: u32 = 8;

/// Space the list adds beyond its rows: 8px padding + a 1px hairline.
///
/// The border counts. Border-box with an explicit height puts padding and border
/// *inside* it, so reserving only the padding grows a scrollbar on a list that
/// fits. Seen in the real window on a six-row list.
const LIST_CHROME: u32 = 9;

/// The "Calculator" caption above the card, gap included (v0.4.5). Must match
/// `CALC_CAPTION_HEIGHT` in `packages/shared/src/ipc.ts`.
const CALC_CAPTION_HEIGHT: u32 = 22;

/// The calculator card: expression, arrow, result, a label under each.
///
/// The one Entry whose height is not [`ROW_HEIGHT`], so it is the one case this
/// arithmetic cannot get from a row count alone. Must match `CALC_CARD_HEIGHT`
/// in `packages/shared/src/ipc.ts`; the test below checks it does.
const CALC_CARD_HEIGHT: u32 = 116;

/// The footer strip naming what Enter does (v0.4.5 task 4).
///
/// Present exactly when the list is, so it is added in the same branch. Must
/// match `FOOTER_HEIGHT` in `packages/shared/src/ipc.ts`.
const FOOTER_HEIGHT: u32 = 34;

/// One row of the `Ctrl+K` menu, and the chrome around its list.
///
/// **Measured from the rendered menu, not chosen.** A Playwright test measures the
/// real menu and asserts these still describe it — they are CSS-derived numbers
/// living in Rust, and nothing else would notice them going stale.
const ACTION_ROW_HEIGHT: u32 = 33;
const MENU_CHROME: u32 = 51;
/// The Palette's own 8px padding, top and bottom, which the menu sits inside.
const MENU_MARGIN: u32 = 16;

/// The banner's 8px top margin, which its measured box excludes.
///
/// Its height is **reported by the frontend**, never reserved here: wrapping text,
/// so the layout engine decides. A constant measured at 100% was 16px short at
/// 150% and the flex column clipped the list's last row.
const BANNER_MARGIN: u32 = 8;

/// A full-window surface the Palette navigates *into*, replacing the list.
///
/// Not a second window: another WebView2 costs the login budget and a large
/// share of the 150 MB ceiling. The warm Palette grows, and `Escape` goes back.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    ClipboardHistory,
}

/// How tall a full-window View is, logical pixels.
///
/// Fixed rather than content-sized: the surface is two panes with their own
/// scrolling, so its height is a layout choice rather than a row count. Must
/// match `VIEW_HEIGHT` in `packages/shared/src/ipc.ts`.
pub const VIEW_HEIGHT: u32 = 560;

/// What the Palette has to accommodate.
///
/// Held here because its parts arrive separately: rows with each query, the menu
/// as its own event. Without somewhere to remember the other half, every caller
/// would re-send both and any disagreement is a wrong-sized window.
#[derive(Clone, Copy, Default)]
pub struct Shape {
    pub rows: usize,
    pub indexing: bool,
    /// True when the top Entry is a calculation, which is drawn as a card rather
    /// than a row and is therefore not [`ROW_HEIGHT`] tall.
    pub calc_card: bool,
    /// `Some(n)` while the action menu is open, holding `n` actions.
    pub menu_actions: Option<usize>,
    /// Measured height of the hotkey-failure banner, 0 when absent.
    ///
    /// Rust owns `HotkeyState` so it knows *whether* there is a banner, but only
    /// the renderer knows how the sentence wrapped — and height is what clips.
    pub banner_height: u32,
    /// `Some` while a full-window View is open, which overrides every row-based
    /// measurement below it.
    pub view: Option<View>,
}

static SHAPE: Mutex<Shape> = Mutex::new(Shape {
    rows: 0,
    indexing: false,
    calc_card: false,
    menu_actions: None,
    banner_height: 0,
    view: None,
});

/// How tall the Palette should be for a given shape.
///
/// Pure, so window arithmetic is checkable without a window. Mirrors
/// `paletteHeight` in `packages/shared/src/ipc.ts`; a test asserts the constants
/// agree.
pub fn content_height(shape: Shape) -> u32 {
    // A View owns the whole window, so nothing below applies: no rows, no card,
    // no footer arithmetic. The banner is the one exception, as everywhere else
    // — it sits *below* the content rather than over it, so it always adds.
    if shape.view.is_some() {
        return VIEW_HEIGHT + banner(shape);
    }
    // The card replaces a row rather than joining it, and the cap applies to what
    // is left: eight rows *plus* a card is taller than the shape TBC-0006 chose.
    let card = if shape.calc_card {
        CALC_CAPTION_HEIGHT + CALC_CARD_HEIGHT
    } else {
        0
    };
    // The indexing notice occupies exactly one row, so the window does not jump
    // when the walk finishes and real Entries replace it.
    let list_rows = if shape.rows == 0 && shape.indexing {
        1
    } else {
        (shape.rows as u32)
            .saturating_sub(u32::from(shape.calc_card))
            .min(MAX_VISIBLE_ROWS)
    };
    let content = if card == 0 && list_rows == 0 {
        EMPTY_HEIGHT
    } else {
        EMPTY_HEIGHT + card + list_rows * ROW_HEIGHT + LIST_CHROME + FOOTER_HEIGHT
    };

    let with_menu = match shape.menu_actions {
        // `max`, never a sum: the menu sits on top of the list rather than below
        // it, so a tall list already has the room and only a short one has to grow.
        Some(actions) => content.max(MENU_CHROME + actions as u32 * ACTION_ROW_HEIGHT + MENU_MARGIN),
        None => content,
    };

    // The banner, by contrast, *is* a sum: it sits below everything else rather
    // than over it, so it always needs its own space.
    with_menu + banner(shape)
}

/// Extra height the hotkey-failure banner needs, or 0 when there is none.
fn banner(shape: Shape) -> u32 {
    if shape.banner_height > 0 {
        shape.banner_height + BANNER_MARGIN
    } else {
        0
    }
}

/// Record the measured banner height and resize. Zero means no banner.
pub fn set_banner(app: &AppHandle, height: u32) {
    let shape = {
        let mut guard = SHAPE.lock().unwrap_or_else(|e| e.into_inner());
        if guard.banner_height == height {
            return;
        }
        guard.banner_height = height;
        *guard
    };
    apply(app, shape);
}

/// Record a new row count and resize.
pub fn set_rows(app: &AppHandle, rows: usize, indexing: bool, calc_card: bool) {
    let shape = {
        let mut guard = SHAPE.lock().unwrap_or_else(|e| e.into_inner());
        guard.rows = rows;
        guard.indexing = indexing;
        guard.calc_card = calc_card;
        *guard
    };
    apply(app, shape);
}

/// Record the action menu opening or closing, and resize.
///
/// Four actions need ~200px against a 120px window, so without this the last two
/// are cut off. Invisible in the browser, which has no window to clip.
pub fn set_menu(app: &AppHandle, actions: Option<usize>) {
    let shape = {
        let mut guard = SHAPE.lock().unwrap_or_else(|e| e.into_inner());
        guard.menu_actions = actions;
        *guard
    };
    apply(app, shape);
}

/// Open or close a full-window View, and resize.
pub fn set_view(app: &AppHandle, view: Option<View>) {
    let shape = {
        let mut guard = SHAPE.lock().unwrap_or_else(|e| e.into_inner());
        if guard.view == view {
            return;
        }
        guard.view = view;
        // Leaving a View returns to an empty Palette rather than to the query
        // that opened it: the row count behind it is stale by now, and a window
        // sized for eight rows that has none is a shadowed empty box.
        if view.is_none() {
            guard.rows = 0;
            guard.calc_card = false;
            guard.menu_actions = None;
        }
        *guard
    };
    apply(app, shape);
}

/// Whether a full-window View is open.
pub fn current_view() -> Option<View> {
    SHAPE.lock().unwrap_or_else(|e| e.into_inner()).view
}

/// Forget the last query's shape, on hide.
///
/// ADR-0001: the Palette opens empty. A shape still describing eight rows would
/// flash an empty shadowed box on the next summon, then snap shut.
pub fn reset_shape(app: &AppHandle) {
    let shape = {
        let mut guard = SHAPE.lock().unwrap_or_else(|e| e.into_inner());
        // The banner survives: hiding does not un-take the hotkey, so it is drawn
        // again on the next show and clearing its height would clip it.
        // The View closes with the window too: reopening the Palette lands on the
        // root search, which is where ADR-0001 says every summon starts.
        *guard = Shape {
            banner_height: guard.banner_height,
            ..Shape::default()
        };
        *guard
    };
    apply(app, shape);
}

/// The interface zoom and where to open, cached out of `settings.db`.
///
/// **Atomics, not a lookup.** `apply` runs on every keystroke and `show` on every
/// summon; a SQLite read there would put disk on the 30 ms budget to answer a
/// question that changes about once a year. Written at startup and on change.
static UI_SCALE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(100);
static ON_PRIMARY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Load both from storage. Called at startup and whenever either changes.
pub fn cache_layout_prefs(prefs: &crate::prefs::Prefs) {
    use std::sync::atomic::Ordering::Relaxed;
    UI_SCALE.store(
        crate::prefs::ui_scale_percent(prefs.get(crate::prefs::UI_SIZE).as_deref()),
        Relaxed,
    );
    ON_PRIMARY.store(crate::settings::placement(prefs) == "primary", Relaxed);
}

/// Apply the stored interface size to a logical height.
///
/// The other half lives in `styles.css` as a root `zoom`. They multiply the same
/// number, so a mismatch shows as the Palette being exactly the zoom too short.
fn scaled(height: u32) -> u32 {
    let percent = UI_SCALE.load(std::sync::atomic::Ordering::Relaxed);
    (height * percent).div_ceil(100)
}

/// Resize the Palette after the interface size changed, without waiting for a
/// keystroke to reshape it.
pub fn rescale(app: &AppHandle) {
    let shape = *SHAPE.lock().unwrap_or_else(|e| e.into_inner());
    apply(app, shape);
}

/// Resize the Palette to fit `shape`.
///
/// **Snap, never tween** (TBC-0006): a tween reads as instability once height
/// changes every keystroke, and competes for the 30 ms budget's frames. Height
/// only — re-centring per keystroke would look like drift.
fn apply(app: &AppHandle, shape: Shape) {
    let Some(win) = palette(app) else { return };
    let height = scaled(content_height(shape));

    let Ok(size) = win.inner_size() else { return };
    let Ok(scale) = win.scale_factor() else { return };
    // Physical pixels, because that is what `inner_size` reports. Rounded, not
    // truncated: at 125% a logical 68 is 85 physical, and truncating would fail
    // the comparison every time and resize on every keystroke.
    let target = (height as f64 * scale).round() as u32;
    if size.height == target {
        return;
    }

    let _ = win.set_size(tauri::LogicalSize::new(
        size.width as f64 / scale,
        height as f64,
    ));
}

/// Emitted when the Palette becomes visible. Must match `EVENT_SHOW` in
/// `packages/shared/src/ipc.ts`; there is a test below that checks it does.
pub const EVENT_SHOW: &str = "takyon://show";
/// Emitted when the Palette is hidden. Must match `EVENT_HIDE` in the same file.
pub const EVENT_HIDE: &str = "takyon://hide";

/// Set to `1` to show without taking foreground, and suppress
/// dismiss-on-focus-loss.
///
/// Without it, inspecting the Palette is impossible — devtools takes focus and the
/// focus-loss rule hides what you were inspecting. An env var rather than a build
/// flag, so a release binary can be debugged in the field.
pub const NO_FOCUS_STEAL_ENV: &str = "TAKYON_NO_FOCUS_STEAL";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowPayload {
    pub show_id: u64,
    pub no_focus_steal: bool,
}

/// How long after a show a focus-loss event is ignored.
///
/// Show and `set_focus()` are not atomic — WebView2's child takes keyboard focus
/// after the outer window activates, and Tauri can deliver a `Focused(false)` in
/// between. Acting on it presented as "every second press does nothing". 300 ms
/// covers the handover without swallowing a real click-away.
const FOCUS_GRACE: std::time::Duration = std::time::Duration::from_millis(300);

/// When the Palette was last shown. Read by [`should_hide_on_focus_loss`].
static LAST_SHOWN: Mutex<Option<Instant>> = Mutex::new(None);

/// Should a `Focused(false)` event actually dismiss the Palette?
///
/// Three reasons it might not: the debug flag is on, the window is not visible
/// anyway, or the event arrived inside [`FOCUS_GRACE`] of a show.
pub fn should_hide_on_focus_loss(app: &AppHandle) -> bool {
    if no_focus_steal() {
        return false;
    }
    if !palette(app).and_then(|w| w.is_visible().ok()).unwrap_or(false) {
        return false;
    }
    let last = *LAST_SHOWN.lock().unwrap_or_else(|e| e.into_inner());
    !focus_loss_is_stray(last.map(|at| at.elapsed()))
}

/// Is this focus-loss event an artefact of a show that just happened?
///
/// Pure, and split out from [`should_hide_on_focus_loss`] because it is the only
/// part with any judgement in it — the rest is reading window state — and because
/// getting it wrong is invisible in code review and maddening in use.
fn focus_loss_is_stray(since_show: Option<std::time::Duration>) -> bool {
    matches!(since_show, Some(d) if d < FOCUS_GRACE)
}

pub fn no_focus_steal() -> bool {
    static CACHED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CACHED.get_or_init(|| {
        std::env::var(NO_FOCUS_STEAL_ENV)
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

pub fn palette(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(PALETTE)
}

/// Show the Palette on the monitor holding the cursor.
///
/// `bench.mark_show()` is called first and deliberately: the span this phase is
/// measuring starts at the moment we decided to show, not after the placement
/// arithmetic, or the number would exclude the part most likely to be slow.
pub fn show(app: &AppHandle, bench: &Bench) {
    let show_id = bench.mark_show();

    let Some(win) = palette(app) else {
        eprintln!("[takyon] the Palette window is missing; it should never be destroyed");
        return;
    };

    place_on_cursor_monitor(app, &win);

    // Stamped before `show()`, not after: the stray focus event can be delivered
    // while we are still inside these calls.
    *LAST_SHOWN.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());

    if let Err(e) = win.show() {
        eprintln!("[takyon] could not show the Palette: {e}");
        return;
    }

    if !no_focus_steal() {
        let _ = win.set_focus();
        // UIPI refuses foreground to a medium-integrity process while an elevated
        // window has it, and refuses silently. Ask the signed helper only when the
        // cheap path demonstrably failed, so a normal show never pays for the pipe.
        #[cfg(windows)]
        if !is_foreground(&win) {
            crate::uiaccess::request_foreground(&win);
        }
    }

    let _ = win.emit(
        EVENT_SHOW,
        ShowPayload {
            show_id,
            no_focus_steal: no_focus_steal(),
        },
    );
}

/// Hide the Palette. Never destroy it.
///
/// `reason` is logged in debug builds only, and earns its place: with three routes
/// out, the one useful question when the Palette vanishes is which fired. Without
/// it a window stealing foreground looks exactly like a hotkey bug.
pub fn hide(app: &AppHandle, reason: &str) {
    #[cfg(debug_assertions)]
    eprintln!("[takyon] hide <- {reason}");
    #[cfg(not(debug_assertions))]
    let _ = reason;

    let Some(win) = palette(app) else { return };
    if let Err(e) = win.hide() {
        eprintln!("[takyon] could not hide the Palette: {e}");
        return;
    }

    // Back to one input row, while hidden. The Palette always opens empty
    // (ADR-0001), so a window still sized for the last query's eight rows would
    // show an empty shadowed box on the next summon and then snap shut — resize
    // jank on the one frame the user is definitely looking at.
    reset_shape(app);
    *LAST_SHOWN.lock().unwrap_or_else(|e| e.into_inner()) = None;
    let _ = win.emit(EVENT_HIDE, ());

    #[cfg(windows)]
    trim_working_set_async();
}

/// The hotkey toggles: it opens the Palette and closes it again.
///
/// Three ways out — hotkey, Escape, clicking away — and all three must work; hard
/// to dismiss is worse than hard to summon. Visibility is read from the window,
/// never mirrored into a bool: the focus-loss handler can hide it at any moment,
/// and a stale flag makes every second press a no-op.
pub fn toggle(app: &AppHandle, bench: &Bench) {
    let visible = palette(app).and_then(|w| w.is_visible().ok()).unwrap_or(false);
    if visible {
        hide(app, "hotkey toggle");
    } else {
        show(app, bench);
    }
}

/// Place the Palette on the monitor the cursor is on, horizontally centred and
/// high on the screen rather than dead centre — that is where the eye already is
/// when someone reaches for a launcher, and it leaves room for Entries to grow
/// downward (TBC-0006) without the window having to move.
fn place_on_cursor_monitor(app: &AppHandle, win: &WebviewWindow) {
    let primary = ON_PRIMARY.load(std::sync::atomic::Ordering::Relaxed);

    let monitor = if primary {
        match app.primary_monitor() {
            Ok(Some(m)) => m,
            _ => return,
        }
    } else {
        let Ok(cursor) = app.cursor_position() else { return };
        match app.monitor_from_point(cursor.x, cursor.y) {
            Ok(Some(m)) => m,
            // No monitor for that point is possible mid-hotplug. Leaving the
            // window where it is beats moving it somewhere arbitrary.
            _ => return,
        }
    };

    let centre = |win: &WebviewWindow| -> Option<tauri::PhysicalPosition<i32>> {
        let size = win.outer_size().ok()?;
        let mpos = monitor.position();
        let msize = monitor.size();
        // i64 throughout: the window can legitimately be wider than a small
        // monitor, and u32 subtraction would wrap into a position off in space.
        let x = mpos.x as i64 + ((msize.width as i64 - size.width as i64) / 2);
        let y = mpos.y as i64 + (msize.height as i64 * 22 / 100);
        Some(tauri::PhysicalPosition::new(x as i32, y as i32))
    };

    if let Some(p) = centre(win) {
        let _ = win.set_position(p);
    }

    // Second pass, and it is not redundant. Moving between monitors with different
    // scale factors resizes the window in physical pixels, so the centring computed
    // against the old size is wrong by half the difference. Re-centre against the
    // size it actually ended up with.
    if let Some(p) = centre(win) {
        let _ = win.set_position(p);
    }
}

#[cfg(windows)]
fn is_foreground(win: &WebviewWindow) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let Ok(hwnd) = win.hwnd() else { return false };
    unsafe { GetForegroundWindow() == hwnd }
}

/// Release the working set of this process **and every process below it**.
///
/// Trimming only ours is pointless: the memory ADR-0003 trades away lives in
/// WebView2's browser, renderer and GPU processes, which are descendants rather
/// than children. A *hint* only — Windows may refuse, and the pages return as
/// soft faults on the next show. TBC-0002 budgets 5-15 ms for that.
#[cfg(windows)]
fn trim_working_set_async() {
    // Off the hide path on purpose. Enumerating the process table costs a couple
    // of milliseconds, and hide must feel instant even though nobody is looking at
    // the window any more: the next keystroke can arrive immediately.
    std::thread::spawn(|| {
        for pid in process_tree(std::process::id()) {
            trim_pid(pid);
        }
    });
}

#[cfg(windows)]
fn trim_pid(pid: u32) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, SetProcessWorkingSetSize, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA,
    };

    unsafe {
        let Ok(handle) = OpenProcess(
            PROCESS_SET_QUOTA | PROCESS_QUERY_LIMITED_INFORMATION,
            false,
            pid,
        ) else {
            // A WebView2 child can exit between the snapshot and here. Routine.
            return;
        };
        // (SIZE_T)-1 for both bounds is the documented "trim as much as you can"
        // request. Any other pair would be setting a real quota.
        let _ = SetProcessWorkingSetSize(handle, usize::MAX, usize::MAX);
        let _ = CloseHandle(handle);
    }
}

/// `root` plus every descendant, from one snapshot of the process table.
///
/// One snapshot rather than one per level: the table is a consistent moment, and
/// re-snapshotting per level would let a process appear in two generations.
#[cfg(windows)]
fn process_tree(root: u32) -> Vec<u32> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    let mut pairs: Vec<(u32, u32)> = Vec::new(); // (pid, parent)
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return vec![root];
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                pairs.push((entry.th32ProcessID, entry.th32ParentProcessID));
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }

    collect_descendants(root, &pairs)
}

/// Pure, so the tree walk is testable without a process table.
///
/// Windows recycles pids, so a table can hold a cycle. Walking that naively spins
/// a thread forever after every dismissal — invisible until the machine gets
/// warm. Hence the visited set.
#[cfg(windows)]
fn collect_descendants(root: u32, pairs: &[(u32, u32)]) -> Vec<u32> {
    let mut out = vec![root];
    let mut seen = std::collections::HashSet::from([root]);
    let mut frontier = vec![root];

    while let Some(parent) = frontier.pop() {
        for (pid, ppid) in pairs {
            if *ppid == parent && seen.insert(*pid) {
                out.push(*pid);
                frontier.push(*pid);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The event names cross a language boundary with no compiler between them.
    /// This is the cheapest possible contract test, and it is the exact drift
    /// TBC-0007 warns is silent: a rename on one side leaves the Palette waiting
    /// for an event that is never sent, and it looks like a performance problem.
    #[test]
    fn v0_1_event_names_match_the_typescript_contract() {
        let ipc = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../packages/shared/src/ipc.ts"),
        )
        .expect("packages/shared/src/ipc.ts");

        assert!(
            ipc.contains(&format!("EVENT_SHOW = \"{EVENT_SHOW}\"")),
            "EVENT_SHOW disagrees with packages/shared/src/ipc.ts"
        );
        assert!(
            ipc.contains(&format!("EVENT_HIDE = \"{EVENT_HIDE}\"")),
            "EVENT_HIDE disagrees with packages/shared/src/ipc.ts"
        );
    }

    /// The window label is what `capabilities/default.json` is scoped to. If they
    /// disagree the Palette silently has no permissions at runtime.
    #[test]
    fn v0_1_the_capability_is_scoped_to_the_palette_label() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

        let conf: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("tauri.conf.json")).unwrap())
                .unwrap();
        assert_eq!(conf["app"]["windows"][0]["label"].as_str(), Some(PALETTE));
        assert_eq!(
            conf["app"]["windows"][0]["visible"].as_bool(),
            Some(false),
            "ADR-0003: the Palette is created hidden, then never destroyed"
        );
        assert_eq!(
            conf["app"]["windows"][0]["height"].as_u64(),
            Some(EMPTY_HEIGHT as u64),
            "a window taller than its content draws its shadow around empty space"
        );

        let cap: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("capabilities/default.json")).unwrap(),
        )
        .unwrap();
        assert!(cap["windows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str() == Some(PALETTE)));
    }

    /// The bug this exists for: WebView2 hands focus from the outer window to its
    /// own child immediately after a show, and Tauri delivers a `Focused(false)`
    /// in between. Acting on it hid the Palette microseconds after summoning it,
    /// which presented as the hotkey working only every second press.
    #[test]
    fn v0_1_a_focus_loss_during_the_show_handover_is_ignored() {
        assert!(focus_loss_is_stray(Some(std::time::Duration::from_millis(0))));
        assert!(focus_loss_is_stray(Some(std::time::Duration::from_millis(299))));
    }

    /// A real click-away must never be swallowed. Clicking away is one of only
    /// three ways out of the Palette, so a grace period that ate genuine events
    /// would trade a flaky hotkey for a Palette you cannot dismiss.
    #[test]
    fn v0_1_a_real_click_away_still_dismisses() {
        assert!(!focus_loss_is_stray(Some(std::time::Duration::from_millis(301))));
        assert!(!focus_loss_is_stray(Some(std::time::Duration::from_secs(30))));
        // No recorded show at all: nothing to be an artefact of.
        assert!(!focus_loss_is_stray(None));
    }

    /// TBC-0006: sizes to content, stops growing at eight rows.
    fn shape(rows: usize, indexing: bool, menu: Option<usize>) -> Shape {
        Shape {
            rows,
            indexing,
            calc_card: false,
            menu_actions: menu,
            banner_height: 0,
            view: None,
        }
    }

    /// The same, with the top Entry drawn as a calculator card (v0.4.5).
    fn calc_shape(rows: usize) -> Shape {
        Shape {
            calc_card: true,
            ..shape(rows, false, None)
        }
    }

    /// v0.4.5: a calculation is a card, so it is not [`ROW_HEIGHT`] tall.
    ///
    /// The only thing that catches a wrong card height. Playwright renders in a
    /// browser with no native window, so an overflowing card looks correct there
    /// and clips against a transparent, undecorated window in the product.
    #[test]
    fn v0_4_5_a_calculation_is_sized_as_a_card_rather_than_a_row() {
        let alone = content_height(calc_shape(1));
        assert_eq!(
            alone,
            EMPTY_HEIGHT + CALC_CAPTION_HEIGHT + CALC_CARD_HEIGHT + LIST_CHROME + FOOTER_HEIGHT
        );
        // Taller than the row it replaced, or the card is not a card.
        assert!(alone > content_height(shape(1, false, None)));
    }

    /// The card replaces a row rather than joining it. A calculation plus one app
    /// is one card and one row, never one card and two.
    #[test]
    fn v0_4_5_the_card_replaces_a_row_it_does_not_add_one() {
        assert_eq!(
            content_height(calc_shape(2)) - content_height(calc_shape(1)),
            ROW_HEIGHT
        );
    }

    /// `MAX_VISIBLE_ROWS` counts rows, and the card is not one. Eight rows *plus*
    /// a card would be taller than the shape TBC-0006 settled on, so the cap
    /// applies to what is left after the card.
    #[test]
    fn v0_4_5_the_row_cap_applies_to_what_is_left_after_the_card() {
        let capped = content_height(calc_shape(MAX_VISIBLE_ROWS as usize + 1));
        assert_eq!(capped, content_height(calc_shape(999)));
        assert_eq!(
            capped,
            EMPTY_HEIGHT
                + CALC_CAPTION_HEIGHT
                + CALC_CARD_HEIGHT
                + MAX_VISIBLE_ROWS * ROW_HEIGHT
                + LIST_CHROME
                + FOOTER_HEIGHT
        );
    }

    #[test]
    fn v0_2_the_window_grows_with_its_rows_and_then_stops() {
        assert_eq!(content_height(shape(0, false, None)), EMPTY_HEIGHT);
        assert_eq!(
            content_height(shape(1, false, None)),
            EMPTY_HEIGHT + ROW_HEIGHT + LIST_CHROME + FOOTER_HEIGHT
        );
        assert_eq!(
            content_height(shape(8, false, None)),
            EMPTY_HEIGHT + 8 * ROW_HEIGHT + LIST_CHROME + FOOTER_HEIGHT
        );
        // §3 ranks twelve. The extra four scroll inside the list rather than
        // pushing the window another 176 pixels down the screen.
        assert_eq!(
            content_height(shape(12, false, None)),
            content_height(shape(8, false, None))
        );
        assert_eq!(
            content_height(shape(100, false, None)),
            content_height(shape(8, false, None))
        );
    }

    /// One row tall, so the window does not jump when the walk finishes.
    #[test]
    fn v0_2_the_indexing_notice_occupies_one_row() {
        assert_eq!(
            content_height(shape(0, true, None)),
            content_height(shape(1, false, None))
        );
        // Once there are Entries, the notice is no longer what sets the height.
        assert_eq!(
            content_height(shape(3, true, None)),
            content_height(shape(3, false, None))
        );
    }

    /// The bug: a ~200px menu against a 120px window cuts off two actions.
    /// Invisible in the browser, which has no window to clip anything.
    #[test]
    fn v0_2_opening_the_action_menu_grows_a_short_palette_to_fit_it() {
        let one_row = content_height(shape(1, false, None));
        let with_menu = content_height(shape(1, false, Some(4)));
        assert!(
            with_menu > one_row,
            "a {one_row}px window cannot show a four-action menu"
        );
        assert_eq!(with_menu, MENU_CHROME + 4 * ACTION_ROW_HEIGHT + MENU_MARGIN);
    }

    /// The menu overlays the list, so a tall Palette must not grow further —
    /// that reads as the window lurching whenever the menu opens.
    #[test]
    fn v0_2_a_tall_palette_does_not_grow_further_for_a_menu() {
        let eight_rows = content_height(shape(8, false, None));
        assert_eq!(content_height(shape(8, false, Some(4))), eight_rows);
    }

    /// Drawn below everything else, so its space adds rather than overlaps.
    /// Found by running the real binary with Raycast holding Alt+Space.
    #[test]
    fn v0_2_a_failed_hotkey_banner_gets_its_own_space() {
        let mut with_banner = shape(1, false, None);
        with_banner.banner_height = 73;
        assert_eq!(
            content_height(with_banner),
            content_height(shape(1, false, None)) + 73 + BANNER_MARGIN
        );

        // And it stacks with the menu rather than being swallowed by it.
        let mut banner_and_menu = shape(1, false, Some(4));
        banner_and_menu.banner_height = 73;
        assert_eq!(
            content_height(banner_and_menu),
            content_height(shape(1, false, Some(4))) + 73 + BANNER_MARGIN
        );
    }

    /// Whatever the renderer measured, not a number chosen here — a banner
    /// wrapping to three lines is taller than one wrapping to two, and a constant
    /// cannot know which happened.
    #[test]
    fn v0_2_the_window_follows_the_measured_banner_rather_than_a_constant() {
        let mut two_lines = shape(1, false, None);
        two_lines.banner_height = 57;
        let mut three_lines = shape(1, false, None);
        three_lines.banner_height = 73;
        assert_eq!(
            content_height(three_lines) - content_height(two_lines),
            73 - 57
        );
    }

    /// Rust sizes the window from these; the CSS draws rows with them. A
    /// disagreement clips the last row, and nothing on either side would say so.
    /// A View owns the window: no rows, no card, no footer arithmetic reaches it.
    #[test]
    fn v0_5_a_view_overrides_every_row_based_measurement() {
        let mut shape = shape(8, false, Some(4));
        shape.calc_card = true;
        let as_rows = content_height(shape);

        shape.view = Some(View::ClipboardHistory);
        assert_eq!(content_height(shape), VIEW_HEIGHT);
        assert_ne!(as_rows, VIEW_HEIGHT, "the test would prove nothing");
    }

    /// The banner is the exception it always was: it sits *below* everything
    /// rather than over it, so even a View has to make room.
    #[test]
    fn v0_5_a_view_still_leaves_room_for_the_hotkey_banner() {
        let mut shape = Shape {
            view: Some(View::ClipboardHistory),
            ..Shape::default()
        };
        assert_eq!(content_height(shape), VIEW_HEIGHT);
        shape.banner_height = 40;
        assert!(content_height(shape) > VIEW_HEIGHT);
    }

    #[test]
    fn v0_2_row_geometry_agrees_with_the_typescript_contract() {
        let ipc = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../packages/shared/src/ipc.ts"),
        )
        .expect("packages/shared/src/ipc.ts");

        for (name, value) in [
            ("ROW_HEIGHT", ROW_HEIGHT),
            ("MAX_VISIBLE_ROWS", MAX_VISIBLE_ROWS),
            ("EMPTY_HEIGHT", EMPTY_HEIGHT),
            ("LIST_CHROME", LIST_CHROME),
            ("ACTION_ROW_HEIGHT", ACTION_ROW_HEIGHT),
            ("MENU_CHROME", MENU_CHROME),
            ("MENU_MARGIN", MENU_MARGIN),
            ("BANNER_MARGIN", BANNER_MARGIN),
            ("CALC_CAPTION_HEIGHT", CALC_CAPTION_HEIGHT),
            ("CALC_CARD_HEIGHT", CALC_CARD_HEIGHT),
            ("FOOTER_HEIGHT", FOOTER_HEIGHT),
        ] {
            assert!(
                ipc.contains(&format!("{name} = {value}")),
                "{name} disagrees with packages/shared/src/ipc.ts"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn v0_1_the_process_tree_reaches_grandchildren() {
        // WebView2's renderer is a child of its browser process, not of us. A walk
        // that stopped at direct children would trim the one process that holds
        // almost none of the memory.
        let pairs = [(100, 1), (200, 100), (300, 200), (400, 999)];
        let mut tree = collect_descendants(100, &pairs);
        tree.sort_unstable();
        assert_eq!(tree, vec![100, 200, 300]);
    }

    #[cfg(windows)]
    #[test]
    fn v0_1_a_recycled_pid_cycle_does_not_hang_the_walk() {
        // Windows recycles pids, so this table is possible: 200's parent is 100,
        // and 100's parent has been recycled to 200. Without the visited set this
        // spins forever on a thread nobody is watching.
        let pairs = [(200, 100), (100, 200)];
        let mut tree = collect_descendants(100, &pairs);
        tree.sort_unstable();
        assert_eq!(tree, vec![100, 200]);
    }
}
