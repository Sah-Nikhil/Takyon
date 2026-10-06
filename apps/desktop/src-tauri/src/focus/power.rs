//! The Focus thread's hidden window: sleep and wake in, the shutdown hold out.
//!
//! Top-level and never shown: message-only windows get no broadcasts, so they
//! miss `WM_POWERBROADCAST` and `WM_QUERYENDSESSION`. Windows names the hold on
//! its "preventing shutdown" screen from `ShutdownBlockReasonCreate`, which is
//! the documented way for an app with no visible window to hold a shutdown.
//! One per thread; messages reach it only while that thread pumps ([`pump`]).

/// A power transition the session clock cares about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Power {
    /// `PBT_APMSUSPEND`: the clock stops.
    Sleep,
    /// `PBT_APMRESUMEAUTOMATIC`: it runs again.
    Wake,
}

#[cfg(windows)]
pub use imp::{keep_awake, pump, Watch};

#[cfg(windows)]
mod imp {
    use std::cell::Cell;
    use std::time::Duration;

    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Power::{
        SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
    };
    use windows::Win32::System::Shutdown::{ShutdownBlockReasonCreate, ShutdownBlockReasonDestroy};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetWindowLongPtrW,
        MsgWaitForMultipleObjects, PeekMessageW, RegisterClassW, SetWindowLongPtrW,
        TranslateMessage, GWLP_USERDATA, MSG, PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND, PM_REMOVE,
        QS_ALLINPUT, WINDOW_EX_STYLE, WM_POWERBROADCAST, WM_QUERYENDSESSION, WNDCLASSW,
        WS_OVERLAPPED,
    };

    use super::Power;

    struct State {
        on_power: Box<dyn Fn(Power)>,
        hold: Cell<bool>,
    }

    /// The hidden window. Lives on, and must be dropped by, the thread that made it.
    pub struct Watch {
        hwnd: HWND,
        state: *mut State,
    }

    unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const State;
        // Null until `new` stores it: creation messages come first.
        let Some(state) = (unsafe { state.as_ref() }) else {
            return unsafe { DefWindowProcW(hwnd, msg, wp, lp) };
        };
        match msg {
            WM_POWERBROADCAST => {
                match wp.0 as u32 {
                    PBT_APMSUSPEND => (state.on_power)(Power::Sleep),
                    PBT_APMRESUMEAUTOMATIC => (state.on_power)(Power::Wake),
                    _ => {}
                }
                LRESULT(1)
            }
            // FALSE holds the shutdown behind Windows' "preventing shutdown"
            // screen, whose Shut down anyway always wins (ADR-0036).
            WM_QUERYENDSESSION if state.hold.get() => LRESULT(0),
            _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
        }
    }

    impl Watch {
        /// Create the window on this thread. None if Windows refuses one.
        pub fn new(on_power: impl Fn(Power) + 'static) -> Option<Watch> {
            unsafe {
                let instance = GetModuleHandleW(PCWSTR::null()).ok()?;
                let class = w!("TakyonFocus");
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(wndproc),
                    hInstance: instance.into(),
                    lpszClassName: class,
                    ..Default::default()
                };
                // Fails harmlessly once the class exists (a second Watch, a test).
                RegisterClassW(&wc);
                let hwnd = CreateWindowExW(
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
                .ok()?;
                let state = Box::into_raw(Box::new(State {
                    on_power: Box::new(on_power),
                    hold: Cell::new(false),
                }));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
                Some(Watch { hwnd, state })
            }
        }

        pub fn hwnd(&self) -> isize {
            self.hwnd.0 as isize
        }

        /// Hold shutdown with `reason` on Windows' screen, or let it go (None).
        pub fn hold_shutdown(&self, reason: Option<&str>) {
            let state = unsafe { &*self.state };
            state.hold.set(reason.is_some());
            unsafe {
                match reason {
                    Some(text) => {
                        let _ = ShutdownBlockReasonCreate(self.hwnd, &HSTRING::from(text));
                    }
                    None => {
                        let _ = ShutdownBlockReasonDestroy(self.hwnd);
                    }
                }
            }
        }
    }

    impl Drop for Watch {
        fn drop(&mut self) {
            unsafe {
                SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
                let _ = DestroyWindow(self.hwnd);
                drop(Box::from_raw(self.state));
            }
        }
    }

    /// Hold idle sleep off from this thread, or release it. Per thread: the
    /// request lapses with the thread that made it. True if it was held before.
    pub fn keep_awake(on: bool) -> bool {
        let flags = if on {
            ES_CONTINUOUS | ES_SYSTEM_REQUIRED
        } else {
            ES_CONTINUOUS
        };
        let before = unsafe { SetThreadExecutionState(flags) };
        before.0 & ES_SYSTEM_REQUIRED.0 != 0
    }

    /// Dispatch this thread's messages, waiting up to `timeout` for the first.
    pub fn pump(timeout: Duration) {
        unsafe {
            MsgWaitForMultipleObjects(None, false, timeout.as_millis() as u32, QS_ALLINPUT);
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}
