//! Tapping the Windows key to open the Palette (v0.10).
//!
//! Everything else Takyon binds is an accelerator (`hotkey.rs`). This cannot be:
//! the Windows key is a *modifier*, and the shell opens Start on its release
//! when no other key intervened. So a `WH_KEYBOARD_LL` hook injects an undefined
//! virtual key on the down-stroke, the tap stops looking like a tap, and Start
//! is not owed. Nothing is ever swallowed — [`imp::hook_proc`] says why.
//!
//! Off by default. The mechanism, its three costs and the macOS branch are in
//! `docs/plans/v0.10-appearance.md` §6 and `docs/tbd/v0.10.md`.

use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};

use tauri::AppHandle;

/// Whether the Windows key is currently held, as this hook has seen it.
static WIN_DOWN: AtomicBool = AtomicBool::new(false);
/// Whether any other key was pressed while it was held. A chord, not a tap.
static CHORDED: AtomicBool = AtomicBool::new(false);
/// Whether a hook is installed right now. The switch in Settings reads this.
static ARMED: AtomicBool = AtomicBool::new(false);

/// The worker that actually toggles the Palette.
///
/// The callback may not: showing a window touches the event loop, far beyond
/// this hook's budget. A send is a few instructions and blocks on nothing.
static TOGGLE: OnceLock<Sender<()>> = OnceLock::new();

/// The hook thread's id, so it can be told to quit. `None` when nothing is armed.
static THREAD: Mutex<Option<u32>> = Mutex::new(None);

/// Whether the hook is installed. Not the same question as whether it was asked
/// for — `SetWindowsHookExW` can refuse, and Settings has to be able to say so.
pub fn armed() -> bool {
    ARMED.load(Relaxed)
}

/// Arm the hook at startup if the stored preference asks for it.
///
/// Quiet about failure on purpose: a refused hook at login should not put a
/// dialog over whatever else is starting. The Keyboard page reports it instead,
/// which is where someone can act on it.
pub fn restore(app: &AppHandle, prefs: &crate::prefs::Prefs) {
    if crate::prefs::flag(prefs, crate::prefs::SUPER_HOTKEY, false) && !arm(app, true) {
        eprintln!("[takyon] the Windows-key hook could not be installed at startup");
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_LWIN, VK_RWIN,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
        UnhookWindowsHookEx, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, WH_KEYBOARD_LL,
        WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    /// The key injected to make a Windows-key tap look like a chord.
    ///
    /// `0xE8` is unassigned in Microsoft's table: reserved, handled by nobody.
    /// The shell only needs *a* key between down and up, and anything with a
    /// meaning would also do it. `VK_F24` is worse — a keyboard can send it.
    const VK_NOTHING: u16 = 0xE8;

    /// Post a down/up pair for [`VK_NOTHING`].
    ///
    /// Two events in one `SendInput` call rather than two calls: the pair has to
    /// reach the queue with nothing interleaved, or the shell can still see a
    /// clean tap between them.
    fn defeat_start_menu() {
        let key = |flags: KEYBD_EVENT_FLAGS| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(VK_NOTHING),
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let events = [key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)];
        // The return value is how many events were queued. Nothing useful to do
        // with a short write here: the consequence is the Start menu opening
        // alongside the Palette, which is visible, and the alternative — logging
        // from inside a low-level hook — is the thing that gets us unhooked.
        unsafe {
            SendInput(&events, std::mem::size_of::<INPUT>() as i32);
        }
    }

    /// The hook. Everything here is shaped by its budget.
    ///
    /// **Nothing is ever swallowed.** Eating the release stops Start too, but
    /// leaves the OS believing Win is held, so the next click is a Win+click.
    /// Injecting makes the worst failure a Start menu that opens *as well*.
    pub unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        // Negative codes are not ours to interpret; the documented contract is to
        // pass them straight on.
        if code < 0 {
            return unsafe { CallNextHookEx(None, code, wparam, lparam) };
        }

        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let message = wparam.0 as u32;
        let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
        let up = message == WM_KEYUP || message == WM_SYSKEYUP;
        // Our own [`defeat_start_menu`] pair arrives here too. Counting it as a
        // chord would mean no tap ever registers — the hook would defeat itself.
        let injected = info.flags.0 & LLKHF_INJECTED.0 != 0;
        let is_win = info.vkCode == VK_LWIN.0 as u32 || info.vkCode == VK_RWIN.0 as u32;

        if !injected {
            if is_win && down {
                // Auto-repeat sends the down-stroke over and over while the key
                // is held. Only the first is the start of a gesture; injecting on
                // each would put a stream of dummy keys into the queue.
                if !WIN_DOWN.swap(true, Relaxed) {
                    CHORDED.store(false, Relaxed);
                    defeat_start_menu();
                }
            } else if is_win && up {
                WIN_DOWN.store(false, Relaxed);
                if !CHORDED.load(Relaxed) {
                    // A send, never the toggle itself. See [`TOGGLE`].
                    if let Some(tx) = TOGGLE.get() {
                        let _ = tx.send(());
                    }
                }
            } else if down && WIN_DOWN.load(Relaxed) {
                CHORDED.store(true, Relaxed);
            }
        }

        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    /// Install the hook and pump messages until told to stop.
    ///
    /// **Its own thread and its own message loop, not for tidiness.**
    /// `SetWindowsHookExW` binds the hook to the installing thread and delivers
    /// through its queue; without a loop it reports success and never fires.
    pub fn run(ready: Sender<bool>) {
        let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0) };
        let Ok(hook) = hook else {
            let _ = ready.send(false);
            return;
        };

        *THREAD.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(unsafe { GetCurrentThreadId() });
        ARMED.store(true, Relaxed);
        let _ = ready.send(true);

        let mut message = MSG::default();
        // `GetMessageW` returns 0 on `WM_QUIT`, which is what [`release`] posts.
        // Nothing else is dispatched: this queue exists only to keep the hook
        // serviced, and there is no window to deliver anything to.
        while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
            if message.message == WM_QUIT {
                break;
            }
        }

        let _ = unsafe { UnhookWindowsHookEx(hook) };
        ARMED.store(false, Relaxed);
        *THREAD.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Ask the hook thread to unhook and exit.
    pub fn release() {
        let id = THREAD.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(id) = id {
            let _ = unsafe { PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        }
        // Cleared here as well as on the thread: the caller reads this
        // immediately and the thread may not have woken yet.
        ARMED.store(false, Relaxed);
        WIN_DOWN.store(false, Relaxed);
        CHORDED.store(false, Relaxed);
    }
}

/// Turn the Windows-key binding on or off. Returns what is actually true.
///
/// The return value is the whole contract: a switch that reads on against a hook
/// that is not installed is worse than either honest state, so `settings.rs`
/// writes the preference only when this agrees with what was asked.
#[cfg(windows)]
pub fn arm(app: &AppHandle, on: bool) -> bool {
    if !on {
        imp::release();
        return false;
    }
    if armed() {
        return true;
    }

    // Started once and kept: the worker outlives any number of arm/release
    // cycles, and a `Sender` held in a `OnceLock` is what makes the hook's own
    // send lock-free.
    TOGGLE.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<()>();
        let app = app.clone();
        std::thread::spawn(move || {
            while rx.recv().is_ok() {
                let bench = tauri::Manager::state::<crate::bench::Bench>(&app);
                crate::window::toggle(&app, &bench);
            }
        });
        tx
    });

    let (ready, done) = mpsc::channel::<bool>();
    std::thread::spawn(move || imp::run(ready));
    // Waited on rather than assumed: `SetWindowsHookExW` is the call that can
    // refuse, and the answer this function returns is the one the user's switch
    // settles on.
    done.recv().unwrap_or(false)
}

#[cfg(not(windows))]
pub fn arm(_app: &AppHandle, _on: bool) -> bool {
    // No `WH_KEYBOARD_LL` anywhere else, and the macOS binding is a different
    // key entirely (`docs/plans/post-v1.md`). Reporting false is honest: the
    // switch settles off and says the hook is not installed.
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nothing is armed until something arms it. The interesting half of this is
    /// that `armed()` must be readable before any hook has ever been installed —
    /// the Keyboard page calls it on mount, which is before any of this runs.
    #[test]
    fn v0_10_nothing_is_armed_by_default() {
        assert!(!armed());
    }
}
