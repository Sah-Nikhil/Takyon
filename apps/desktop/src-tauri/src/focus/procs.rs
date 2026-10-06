//! Running processes: a snapshot Focus can match, and quitting one.
//!
//! Quit is `taskkill` without `/F`, then with: `WM_CLOSE` to every top-level
//! window so an editor can save, the grace, then terminate. No windows, no
//! grace. An elevated process cannot be opened to terminate from an
//! unelevated Takyon; that is [`Quit::Denied`].

use std::time::Duration;

use super::identity::Process;

/// One process in a snapshot.
#[derive(Clone, Debug)]
pub struct Running {
    pub pid: u32,
    pub process: Process,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quit {
    /// Exited after `WM_CLOSE`, within the grace.
    Closed,
    /// Refused or ignored `WM_CLOSE`, or had no window; terminated.
    Terminated,
    /// Elevated or protected: cannot be opened to terminate.
    Denied,
    /// Not running.
    Gone,
}

/// Every process this user can open for query. Pid 0 and 4 skipped.
pub fn snapshot() -> Vec<Running> {
    imp::snapshot()
}

/// Visible top-level windows `pid` owns.
pub fn window_count(pid: u32) -> usize {
    imp::windows_of(pid, true).len()
}

/// Quit `pid`: close, wait up to `grace`, terminate. Blocks for up to `grace`.
pub fn quit(pid: u32, grace: Duration) -> Quit {
    imp::quit(pid, grace)
}

#[cfg(windows)]
mod imp {
    use std::path::PathBuf;
    use std::time::Duration;

    use windows::core::{BOOL, PWSTR};
    use windows::Win32::Foundation::{
        CloseHandle, ERROR_INVALID_PARAMETER, ERROR_SUCCESS, HANDLE, HWND, LPARAM, WAIT_OBJECT_0,
        WIN32_ERROR, WPARAM,
    };
    use windows::Win32::Storage::Packaging::Appx::{
        GetApplicationUserModelId, GetPackageFamilyName,
    };
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
        PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        PROCESS_TERMINATE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE,
    };

    use super::{Quit, Running};
    use crate::focus::identity::Process;

    /// Closed on drop.
    struct Handle(HANDLE);

    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    pub fn snapshot() -> Vec<Running> {
        pids()
            .into_iter()
            .filter(|&pid| pid > 4)
            .filter_map(|pid| {
                Some(Running {
                    pid,
                    process: describe(pid)?,
                })
            })
            .collect()
    }

    fn pids() -> Vec<u32> {
        let mut out = Vec::new();
        unsafe {
            let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return out;
            };
            let snap = Handle(snap);
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            if Process32FirstW(snap.0, &mut entry).is_ok() {
                loop {
                    out.push(entry.th32ProcessID);
                    if Process32NextW(snap.0, &mut entry).is_err() {
                        break;
                    }
                }
            }
        }
        out
    }

    /// None when the process cannot be opened for query (protected, gone).
    fn describe(pid: u32) -> Option<Process> {
        unsafe {
            let h = Handle(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?);
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            QueryFullProcessImageNameW(h.0, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len)
                .ok()?;
            let image = PathBuf::from(String::from_utf16_lossy(&buf[..len as usize]));
            let mut session_id = 0;
            let _ = ProcessIdToSessionId(pid, &mut session_id);
            Some(Process {
                image,
                aumid: package_string(|n, b| GetApplicationUserModelId(h.0, n, b)),
                package_family: package_string(|n, b| GetPackageFamilyName(h.0, n, b)),
                session_id,
            })
        }
    }

    /// Two-call buffer dance for the `appmodel` getters. None = not packaged.
    fn package_string(get: impl Fn(*mut u32, Option<PWSTR>) -> WIN32_ERROR) -> Option<String> {
        let mut len = 0u32;
        let _ = get(&mut len, None);
        if len == 0 {
            return None;
        }
        let mut buf = vec![0u16; len as usize];
        if get(&mut len, Some(PWSTR(buf.as_mut_ptr()))) != ERROR_SUCCESS {
            return None;
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..end]))
    }

    /// Top-level windows of `pid`; `visible_only` for the test's readiness check.
    pub fn windows_of(pid: u32, visible_only: bool) -> Vec<HWND> {
        struct Search {
            pid: u32,
            visible_only: bool,
            found: Vec<HWND>,
        }
        unsafe extern "system" fn each(hwnd: HWND, lp: LPARAM) -> BOOL {
            let search = &mut *(lp.0 as *mut Search);
            let mut owner = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut owner));
            if owner == search.pid && (!search.visible_only || IsWindowVisible(hwnd).as_bool()) {
                search.found.push(hwnd);
            }
            BOOL(1)
        }
        let mut search = Search {
            pid,
            visible_only,
            found: Vec::new(),
        };
        unsafe {
            let _ = EnumWindows(Some(each), LPARAM(&mut search as *mut Search as isize));
        }
        search.found
    }

    pub fn quit(pid: u32, grace: Duration) -> Quit {
        unsafe {
            let h = match OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid) {
                Ok(h) => Handle(h),
                Err(e) if e.code() == ERROR_INVALID_PARAMETER.to_hresult() => return Quit::Gone,
                Err(_) => return Quit::Denied,
            };
            if WaitForSingleObject(h.0, 0) == WAIT_OBJECT_0 {
                return Quit::Gone;
            }
            // All top-level windows, hidden too: a tray app's are all hidden.
            let windows = windows_of(pid, false);
            for hwnd in &windows {
                let _ = PostMessageW(Some(*hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            let wait = if windows.is_empty() {
                0
            } else {
                grace.as_millis().min(u32::MAX as u128) as u32
            };
            if WaitForSingleObject(h.0, wait) == WAIT_OBJECT_0 {
                return Quit::Closed;
            }
            if TerminateProcess(h.0, 1).is_err() {
                return Quit::Denied;
            }
            let _ = WaitForSingleObject(h.0, 2_000);
            Quit::Terminated
        }
    }
}

#[cfg(not(windows))]
mod imp {
    //! macOS lands with v0.12's port (plan § macOS); nothing runs until then.
    use std::time::Duration;

    use super::{Quit, Running};

    pub fn snapshot() -> Vec<Running> {
        Vec::new()
    }

    pub fn windows_of(_: u32, _: bool) -> Vec<()> {
        Vec::new()
    }

    pub fn quit(_: u32, _: Duration) -> Quit {
        Quit::Denied
    }
}
