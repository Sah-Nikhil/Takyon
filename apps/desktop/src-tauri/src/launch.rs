//! Starting things, and the other `Ctrl+K` actions.
//!
//! **Everything goes through `ShellExecuteW`**, including plain executables. It is
//! what Explorer does, so an app gets the same environment and `AppUserModelID`
//! association as a double-click; it is the only way to reach a `steam://` URL or
//! a `shell:AppsFolder\` item; and `std::process::Command` would hand the child
//! our standard handles, so a session-long launcher would hold a pipe open for
//! every app it ever started.
//!
//! Elevation is the same call with the `runas` verb. Nothing here runs elevated.
//!
//! macOS goes through **`NSWorkspace`** (ADR-0026), whose
//! `openApplicationAtURL:` reports the launched app rather than needing a diff.

use std::path::PathBuf;

use crate::entry::LaunchTarget;

/// Start something.
///
/// **The Palette must already be hidden** (v0.2 task 7): `ShellExecuteW` returns
/// when the shell accepts the request, not when a window exists — hundreds of
/// milliseconds for a large app, with the Palette sitting over it.
pub fn open(target: &LaunchTarget) -> Result<Option<PathBuf>, String> {
    match target {
        LaunchTarget::Exe {
            path,
            args,
            working_dir,
        } => shell_execute(
            None,
            &path.to_string_lossy(),
            args.as_deref(),
            working_dir.as_ref().map(|d| d.to_string_lossy().to_string()).as_deref(),
        ),
        LaunchTarget::Aumid(aumid) => {
            shell_execute(None, &format!(r"shell:AppsFolder\{aumid}"), None, None)
        }
        // Through the launcher, never the game's own executable. Each URI and why
        // it is shaped that way lives in `GameLauncher::uri`.
        LaunchTarget::Game { launcher, id } => {
            shell_execute(None, &launcher.uri(id), None, None)
        }
        // A URI the shell resolves itself — `ms-settings:bluetooth`. Same call as
        // a `steam://` URL; the shell picks the handler.
        LaunchTarget::Uri(uri) => shell_execute(None, uri, None, None),
        // A control-panel task, identified only by an absolute PIDL. No path
        // `ShellExecuteW` accepts, so invoke its default verb (`open`) through the
        // id-list directly.
        LaunchTarget::ShellItem(pidl) => shell_execute_idlist(pidl),
    }
}

/// Start something elevated, raising the UAC prompt.
///
/// Only meaningful for a real executable: there is nothing to elevate about a
/// packaged app or a Steam URL, and asking the shell to `runas` one of those
/// produces an error dialog rather than a useful outcome.
pub fn run_as_admin(target: &LaunchTarget) -> Result<Option<PathBuf>, String> {
    match target {
        LaunchTarget::Exe {
            path,
            args,
            working_dir,
        } => shell_execute(
            Some("runas"),
            &path.to_string_lossy(),
            args.as_deref(),
            working_dir.as_ref().map(|d| d.to_string_lossy().to_string()).as_deref(),
        ),
        _ => Err("This kind of application cannot be run as administrator.".into()),
    }
}

/// Open Explorer with the target selected.
///
/// `/select,` needs the path quoted and needs no space after the comma — both are
/// load-bearing. Without the quotes any path containing a space opens Explorer at
/// Documents instead, which looks like the feature silently not working.
pub fn reveal(target: &LaunchTarget) -> Result<(), String> {
    let LaunchTarget::Exe { path, .. } = target else {
        return Err("This kind of application has no file to show.".into());
    };

    #[cfg(windows)]
    {
        shell_execute(
            None,
            "explorer.exe",
            Some(&format!("/select,\"{}\"", path.display())),
            None,
        )
        .map(|_| ())
    }

    // `-R` is Finder's own reveal-and-select. No quoting problem to solve: the
    // path is one argument rather than a command line.
    #[cfg(target_os = "macos")]
    {
        reveal_in_finder(path)
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = path;
        Err("revealing a file is not implemented on this platform".into())
    }
}

/// The path an Entry points at, for "Copy path".
pub fn path_of(target: &LaunchTarget) -> Option<String> {
    match target {
        LaunchTarget::Exe { path, .. } => Some(path.to_string_lossy().to_string()),
        _ => None,
    }
}

/// Start something and, where Windows will say, report what actually started.
///
/// `ShellExecuteExW` rather than `ShellExecuteW` purely for `SEE_MASK_NOCLOSEPROCESS`,
/// which is the only way to learn the image path of what was launched (v0.3 task
/// 1b). Same verb, file, arguments and show command, so behaviour is unchanged.
#[cfg(windows)]
fn shell_execute(
    verb: Option<&str>,
    file: &str,
    args: Option<&str>,
    dir: Option<&str>,
) -> Result<Option<PathBuf>, String> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let verb = verb.map(HSTRING::from);
    let file = HSTRING::from(file);
    let args = args.map(HSTRING::from);
    let dir = dir.map(HSTRING::from);

    // `PCWSTR(h.as_ptr())`, not a `From` conversion: the pointer borrows from the
    // `HSTRING`, so each must outlive the call. Building them inline would drop
    // every string at the end of its argument and hand the shell dangling
    // pointers.
    let as_pcwstr = |s: &Option<HSTRING>| {
        s.as_ref()
            .map(|h| PCWSTR(h.as_ptr()))
            .unwrap_or(PCWSTR::null())
    };

    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: as_pcwstr(&verb),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: as_pcwstr(&args),
        lpDirectory: as_pcwstr(&dir),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };

    let started = unsafe { ShellExecuteExW(&mut info) };

    // The documented success test, and it is genuinely this odd: `hInstApp` is an
    // `HINSTANCE` for compatibility with 16-bit Windows, and any value of 32 or
    // below is an error code wearing a pointer's clothes.
    if started.is_err() || info.hInstApp.0 as isize <= 32 {
        return Err(explain_shell_error(info.hInstApp.0 as isize));
    }

    let image = image_of(info.hProcess);
    if !info.hProcess.is_invalid() {
        // `SEE_MASK_NOCLOSEPROCESS` hands us the handle to close. Closing it does
        // not end the process; leaking it would hold a dead one alive all session.
        unsafe { let _ = CloseHandle(info.hProcess); };
    }
    Ok(image)
}

/// The executable behind a process handle, if Windows gave us one.
///
/// `None` is routine rather than an error: a packaged app is activated through a
/// broker and returns no handle, and a `steam://` URL starts nothing of ours.
/// Task 1b treats a missing observation as no evidence, never as a negative.
#[cfg(windows)]
fn image_of(handle: windows::Win32::Foundation::HANDLE) -> Option<PathBuf> {
    use windows::core::PWSTR;
    use windows::Win32::System::Threading::{QueryFullProcessImageNameW, PROCESS_NAME_WIN32};

    if handle.is_invalid() {
        return None;
    }
    let mut buffer = [0u16; 32_768];
    let mut len = buffer.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut len,
        )
        .ok()?;
    }
    Some(PathBuf::from(String::from_utf16_lossy(
        &buffer[..len as usize],
    )))
}

/// Start something through `/usr/bin/open`.
///
/// Returns `None` rather than a path: `open` hands off to `launchd` and says
/// nothing about what started, so v0.3's launched-image identity has no macOS
/// half. Frecency falls back to the Entry's own id, as it did before v0.3.
#[cfg(target_os = "macos")]
fn shell_execute(
    verb: Option<&str>,
    file: &str,
    args: Option<&str>,
    dir: Option<&str>,
) -> Result<Option<PathBuf>, String> {
    // No macOS analogue, and none wanted: privilege is a per-operation
    // authorisation prompt, not a way to start an application (ADR-0007's
    // no-elevation stance holds here too).
    if verb == Some("runas") {
        return Err("macOS has no equivalent of running an application as administrator.".into());
    }

    let mut argv: Vec<&str> = vec![file];
    // Split on whitespace, which is what the Windows side's single argument
    // string already assumes; a quoted argument containing spaces is a gap both
    // platforms share. No `--args` separator any more — that was `open`'s
    // calling convention, and `NSWorkspaceOpenConfiguration` takes a real array.
    if let Some(args) = args {
        argv.extend(args.split_whitespace());
    }
    run_open(&argv, dir)
}

/// How long to wait for `openApplicationAtURL:` to report what it started.
///
/// The Palette is already hidden and nothing is waiting on a frame, but a launch
/// that hangs must not hang the action thread with it.
#[cfg(target_os = "macos")]
const LAUNCH_REPORT_WAIT_MS: u64 = 2_000;

/// Start something through `NSWorkspace`, and report what actually started.
///
/// Replaces the `/usr/bin/open` stopgap (ADR-0026). An `.app` goes through
/// `openApplicationAtURL:`, whose completion handler hands back the launched
/// `NSRunningApplication` — no diffing, no notification observing, no race.
#[cfg(target_os = "macos")]
fn run_open(args: &[&str], dir: Option<&str>) -> Result<Option<PathBuf>, String> {
    // `dir` has no NSWorkspace counterpart: a launched app inherits launchd's
    // environment rather than ours, so there is no working directory to set.
    // Dropped rather than faked; it is a Windows-only affordance.
    let _ = dir;

    let (file, arguments) = args.split_first().ok_or("nothing to open")?;

    if is_app_bundle(file) {
        open_application(file, arguments)
    } else {
        open_url(file).map(|_| None)
    }
}

/// Is this path an application bundle rather than a document or a URL?
///
/// Decides which of the two `NSWorkspace` entry points to use, and is the one
/// piece of `run_open` with any judgement in it. Compiled on every platform so a
/// Windows test run covers it (the same reason `shellenv`'s parsers are).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn is_app_bundle(path: &str) -> bool {
    std::path::Path::new(path.trim_end_matches('/'))
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("app"))
}

/// Is this a URL to hand to `URLWithString`, or a path for `fileURLWithPath`?
///
/// `URLWithString` returns nil for an absolute path, and `fileURLWithPath`
/// mangles a URL into a relative file name, so guessing wrong fails either
/// silently or absurdly rather than loudly.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn looks_like_url(spec: &str) -> bool {
    spec.contains(':') && !spec.starts_with('/')
}

/// Open an `.app` bundle and wait briefly for its identity.
///
/// The wait is what buys launched-image identity: `openApplicationAtURL:` is
/// asynchronous, so without it the handler fires after this returns. A timeout
/// yields `None`, never an error — the application started regardless.
#[cfg(target_os = "macos")]
fn open_application(path: &str, arguments: &[&str]) -> Result<Option<PathBuf>, String> {
    use objc2_app_kit::{NSRunningApplication, NSWorkspace, NSWorkspaceOpenConfiguration};
    use objc2_foundation::{NSArray, NSError, NSString, NSURL};

    let url = NSURL::fileURLWithPath(&NSString::from_str(path));
    let config = NSWorkspaceOpenConfiguration::configuration();
    if !arguments.is_empty() {
        let args: Vec<_> = arguments.iter().map(|a| NSString::from_str(a)).collect();
        config.setArguments(&NSArray::from_retained_slice(&args));
    }

    let (tx, rx) = std::sync::mpsc::channel::<Result<Option<String>, String>>();
    let handler = block2::RcBlock::new(
        move |app: *mut NSRunningApplication, err: *mut NSError| {
            // SAFETY: AppKit passes objects that are valid for this call.
            let outcome = if !err.is_null() {
                Err(unsafe { (*err).localizedDescription() }.to_string())
            } else if app.is_null() {
                Ok(None)
            } else {
                Ok(unsafe { (*app).bundleURL() }
                    .and_then(|u| u.path())
                    .map(|p| p.to_string()))
            };
            let _ = tx.send(outcome);
        },
    );

    NSWorkspace::sharedWorkspace().openApplicationAtURL_configuration_completionHandler(
        &url,
        &config,
        Some(&handler),
    );

    match rx.recv_timeout(std::time::Duration::from_millis(LAUNCH_REPORT_WAIT_MS)) {
        Ok(Ok(path)) => Ok(path.map(PathBuf::from)),
        Ok(Err(e)) => Err(format!("macOS refused to start it: {e}")),
        Err(_) => Ok(None),
    }
}

/// Hand a URL, or a file that is not an application, to its default handler.
#[cfg(target_os = "macos")]
fn open_url(spec: &str) -> Result<(), String> {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSString, NSURL};

    let text = NSString::from_str(spec);
    // A scheme makes it a URL, anything else is a path — and a path needs the
    // file-URL constructor, because `URLWithString` returns nil for one.
    let url = if looks_like_url(spec) {
        NSURL::URLWithString(&text)
    } else {
        Some(NSURL::fileURLWithPath(&text))
    };
    let url = url.ok_or_else(|| format!("{spec} is not something macOS can open."))?;

    if NSWorkspace::sharedWorkspace().openURL(&url) {
        Ok(())
    } else {
        Err("macOS refused to open it.".into())
    }
}

/// Show a file in Finder with it selected.
#[cfg(target_os = "macos")]
fn reveal_in_finder(path: &std::path::Path) -> Result<(), String> {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSArray, NSString, NSURL};

    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let urls = NSArray::from_retained_slice(&[url]);
    NSWorkspace::sharedWorkspace().activateFileViewerSelectingURLs(&urls);
    Ok(())
}

#[cfg(not(any(windows, target_os = "macos")))]
fn shell_execute(
    _verb: Option<&str>,
    _file: &str,
    _args: Option<&str>,
    _dir: Option<&str>,
) -> Result<Option<PathBuf>, String> {
    Err("launching is only implemented on Windows and macOS".into())
}

/// Invoke a shell item's default verb by its absolute PIDL (task 8).
///
/// A control-panel task has no path or reparseable name, so the PIDL captured at
/// enumeration is the handle: launch it through `SEE_MASK_IDLIST`, which runs the
/// default verb. Opens its own COM scope; no image path, nothing of ours ran.
#[cfg(windows)]
fn shell_execute_idlist(pidl_bytes: &[u8]) -> Result<Option<PathBuf>, String> {
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_IDLIST, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let _com = crate::com::ComScope::new();
    with_aligned_pidl(pidl_bytes, |pidl| {
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_IDLIST,
            lpIDList: pidl,
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        let started = unsafe { ShellExecuteExW(&mut info) };
        if started.is_err() || info.hInstApp.0 as isize <= 32 {
            return Err(explain_shell_error(info.hInstApp.0 as isize));
        }
        Ok(None)
    })
}

/// Copy PIDL bytes into a `CoTaskMemAlloc` buffer, run `f` with the aligned
/// pointer, free it. A `Vec<u8>` is only byte-aligned; an id-list wants 2, and the
/// shell owns the free contract for id-lists it is handed.
#[cfg(windows)]
fn with_aligned_pidl<T>(
    bytes: &[u8],
    f: impl FnOnce(*mut core::ffi::c_void) -> Result<T, String>,
) -> Result<T, String> {
    use windows::Win32::System::Com::{CoTaskMemAlloc, CoTaskMemFree};

    if bytes.is_empty() {
        return Err("That system entry is no longer available.".into());
    }
    unsafe {
        let buf = CoTaskMemAlloc(bytes.len());
        if buf.is_null() {
            return Err("Out of memory launching that system entry.".into());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, bytes.len());
        let result = f(buf);
        CoTaskMemFree(Some(buf as *const _));
        result
    }
}

/// A PIDL is a Windows shell concept and has no counterpart anywhere else, so
/// this refuses rather than waiting for an implementation. Nothing off Windows
/// produces a `LaunchTarget::ShellItem` to reach it.
#[cfg(not(windows))]
fn shell_execute_idlist(_pidl_bytes: &[u8]) -> Result<Option<PathBuf>, String> {
    Err("shell items exist only on Windows.".into())
}

/// Does a captured PIDL still bind to a live shell item, without launching it?
///
/// The half of [`shell_execute_idlist`] short of `ShellExecuteEx`, so an
/// integration test can prove the walk's PIDLs are launchable —
/// `SHCreateItemFromIDList` accepting what enumeration captured — window-free.
#[cfg(windows)]
pub fn shell_item_is_bindable(pidl_bytes: &[u8]) -> bool {
    use windows::Win32::UI::Shell::{IShellItem, SHCreateItemFromIDList};

    let _com = crate::com::ComScope::new();
    with_aligned_pidl(pidl_bytes, |pidl| {
        let item: Result<IShellItem, _> =
            unsafe { SHCreateItemFromIDList(pidl as *const _) };
        Ok(item.is_ok())
    })
    .unwrap_or(false)
}

#[cfg(not(windows))]
pub fn shell_item_is_bindable(_pidl_bytes: &[u8]) -> bool {
    false
}

/// Turn a `ShellExecuteW` error code into something worth showing.
///
/// Pure, so it is testable without launching anything. The two cases that matter
/// are the ones the user causes: cancelling the UAC prompt, and an application
/// that has been uninstalled since the walk ran.
pub fn explain_shell_error(code: isize) -> String {
    match code {
        // SE_ERR_ACCESSDENIED, which is also what a cancelled UAC prompt returns.
        5 => "Cancelled, or Windows refused permission.".into(),
        2 => "That application is no longer installed.".into(),
        3 => "The folder that application lived in is gone.".into(),
        // SE_ERR_NOASSOC
        31 => "Windows has nothing registered to open that.".into(),
        other => format!("Windows refused to start it (error {other})."),
    }
}

/// How many times to ask for the clipboard, and how long to wait between.
///
/// The clipboard is one global lock and `OpenClipboard` fails outright while
/// another process holds it — Microsoft's own guidance is to retry. Takyon races
/// **itself**: `clips::watch` opens it on every change notification.
#[cfg(windows)]
const CLIPBOARD_TRIES: u32 = 10;
#[cfg(windows)]
const CLIPBOARD_RETRY: std::time::Duration = std::time::Duration::from_millis(10);

/// Take the clipboard, waiting out whoever currently holds it.
///
/// 100 ms in the worst case, on a path that only runs on an explicit Copy.
/// Failing without retrying presents as "copy silently did nothing", which is
/// indistinguishable from a broken action.
#[cfg(windows)]
unsafe fn open_clipboard_retrying() -> Result<(), String> {
    use windows::Win32::System::DataExchange::OpenClipboard;

    let mut last = String::new();
    for attempt in 0..CLIPBOARD_TRIES {
        match OpenClipboard(None) {
            Ok(()) => return Ok(()),
            Err(e) => last = e.to_string(),
        }
        if attempt + 1 < CLIPBOARD_TRIES {
            std::thread::sleep(CLIPBOARD_RETRY);
        }
    }
    Err(format!("could not open the clipboard: {last}"))
}

/// Put text on the clipboard.
///
/// Done with the Win32 calls rather than `tauri-plugin-clipboard-manager`, which
/// would mean a plugin, a capability entry and a second route from the webview to
/// the OS. Reached through [`crate::clips::os::ClipboardStore`], never directly.
#[cfg(windows)]
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    use windows::Win32::Foundation::{HANDLE, HGLOBAL};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::CF_UNICODETEXT;

    let mut wide: Vec<u16> = text.encode_utf16().collect();
    wide.push(0);
    let bytes = wide.len() * 2;

    // Held for the whole open/close span. `crate::clips::os::lock` says why the
    // Win32 lock is not enough on its own.
    let _clipboard = crate::clips::os::lock();

    unsafe {
        open_clipboard_retrying()?;

        // Every early return from here has to close the clipboard. Leaving it open
        // locks it for every other process on the desktop, which presents as
        // "copy and paste stopped working" with no clue pointing back here.
        let result = (|| -> Result<(), String> {
            EmptyClipboard().map_err(|e| format!("could not clear the clipboard: {e}"))?;

            let handle: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, bytes)
                .map_err(|e| format!("could not allocate for the clipboard: {e}"))?;
            let ptr = GlobalLock(handle);
            if ptr.is_null() {
                return Err("could not lock the clipboard buffer".into());
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr.cast::<u16>(), wide.len());
            let _ = GlobalUnlock(handle);

            // Ownership of the memory passes to the clipboard on success. Freeing
            // it here would hand every paste a dangling pointer.
            SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(handle.0)))
                .map_err(|e| format!("could not set the clipboard: {e}"))?;
            Ok(())
        })();

        let _ = CloseClipboard();
        result
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::GameLauncher;
    use std::path::PathBuf;

    #[test]
    fn v0_12_an_app_bundle_is_told_from_a_document() {
        // Picks which NSWorkspace entry point runs, and only one of the two
        // reports what it launched.
        assert!(is_app_bundle("/Applications/Safari.app"));
        assert!(is_app_bundle("/Applications/Safari.app/"));
        assert!(is_app_bundle("/Users/me/My Apps/Some Thing.APP"));
        assert!(!is_app_bundle("/Users/me/notes.txt"));
        assert!(!is_app_bundle("/opt/homebrew/bin/claude"));
        assert!(!is_app_bundle("steam://rungameid/440"));
    }

    #[test]
    fn v0_12_a_url_is_told_from_a_path() {
        assert!(looks_like_url("steam://rungameid/440"));
        assert!(looks_like_url("https://example.com"));
        assert!(looks_like_url(
            "x-apple.systempreferences:com.apple.preference.security"
        ));
        // An absolute path can hold a colon and is still a path.
        assert!(!looks_like_url("/Users/me/a:b.txt"));
        assert!(!looks_like_url("/Applications/Safari.app"));
    }

    fn exe(path: &str) -> LaunchTarget {
        LaunchTarget::Exe {
            path: PathBuf::from(path),
            args: None,
            working_dir: None,
        }
    }

    /// Elevation only means something for a real file. Offering it for a packaged
    /// app would produce a menu item whose only outcome is an error dialog —
    /// `actions::for_app` already withholds it, and this is the other half.
    #[test]
    fn v0_2_only_an_executable_can_be_elevated_or_revealed() {
        let uwp = LaunchTarget::Aumid("Microsoft.Whatever_abc!App".into());
        let steam = LaunchTarget::Game {
            launcher: GameLauncher::Steam,
            id: "440".into(),
        };
        assert!(run_as_admin(&uwp).is_err());
        assert!(run_as_admin(&steam).is_err());
        assert!(reveal(&uwp).is_err());
        assert!(reveal(&steam).is_err());
    }

    #[test]
    fn v0_2_copy_path_has_nothing_to_copy_for_a_pathless_app() {
        assert_eq!(path_of(&exe(r"C:\a\b.exe")).as_deref(), Some(r"C:\a\b.exe"));
        assert!(path_of(&LaunchTarget::Aumid("A_b!c".into())).is_none());
        assert!(path_of(&LaunchTarget::Game {
            launcher: GameLauncher::Steam,
            id: "1".into(),
        })
        .is_none());
    }

    /// A cancelled UAC prompt is the most likely failure of the elevation action,
    /// and it is not an error the user needs apologised for. It must not read as a
    /// crash.
    #[test]
    fn v0_2_a_cancelled_elevation_prompt_reads_as_cancelled() {
        let msg = explain_shell_error(5);
        assert!(msg.to_lowercase().contains("cancelled"));
    }

    #[test]
    fn v0_2_an_uninstalled_app_says_so() {
        assert!(explain_shell_error(2).contains("no longer installed"));
        // Anything unrecognised keeps its number, so it can be quoted in a report
        // rather than flattened into a friendly non-answer.
        assert!(explain_shell_error(1234).contains("1234"));
    }
}
