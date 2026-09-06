//! The UIAccess helper.
//!
//! One job: when Takyon asks, bring a given window to the foreground. It exists
//! because UIPI refuses foreground to a medium-integrity process while an elevated
//! window holds it, and a manifest carrying `uiAccess="true"` is the only
//! sanctioned way around that. See `../../src/uiaccess.rs` for the full reasoning
//! and `docs/plans/uiaccess-signing.md` for what signing it requires.
//!
//! # Threat model
//!
//! A `uiAccess` process is a privileged thing, so the surface here is deliberately
//! tiny: it accepts eight bytes on one named pipe and does one API call.
//!
//! The pipe's default DACL admits other processes running as the same user, and
//! tightening it would not buy much — code already running as you can do far worse
//! than move a window. The control that matters is the **ownership check**: a
//! window handle is acted on only if it belongs to the process that launched this
//! helper. So the worst an unauthorised caller achieves is foregrounding Takyon's
//! own Palette, which is a thing they could do by pressing Alt+Space.
//!
//! The helper also exits when its parent does, so a `uiAccess` process is never
//! left running after the app it serves is gone.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, INVALID_HANDLE_VALUE};

/// Must match `PIPE_NAME` in `../../src/uiaccess.rs`. There is a test on that side
/// that reads this file to check they agree, since the two crates share no types.
const PIPE_NAME: &str = r"\\.\pipe\com.v3sper.takyon.uiaccess";

fn main() {
    let Some(parent) = parent_pid() else {
        // No parent means nothing to serve and nothing to authorise against.
        // Refusing to start is the right answer: a uiAccess process with no
        // ownership check is exactly what this design is avoiding.
        eprintln!("[takyon-uiaccess] could not identify the parent process; refusing to run");
        std::process::exit(1);
    };

    exit_with_parent(parent);
    serve(parent);
}

/// Accept requests forever. One instance, one client at a time — the only client
/// is Takyon, and a queue would just be a queue of the same request.
fn serve(parent: u32) {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{ReadFile, PIPE_ACCESS_INBOUND};
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
    };

    let wide: Vec<u16> = PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        // Inbound only: the client has nothing to learn from us, so the pipe cannot
        // send anything back by construction.
        let pipe = CreateNamedPipeW(
            PCWSTR(wide.as_ptr()),
            PIPE_ACCESS_INBOUND,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            0,
            64,
            0,
            None,
        );
        if pipe == INVALID_HANDLE_VALUE {
            eprintln!("[takyon-uiaccess] could not create {PIPE_NAME}");
            std::process::exit(1);
        }

        loop {
            // A client that connected and vanished before we called this returns an
            // error that means "already connected", which is a success for us.
            let _ = ConnectNamedPipe(pipe, None);

            let mut buf = [0u8; 8];
            let mut read = 0u32;
            if ReadFile(pipe, Some(&mut buf), Some(&mut read), None).is_ok() && read == 8 {
                let raw = u64::from_le_bytes(buf);
                foreground_if_owned(raw, parent);
            }

            let _ = DisconnectNamedPipe(pipe);
        }
    }
}

/// Bring the window to the front, but only if it belongs to the parent process.
///
/// This check is the whole authorisation model. Without it, anything on the
/// machine could borrow this process's privilege to yank an arbitrary window into
/// the foreground.
fn foreground_if_owned(raw: u64, parent: u32) {
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetWindowThreadProcessId, IsWindow, SetForegroundWindow,
    };

    let hwnd = HWND(raw as usize as *mut std::ffi::c_void);

    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            return;
        }
        let mut owner = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut owner));
        if owner != parent {
            return;
        }

        // SetForegroundWindow is the call UIPI would refuse from Takyon itself.
        // It succeeds here because this process carries uiAccess, which is the
        // entire reason this binary exists.
        let _ = SetForegroundWindow(hwnd);
        let _ = BringWindowToTop(hwnd);
    }
}

/// Exit when the parent does.
///
/// Not optional hygiene. Leaving a `uiAccess` process running after Takyon has
/// quit means a privileged listener on a well-known pipe with nobody watching it,
/// and it would block the next launch from creating the same pipe.
fn exit_with_parent(parent: u32) {
    use windows::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE,
    };

    std::thread::spawn(move || unsafe {
        let Ok(handle) = OpenProcess(PROCESS_SYNCHRONIZE, false, parent) else {
            return;
        };
        WaitForSingleObject(handle, INFINITE);
        let _ = CloseHandle(handle);
        std::process::exit(0);
    });
}

/// Windows exposes no `GetParentProcessId`, so this reads it out of a process
/// table snapshot — the same mechanism Task Manager uses.
fn parent_pid() -> Option<u32> {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::GetCurrentProcessId;

    unsafe {
        let me = GetCurrentProcessId();
        let snapshot: HANDLE = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        let mut found = None;
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                if entry.th32ProcessID == me {
                    found = Some(entry.th32ParentProcessID);
                    break;
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
        found
    }
}
