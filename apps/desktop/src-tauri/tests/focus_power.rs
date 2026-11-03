//! The Focus thread's power window against real Win32 (v0.16): sleep and wake
//! reach the session clock, the shutdown hold answers Windows and names the
//! session, idle sleep is held off and let go.
//!
//! Messages are sent, not broadcast: a real suspend or shutdown cannot run in
//! a test. What is left for a hand run is Windows' own screen
//! (`docs/verify/v0.16.md` §D).

#![cfg(windows)]

use std::cell::RefCell;
use std::rc::Rc;

use takyon_lib::focus::power::{keep_awake, Power, Watch};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Shutdown::ShutdownBlockReasonQuery;
use windows::Win32::UI::WindowsAndMessaging::{
    SendMessageW, PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND, WM_POWERBROADCAST, WM_QUERYENDSESSION,
};

fn send(hwnd: isize, msg: u32, wp: usize) -> isize {
    unsafe { SendMessageW(HWND(hwnd as *mut _), msg, Some(WPARAM(wp)), Some(LPARAM(0))).0 }
}

fn reason(hwnd: isize) -> Option<String> {
    let mut buf = [0u16; 256];
    let mut len = buf.len() as u32;
    unsafe {
        ShutdownBlockReasonQuery(
            HWND(hwnd as *mut _),
            Some(windows::core::PWSTR(buf.as_mut_ptr())),
            &mut len,
        )
        .ok()?;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(0);
    Some(String::from_utf16_lossy(&buf[..end]))
}

#[test]
fn v0_16_suspend_and_resume_reach_the_session_clock() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    let watch = Watch::new(move |p| log.borrow_mut().push(p)).expect("hidden window");
    assert_eq!(
        send(watch.hwnd(), WM_POWERBROADCAST, PBT_APMSUSPEND as usize),
        1
    );
    send(
        watch.hwnd(),
        WM_POWERBROADCAST,
        PBT_APMRESUMEAUTOMATIC as usize,
    );
    assert_eq!(*seen.borrow(), [Power::Sleep, Power::Wake]);
}

/// Held: the query is refused and Windows has the reason to show. Released:
/// shutdown goes through and the reason is gone.
#[test]
fn v0_16_the_shutdown_hold_refuses_the_query_and_names_the_session() {
    let watch = Watch::new(|_| {}).expect("hidden window");
    assert_ne!(send(watch.hwnd(), WM_QUERYENDSESSION, 0), 0);

    watch.hold_shutdown(Some("Focus: Write report, 25 min left"));
    assert_eq!(send(watch.hwnd(), WM_QUERYENDSESSION, 0), 0);
    assert_eq!(
        reason(watch.hwnd()).as_deref(),
        Some("Focus: Write report, 25 min left")
    );

    watch.hold_shutdown(None);
    assert_ne!(send(watch.hwnd(), WM_QUERYENDSESSION, 0), 0);
    assert_eq!(reason(watch.hwnd()), None);
}

/// Read back through the previous state each call returns: the machine-wide
/// sum includes every other app's request (a video, a download), so it
/// cannot tell this thread's hold from theirs.
#[test]
fn v0_16_idle_sleep_is_held_off_and_let_go() {
    keep_awake(false);
    assert!(!keep_awake(true), "nothing held before");
    assert!(keep_awake(true), "held after asking");
    assert!(keep_awake(false), "still held until released");
    assert!(!keep_awake(false), "released");
}
