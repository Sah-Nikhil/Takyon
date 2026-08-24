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

/// The empty Palette's height in logical pixels: an 8px gutter, a 1px border, a
/// 48px input row, and the same again below. `tauri.conf.json` must agree.
///
/// This is not cosmetic. The window is transparent and undecorated with
/// `shadow: true`, and Windows draws that shadow around the whole **window rect**,
/// not around the painted content. A window taller than its content therefore
/// shows a large empty box outlined in shadow hanging below the input row — which
/// is what 160px looked like on screen. TBC-0006 says the Palette sizes itself to
/// its content; until Entries exist to grow it, that means starting here.
pub const EMPTY_HEIGHT: u32 = 68;

/// Emitted when the Palette becomes visible. Must match `EVENT_SHOW` in
/// `packages/shared/src/ipc.ts`; there is a test below that checks it does.
pub const EVENT_SHOW: &str = "takyon://show";
/// Emitted when the Palette is hidden. Must match `EVENT_HIDE` in the same file.
pub const EVENT_HIDE: &str = "takyon://hide";

/// Set to `1` to show the Palette without taking foreground, and to suppress
/// dismiss-on-focus-loss.
///
/// Without this, inspecting the Palette is impossible: the moment devtools takes
/// focus, the focus-loss rule hides the thing you were inspecting. Deliberately an
/// environment variable rather than a build flag, so a release binary can be
/// debugged in the field without a rebuild.
pub const NO_FOCUS_STEAL_ENV: &str = "TAKYON_NO_FOCUS_STEAL";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowPayload {
    pub show_id: u64,
    pub no_focus_steal: bool,
}

/// How long after a show a focus-loss event is ignored.
///
/// Showing the window and calling `set_focus()` is not one atomic act. The outer
/// window is activated, then WebView2's own child window takes keyboard focus, and
/// in between Tauri can deliver a `Focused(false)` for the outer window. Acting on
/// that hides the Palette microseconds after summoning it — which presented as
/// "every second press of the hotkey does nothing", because the press *did* show
/// it and the stray event immediately took it away.
///
/// 300 ms is long enough to cover that handover and short enough that a real
/// click-away is never swallowed: it takes longer than that to summon a launcher
/// and change your mind.
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
/// `reason` is logged in debug builds and nowhere else. It earns its place: there
/// are three routes out (hotkey, Escape, focus loss) and when the Palette vanishes
/// unexpectedly, the only useful question is which one fired. Without it, an
/// unrelated window stealing foreground is indistinguishable from a bug in the
/// hotkey — a distinction that cost real time to make once already.
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
    *LAST_SHOWN.lock().unwrap_or_else(|e| e.into_inner()) = None;
    let _ = win.emit(EVENT_HIDE, ());

    #[cfg(windows)]
    trim_working_set_async();
}

/// The hotkey toggles: it opens the Palette, and pressing it again closes it.
///
/// So there are three ways out — the hotkey, Escape, and clicking away — and all
/// three must work, because a launcher that is hard to dismiss is worse than one
/// that is hard to summon. The click-away route is `WindowEvent::Focused(false)`
/// in `lib.rs`, and it is suppressed only under the debug no-steal-focus flag.
///
/// Visibility is read from the window rather than tracked in a flag of our own:
/// the window can be hidden by the focus-loss handler at any moment, and a
/// mirrored bool would disagree the first time that happened, making every second
/// press a no-op.
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
    let Ok(cursor) = app.cursor_position() else { return };
    let monitor = match app.monitor_from_point(cursor.x, cursor.y) {
        Ok(Some(m)) => m,
        // No monitor for that point is possible mid-hotplug. Leaving the window
        // where it is beats moving it somewhere arbitrary.
        _ => return,
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
/// Trimming only our own process would be close to pointless: the Rust host is a
/// few megabytes, and essentially all of the resident memory ADR-0003 is trading
/// away lives in WebView2's browser, renderer and GPU processes. Those are
/// descendants, not children — the renderer's parent is the browser process, not
/// us — so this walks the tree rather than one level of it.
///
/// This is a *hint*. Windows may refuse, and the pages come back on the next show
/// as soft faults, which is the 5-15 ms TBC-0002 budgets for. The one number that
/// decides whether the bet was right is the first show after a long idle, when
/// Windows has genuinely reclaimed rather than merely unmapped.
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
/// Windows recycles pids, so a table can contain a cycle (a child whose recycled
/// parent id points back into its own subtree). Walking that naively hangs, which
/// on this code path would mean a thread spinning forever after every dismissal —
/// invisible until the machine gets warm. Hence the visited set.
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
