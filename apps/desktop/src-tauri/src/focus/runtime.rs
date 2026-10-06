//! The enforcer: one thread that quits blocked apps, redirects blocked
//! sites, and holds the hurdles while a Focus session runs.
//!
//! Polls rather than hooks: foreground every 250 ms, the process table every
//! 2 s. A hidden top-level window (message-only ones miss broadcasts) takes
//! `WM_POWERBROADCAST` for sleep and `WM_QUERYENDSESSION` for the shutdown
//! hurdle. Decisions live in `rules`; this file is only the OS glue.

use std::sync::Arc;

use serde::Serialize;

use super::service::View;
use super::store::Outcome;

/// What the runtime tells the frontend.
#[derive(Clone, Debug, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum Signal {
    /// The session changed; redraw.
    View {
        view: View,
    },
    /// Something was blocked. `snoozable` drives the bar's Snooze button.
    Blocked {
        key: String,
        label: String,
        snoozable: bool,
    },
    Ended {
        outcome: Outcome,
    },
}

pub type Emit = Arc<dyn Fn(Signal) + Send + Sync>;

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Original URL for a block page, from what the address bar showed. Chromium
/// hides the scheme; Zen shows the host only, so its path is lost.
pub fn original_url(display: &str) -> String {
    let d = display.trim();
    let lower = d.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        d.to_string()
    } else {
        format!("https://{d}")
    }
}

#[cfg(windows)]
pub use imp::spawn;

#[cfg(not(windows))]
pub fn spawn(
    _: Arc<super::service::Focus>,
    _: Arc<crate::prefs::Prefs>,
    _: Arc<std::sync::Mutex<Option<std::path::PathBuf>>>,
    _: Emit,
) {
}

#[cfg(windows)]
mod imp {
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::Duration;

    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Power::{
        SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
    };
    use windows::Win32::System::Shutdown::{ShutdownBlockReasonCreate, ShutdownBlockReasonDestroy};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow,
        GetWindowThreadProcessId, MsgWaitForMultipleObjects, PeekMessageW, RegisterClassW,
        TranslateMessage, MSG, PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND, PM_REMOVE, QS_ALLINPUT,
        WINDOW_EX_STYLE, WM_POWERBROADCAST, WM_QUERYENDSESSION, WNDCLASSW, WS_OVERLAPPED,
    };

    use super::{now_ms, original_url, Emit, Signal};
    use crate::entry::LaunchTarget;
    use crate::focus::browser::{engine_of, mta, Bar, Uia};
    use crate::focus::identity::Process;
    use crate::focus::procs::{self, Quit};
    use crate::focus::service::Focus;
    use crate::focus::settings::{snooze_ms, Hurdle};
    use crate::focus::site::block_url;
    use crate::focus::store::EventKind;
    use crate::prefs::{self, Prefs};

    /// Grace before terminating an app asked to close.
    const GRACE: Duration = Duration::from_secs(2);
    const SCAN_EVERY_MS: u64 = 2_000;
    /// One notice per target per this long, or a relaunch loop floods the bar.
    const NOTICE_EVERY_MS: u64 = 5_000;
    /// Tabs left on the block page get restored on visit for this long.
    const RESTORE_WINDOW_MS: u64 = 30 * 60_000;

    static FOCUS: OnceLock<Arc<Focus>> = OnceLock::new();
    /// Shutdown hurdle armed; read by the window procedure.
    static HOLD_SHUTDOWN: AtomicBool = AtomicBool::new(false);

    unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        match msg {
            WM_POWERBROADCAST => {
                if let Some(focus) = FOCUS.get() {
                    match wp.0 as u32 {
                        PBT_APMSUSPEND => focus.sleep(now_ms()),
                        PBT_APMRESUMEAUTOMATIC => focus.wake(now_ms()),
                        _ => {}
                    }
                }
                LRESULT(1)
            }
            // FALSE holds the shutdown behind Windows' "preventing shutdown"
            // screen, whose Shut down anyway always wins (ADR-0036).
            WM_QUERYENDSESSION if HOLD_SHUTDOWN.load(Ordering::Relaxed) => LRESULT(0),
            _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
        }
    }

    fn hidden_window() -> Option<HWND> {
        unsafe {
            let instance = GetModuleHandleW(PCWSTR::null()).ok()?;
            let class = w!("TakyonFocus");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&wc);
            // Top-level and never shown: message-only windows get no broadcasts.
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                w!("Takyon Focus"),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .ok()
        }
    }

    /// Start the enforcer. `page`: the themed block page for the running session.
    pub fn spawn(
        focus: Arc<Focus>,
        prefs: Arc<Prefs>,
        page: Arc<Mutex<Option<PathBuf>>>,
        emit: Emit,
    ) {
        let _ = FOCUS.set(focus.clone());
        std::thread::Builder::new()
            .name("focus".into())
            .spawn(move || run(focus, prefs, page, emit))
            .ok();
    }

    #[derive(Default)]
    struct Loop {
        last_scan: u64,
        last_front: isize,
        quitting: Arc<Mutex<HashSet<u32>>>,
        noticed: HashMap<String, u64>,
        nav_cooldown: HashMap<isize, u64>,
        restore_until: u64,
        restored: HashSet<isize>,
        holding: bool,
        last_view: Option<crate::focus::service::View>,
    }

    fn run(focus: Arc<Focus>, prefs: Arc<Prefs>, page: Arc<Mutex<Option<PathBuf>>>, emit: Emit) {
        let _com = mta();
        let uia = Uia::new();
        let window = hidden_window();
        let mut st = Loop::default();
        loop {
            pump(Duration::from_millis(250));
            let now = now_ms();
            if let Some(outcome) = focus.tick(now) {
                ended(&focus, &prefs, &emit, &mut st, window, now);
                emit(Signal::Ended { outcome });
            }
            let view = focus.view(now);
            // On change only: the frontend counts down from `endsAt` itself, so
            // `remaining_ms` ticking is not a change.
            let shape = crate::focus::service::View {
                remaining_ms: 0,
                ..view.clone()
            };
            if st.last_view.as_ref() != Some(&shape) {
                emit(Signal::View { view: view.clone() });
                st.last_view = Some(shape);
            }
            if !view.running {
                if st.holding {
                    release(window, &mut st);
                }
                restore_tabs(&uia, &mut st, now);
                continue;
            }
            hurdles(&focus, &prefs, window, &mut st, &view);
            if view.paused {
                continue;
            }
            let front = unsafe { GetForegroundWindow() }.0 as isize;
            let front_pid = pid_of(front);
            if now.saturating_sub(st.last_scan) >= SCAN_EVERY_MS || front != st.last_front {
                st.last_scan = now;
                st.last_front = front;
                quit_blocked(&focus, &prefs, &emit, &mut st, front_pid, now);
            }
            block_site(
                &focus, &prefs, &emit, &uia, &page, &mut st, front, front_pid, now,
            );
        }
    }

    fn pump(timeout: Duration) {
        unsafe {
            MsgWaitForMultipleObjects(None, false, timeout.as_millis() as u32, QS_ALLINPUT);
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    fn pid_of(hwnd: isize) -> Option<u32> {
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid)) };
        (pid != 0).then_some(pid)
    }

    fn label_of(key: &str, process: Option<&Process>) -> String {
        if let Some(domain) = key.strip_prefix("site:") {
            return domain.to_string();
        }
        process
            .and_then(|p| p.image.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| key.rsplit([':', '\\']).next().unwrap_or(key).to_string())
    }

    fn notice(
        focus: &Focus,
        prefs: &Prefs,
        emit: &Emit,
        st: &mut Loop,
        key: &str,
        label: String,
        now: u64,
    ) {
        if st
            .noticed
            .get(key)
            .is_some_and(|&t| now.saturating_sub(t) < NOTICE_EVERY_MS)
        {
            return;
        }
        st.noticed.insert(key.to_string(), now);
        let view_strict = focus.view(now).strict;
        let snoozable =
            !view_strict && snooze_ms(prefs.get(prefs::FOCUS_SNOOZE).as_deref()).is_some();
        focus.with_active(|a| {
            focus
                .store()
                .record(a.session.id, now, EventKind::Blocked, Some(key))
                .ok()
        });
        emit(Signal::Blocked {
            key: key.to_string(),
            label,
            snoozable,
        });
    }

    fn quit_blocked(
        focus: &Arc<Focus>,
        prefs: &Prefs,
        emit: &Emit,
        st: &mut Loop,
        front: Option<u32>,
        now: u64,
    ) {
        let procs = procs::snapshot();
        let takyon_dir = takyon_dir();
        let doomed = focus
            .with_active(|a| {
                a.rules
                    .to_quit(&procs, &mut a.snoozes, front, now, &takyon_dir)
            })
            .unwrap_or_default();
        for (pid, key) in doomed {
            if !st
                .quitting
                .lock()
                .map(|mut q| q.insert(pid))
                .unwrap_or(false)
            {
                continue;
            }
            let process = procs
                .iter()
                .find(|r| r.pid == pid)
                .map(|r| r.process.clone());
            notice(
                focus,
                prefs,
                emit,
                st,
                &key,
                label_of(&key, process.as_ref()),
                now,
            );
            let (focus, quitting) = (focus.clone(), st.quitting.clone());
            std::thread::spawn(move || {
                let result = procs::quit(pid, GRACE);
                if matches!(result, Quit::Closed | Quit::Terminated) {
                    if let Some(p) = &process {
                        focus.note_quit(p);
                    }
                }
                if result == Quit::Denied {
                    focus.with_active(|a| {
                        focus
                            .store()
                            .record(a.session.id, now_ms(), EventKind::Refused, Some(&key))
                            .ok()
                    });
                }
                if let Ok(mut q) = quitting.lock() {
                    q.remove(&pid);
                }
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn block_site(
        focus: &Focus,
        prefs: &Prefs,
        emit: &Emit,
        uia: &Option<Uia>,
        page: &Mutex<Option<PathBuf>>,
        st: &mut Loop,
        front: isize,
        front_pid: Option<u32>,
        now: u64,
    ) {
        let Some(uia) = uia else { return };
        if st.nav_cooldown.get(&front).is_some_and(|&t| now < t) {
            return;
        }
        let Some(engine) = front_pid
            .and_then(procs::describe)
            .and_then(|p| engine_of(&p.image))
        else {
            return;
        };
        let Bar::Url(display) = uia.read(front, engine) else {
            return;
        };
        let Some(key) = focus
            .with_active(|a| a.rules.site_block(&display, &mut a.snoozes, now))
            .flatten()
        else {
            return;
        };
        let Some(page) = page.lock().ok().and_then(|p| p.clone()) else {
            return;
        };
        let view = focus.view(now);
        let url = block_url(&page, &original_url(&display), view.ends_at, &view.goal);
        if uia.navigate(front, engine, &url) {
            st.nav_cooldown.insert(front, now + 1_500);
            notice(focus, prefs, emit, st, &key, label_of(&key, None), now);
        }
    }

    /// Lazy restore: a browser left holding block pages gets them back the
    /// next time the user brings it to the front. Never raises a window.
    fn restore_tabs(uia: &Option<Uia>, st: &mut Loop, now: u64) {
        let Some(uia) = uia else { return };
        if now >= st.restore_until {
            return;
        }
        let front = unsafe { GetForegroundWindow() }.0 as isize;
        if st.restored.contains(&front) {
            return;
        }
        let Some(engine) = pid_of(front)
            .and_then(procs::describe)
            .and_then(|p| engine_of(&p.image))
        else {
            return;
        };
        st.restored.insert(front);
        uia.restore(front, engine);
    }

    fn hurdles(
        focus: &Focus,
        prefs: &Prefs,
        window: Option<HWND>,
        st: &mut Loop,
        view: &crate::focus::service::View,
    ) {
        let strictness = focus.with_active(|a| a.session.strictness);
        let Some(strictness) = strictness else { return };
        let shutdown =
            Hurdle::parse(prefs.get(prefs::FOCUS_HURDLE_SHUTDOWN).as_deref()).applies(strictness);
        let idle =
            Hurdle::parse(prefs.get(prefs::FOCUS_HURDLE_IDLE).as_deref()).applies(strictness);
        HOLD_SHUTDOWN.store(shutdown, Ordering::Relaxed);
        if let (true, Some(w)) = (shutdown, window) {
            let minutes = view.remaining_ms.div_ceil(60_000);
            let reason = HSTRING::from(format!("Focus: {}, {minutes} min left", view.goal));
            unsafe {
                let _ = ShutdownBlockReasonCreate(w, &reason);
            }
        }
        if idle {
            unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
        }
        st.holding = true;
    }

    fn release(window: Option<HWND>, st: &mut Loop) {
        HOLD_SHUTDOWN.store(false, Ordering::Relaxed);
        if let Some(w) = window {
            unsafe {
                let _ = ShutdownBlockReasonDestroy(w);
            }
        }
        unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
        st.holding = false;
    }

    fn ended(
        focus: &Focus,
        prefs: &Prefs,
        emit: &Emit,
        st: &mut Loop,
        window: Option<HWND>,
        now: u64,
    ) {
        let _ = emit;
        release(window, st);
        st.restore_until = now + RESTORE_WINDOW_MS;
        st.restored.clear();
        let quit = focus.take_quit();
        if prefs::flag(prefs, prefs::FOCUS_RESTORE_APPS, false) {
            for process in quit {
                let target = match process.aumid {
                    Some(aumid) => LaunchTarget::Aumid(aumid),
                    None => LaunchTarget::Exe {
                        path: process.image,
                        args: None,
                        working_dir: None,
                    },
                };
                let _ = crate::launch::open(&target);
            }
        }
    }

    fn takyon_dir() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_16_the_original_url_gets_a_scheme_when_the_bar_hid_it() {
        assert_eq!(
            original_url("youtube.com/watch?v=1"),
            "https://youtube.com/watch?v=1"
        );
        assert_eq!(
            original_url("https://youtube.com/x"),
            "https://youtube.com/x"
        );
        assert_eq!(original_url("http://intranet/x"), "http://intranet/x");
        assert_eq!(original_url(" example.com "), "https://example.com");
    }
}
