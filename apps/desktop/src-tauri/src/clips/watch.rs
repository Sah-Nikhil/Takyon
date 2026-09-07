//! The clipboard watcher: `AddClipboardFormatListener` on a message-only window.
//!
//! Polling `GetClipboardSequenceNumber` would be the other option and is worse in
//! both directions: it misses a copy-then-copy inside the interval, and it wakes a
//! process whose whole premise is being idle and warm (ADR-0003).
//!
//! The listener needs a window, and the Palette's belongs to Tauri's message
//! loop. So this owns a message-only window (`HWND_MESSAGE`) on its own thread:
//! never visible, never in the taskbar, no paint, no input.
//!
//! Two exclusions are checked before anything is read, both from ADR-0006 — the
//! clipboard format password managers set, and the user's own blocklist.

use std::sync::Arc;
#[cfg(windows)]
use std::sync::OnceLock;

use super::blocklist::Blocklist;
#[cfg(windows)]
use super::store::ClipKind;
use super::store::ClipStore;

/// Sent to every listener when the clipboard changes. Not in `windows-rs`.
#[cfg(windows)]
const WM_CLIPBOARDUPDATE: u32 = 0x031D;

/// Longest capture, in characters.
///
/// Past this the copy is skipped rather than truncated: a clip that is silently
/// half a document is worse than one that is absent. Windows' own history draws
/// the line in the same place, at 4 MB.
pub const MAX_CHARS: usize = 4 * 1024 * 1024;

/// The formats an application sets to say "do not record this".
///
/// The first is what ADR-0006 names and what password managers set. The second is
/// Windows' own opt-out, honoured for the same reason: an application that told
/// the OS not to keep this meant it for us too.
#[cfg(windows)]
const EXCLUDE_FORMATS: &[&str] = &[
    "ExcludeClipboardContentFromMonitorProcessing",
    "CanIncludeInClipboardHistory",
];

/// What the window procedure needs. A static because the procedure is an
/// `extern "system"` function with no room for a payload.
#[cfg(windows)]
struct Context {
    store: Arc<ClipStore>,
    blocklist: Arc<Blocklist>,
}

#[cfg(windows)]
static CONTEXT: OnceLock<Context> = OnceLock::new();

/// Whether a copy from `exe` is recorded at all.
///
/// Split out from the Win32 path so the rule is testable without a clipboard: the
/// two mechanisms are the feature's entire safety story.
pub fn should_capture(excluded: bool, exe: Option<&str>, blocklist: &Blocklist) -> bool {
    !excluded && !blocklist.blocks(exe)
}

/// Whether captured text is worth a row.
pub fn acceptable(text: &str) -> bool {
    !text.trim().is_empty() && text.chars().count() <= MAX_CHARS
}

/// Turn a scanned UTF-16 run into a clip, or refuse it.
///
/// Split from the Win32 read so the cap is testable, because it was wrong: the
/// scan stopped *at* `MAX_CHARS`, so an oversized copy arrived exactly on the
/// limit and was stored truncated. The caller scans one past; this refuses it.
pub fn text_within_cap(units: &[u16]) -> Option<String> {
    if units.len() > MAX_CHARS {
        return None;
    }
    let text = String::from_utf16_lossy(units);
    acceptable(&text).then_some(text)
}

/// Which process a copy is attributed to, given both candidate pids.
///
/// Owner wins when there is one: a context-menu copy leaves the owner right and
/// the foreground wrong. But a .NET or WinRT copier destroys its clipboard window
/// on set and Windows reports **no owner** — measured here as every copy.
pub fn attribution(owner_pid: u32, foreground_pid: u32) -> Option<u32> {
    match (owner_pid, foreground_pid) {
        (0, 0) => None,
        (0, front) => Some(front),
        (owner, _) => Some(owner),
    }
}

/// Start watching. Runs until the process ends.
///
/// Returns without a watcher if one is already running — the listener is
/// registered per window, and a second would record every copy twice.
#[cfg(windows)]
pub fn spawn(store: Arc<ClipStore>, blocklist: Arc<Blocklist>) {
    if CONTEXT.set(Context { store, blocklist }).is_err() {
        return;
    }
    std::thread::Builder::new()
        .name("clipboard-watcher".into())
        .spawn(run)
        .ok();
}

#[cfg(not(windows))]
pub fn spawn(_store: Arc<ClipStore>, _blocklist: Arc<Blocklist>) {}

#[cfg(windows)]
fn run() {
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::DataExchange::AddClipboardFormatListener;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
        TranslateMessage, HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSW,
    };

    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_CLIPBOARDUPDATE {
            capture();
            return LRESULT(0);
        }
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    unsafe {
        let instance = match GetModuleHandleW(PCWSTR::null()) {
            Ok(h) => h,
            Err(e) => return eprintln!("[takyon] clipboard watcher has no module handle: {e}"),
        };
        let class = w!("TakyonClipboardWatcher");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        };
        // Zero means the class is already registered, which happens if this is ever
        // restarted. Fine to carry on with — the class is what we would register.
        RegisterClassW(&wc);

        let window = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("takyon-clipboard"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            // Message-only: no z-order, no paint, and invisible to every window
            // enumeration, which is what keeps it out of Alt+Tab and the taskbar.
            Some(HWND_MESSAGE),
            None,
            Some(instance.into()),
            None,
        );
        let window = match window {
            Ok(w) => w,
            Err(e) => return eprintln!("[takyon] clipboard watcher window failed: {e}"),
        };

        if let Err(e) = AddClipboardFormatListener(window) {
            return eprintln!("[takyon] clipboard listener could not register: {e}");
        }

        let mut msg = MSG::default();
        // `GetMessageW` returns -1 on error, and treating that as a message would
        // spin this thread at full speed forever.
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// One clipboard change: decide, read, store.
#[cfg(windows)]
fn capture() {
    let Some(ctx) = CONTEXT.get() else {
        return;
    };
    let exe = owner_exe();
    if !should_capture(excluded_by_format(), exe.as_deref(), &ctx.blocklist) {
        return;
    }
    // Already filtered by `text_within_cap`: blank and oversized never get here.
    let Some(text) = read_text() else {
        return;
    };
    if let Err(e) = ctx.store.insert(ClipKind::Text, exe.as_deref(), &text) {
        eprintln!("[takyon] a clip could not be stored: {e}");
    }
}

/// Whether the copying application asked not to be recorded.
///
/// Presence alone is the signal for both formats. `CanIncludeInClipboardHistory`
/// carries a DWORD, but an application only ever sets it to deny — nobody writes
/// the format to say yes.
#[cfg(windows)]
fn excluded_by_format() -> bool {
    use windows::core::HSTRING;
    use windows::Win32::System::DataExchange::{
        IsClipboardFormatAvailable, RegisterClipboardFormatW,
    };

    EXCLUDE_FORMATS.iter().any(|name| unsafe {
        let id = RegisterClipboardFormatW(&HSTRING::from(*name));
        id != 0 && IsClipboardFormatAvailable(id).is_ok()
    })
}

/// The executable that put this on the clipboard, if it can be identified.
///
/// Owner first, foreground second — see [`attribution`] for why both are needed.
/// `None` where neither can be resolved, and a `None` is never blocked: failing
/// closed would stop capture whenever a window could not be identified.
#[cfg(windows)]
fn owner_exe() -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::DataExchange::GetClipboardOwner;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    unsafe {
        let mut owner_pid = 0u32;
        if let Ok(owner) = GetClipboardOwner() {
            GetWindowThreadProcessId(owner, Some(&mut owner_pid));
        }
        let mut front_pid = 0u32;
        GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut front_pid));

        let pid = attribution(owner_pid, front_pid)?;
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(handle);
        ok.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// The clipboard's text, or `None` if it holds none.
///
/// Opening can fail while another process holds the clipboard, which is normal
/// right after a copy, so it is retried briefly rather than treated as an error.
#[cfg(windows)]
pub fn read_text() -> Option<String> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    use windows::Win32::System::Ole::CF_UNICODETEXT;

    let format = CF_UNICODETEXT.0 as u32;
    // Held for the whole open/close span, and taken before the availability
    // check so a write cannot land between the two. See [`super::os::lock`].
    let _clipboard = super::os::lock();

    unsafe {
        if IsClipboardFormatAvailable(format).is_err() {
            return None;
        }
        let mut opened = false;
        for attempt in 0..10 {
            if OpenClipboard(None).is_ok() {
                opened = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10 * (attempt + 1)));
        }
        if !opened {
            return None;
        }

        // Every path from here closes the clipboard. Leaving it open locks it for
        // the whole desktop, which presents as "copy and paste stopped working".
        let text = (|| {
            let handle = GetClipboardData(format).ok()?;
            let global = HGLOBAL(handle.0);
            let ptr = GlobalLock(global) as *const u16;
            if ptr.is_null() {
                return None;
            }
            // One *past* the cap, so an oversized copy is distinguishable from one
            // that lands exactly on it. Stopping at the cap stored a silently
            // truncated document, which is the bug this shape prevents.
            let mut len = 0usize;
            while *ptr.add(len) != 0 && len <= MAX_CHARS {
                len += 1;
            }
            let units = std::slice::from_raw_parts(ptr, len);
            let text = text_within_cap(units);
            let _ = GlobalUnlock(global);
            text
        })();
        let _ = CloseClipboard();
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocklist() -> Blocklist {
        Blocklist::open(None).expect("in-memory blocklist")
    }

    /// ADR-0006's first mechanism. A password manager setting the format is the
    /// one case this feature exists to get right.
    #[test]
    fn v0_5_the_exclusion_format_stops_capture_whatever_the_source() {
        let list = blocklist();
        assert!(!should_capture(true, Some("bitwarden.exe"), &list));
        assert!(!should_capture(true, None, &list));
    }

    /// The second mechanism, for applications that set no format.
    #[test]
    fn v0_5_a_blocklisted_exe_stops_capture_without_the_format() {
        let list = blocklist();
        list.add("notepad.exe").unwrap();
        assert!(!should_capture(false, Some(r"C:\Windows\notepad.exe"), &list));
        assert!(should_capture(false, Some(r"C:\Windows\write.exe"), &list));
    }

    #[test]
    fn v0_5_an_ordinary_copy_is_captured() {
        assert!(should_capture(false, Some("code.exe"), &blocklist()));
        assert!(should_capture(false, None, &blocklist()));
    }

    #[test]
    fn v0_5_blank_and_oversized_clips_are_skipped() {
        assert!(!acceptable(""));
        assert!(!acceptable("   \r\n\t "));
        assert!(acceptable("x"));
        assert!(!acceptable(&"x".repeat(MAX_CHARS + 1)));
    }

    /// The truncation bug, found by driving the real build: a 5 MB copy arrived
    /// as a row of exactly `MAX_CHARS`, because the scan stopped *at* the cap and
    /// the result then looked acceptable. Half a document stored silently is
    /// worse than no document at all.
    #[test]
    fn v0_5_a_copy_past_the_cap_is_refused_rather_than_truncated() {
        let over = vec![b'x' as u16; MAX_CHARS + 1];
        assert!(text_within_cap(&over).is_none());

        // Exactly at the cap is still a clip. The boundary is the whole point.
        let at = vec![b'x' as u16; MAX_CHARS];
        assert_eq!(text_within_cap(&at).map(|t| t.len()), Some(MAX_CHARS));
    }

    #[test]
    fn v0_5_blank_wide_text_is_not_a_clip() {
        let blank: Vec<u16> = "   
 ".encode_utf16().collect();
        assert!(text_within_cap(&blank).is_none());
        let real: Vec<u16> = "hello".encode_utf16().collect();
        assert_eq!(text_within_cap(&real).as_deref(), Some("hello"));
    }

    /// The attribution bug, also found by driving the real build: every row had a
    /// NULL `source_exe`, because `GetClipboardOwner` reports nothing for a .NET
    /// or WinRT copier — which is most of them. With no source the blocklist can
    /// never match, and half of ADR-0006's exclusion story is dead.
    #[test]
    fn v0_5_attribution_falls_back_to_the_foreground_window() {
        assert_eq!(attribution(0, 4321), Some(4321));
        assert_eq!(attribution(0, 0), None);
    }

    /// The owner still wins when there is one: a copy from a context menu leaves
    /// the owner right and the foreground wrong.
    #[test]
    fn v0_5_the_clipboard_owner_outranks_the_foreground_window() {
        assert_eq!(attribution(1234, 4321), Some(1234));
        assert_eq!(attribution(1234, 0), Some(1234));
    }

    /// Both spellings, asserted literally. A typo in either is an exclusion that
    /// silently never fires, which is the failure nothing else would notice.
    #[cfg(windows)]
    #[test]
    fn v0_5_the_exclusion_format_names_are_the_ones_windows_defines() {
        assert_eq!(
            EXCLUDE_FORMATS,
            &[
                "ExcludeClipboardContentFromMonitorProcessing",
                "CanIncludeInClipboardHistory"
            ]
        );
    }
}
