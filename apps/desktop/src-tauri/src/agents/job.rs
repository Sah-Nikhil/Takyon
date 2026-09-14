//! Process trees that die together: every Agent spawn goes through here.
//!
//! Windows: one Job Object per spawn, `KILL_ON_JOB_CLOSE`, child created
//! suspended and resumed only once assigned. A `.cmd` Agent is `cmd.exe` →
//! `node` → real CLI, and `TerminateProcess` on the first leaves the other two
//! running. Takyon crashing closes the handle, so Windows kills the tree.
//!
//! Unix: own process group via `setsid`, killed as `-pgid`. No crash cover:
//! nothing like `KILL_ON_JOB_CLOSE` exists, `docs/tbd/v0.11.md` records it.
//! Reasoning in ADR-0033.

use std::io;
use std::process::{Child, Command};

/// Handle that kills a spawned process and every descendant. `Send + Sync`.
pub struct Job {
    #[cfg(windows)]
    handle: windows::Win32::Foundation::HANDLE,
    #[cfg(unix)]
    pgid: i32,
}

// HANDLE is a raw pointer; job handles are thread-agnostic kernel objects.
#[cfg(windows)]
unsafe impl Send for Job {}
#[cfg(windows)]
unsafe impl Sync for Job {}

impl Job {
    /// Kill the whole tree. Never blocks, never waits: reaping is the owner's.
    pub fn terminate(&self) {
        #[cfg(windows)]
        unsafe {
            let _ = windows::Win32::System::JobObjects::TerminateJobObject(self.handle, 1);
        }
        #[cfg(unix)]
        unsafe {
            libc::kill(-self.pgid, libc::SIGKILL);
        }
    }

    /// Processes still in the job. For tests: proof a tree is gone, not a guess.
    #[cfg(windows)]
    pub fn active_processes(&self) -> io::Result<u32> {
        use windows::Win32::System::JobObjects::{
            JobObjectBasicAccountingInformation, QueryInformationJobObject,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
        };
        let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        unsafe {
            QueryInformationJobObject(
                Some(self.handle),
                JobObjectBasicAccountingInformation,
                (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                None,
            )
        }
        .map_err(io::Error::other)?;
        Ok(info.ActiveProcesses)
    }
}

#[cfg(windows)]
impl Drop for Job {
    /// Closing last handle kills any member still alive (`KILL_ON_JOB_CLOSE`).
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

/// Spawn `cmd` inside a fresh Job. On any setup failure the child is killed.
///
/// Never a process outside its job: a best-effort job is no job.
#[cfg(windows)]
pub fn spawn(cmd: &mut Command) -> io::Result<(Child, Job)> {
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::process::CommandExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    let handle = unsafe { CreateJobObjectW(None, PCWSTR::null()) }.map_err(io::Error::other)?;
    let job = Job { handle };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    // No BREAKAWAY_OK flag: a descendant cannot leave.
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    unsafe {
        SetInformationJobObject(
            job.handle,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    }
    .map_err(io::Error::other)?;

    // Suspended: a child that runs before assignment can start one outside it.
    cmd.creation_flags(super::probe::CREATE_NO_WINDOW | CREATE_SUSPENDED);
    let mut child = cmd.spawn()?;

    let process = HANDLE(child.as_raw_handle());
    let placed = unsafe { AssignProcessToJobObject(job.handle, process) }
        .map_err(io::Error::other)
        .and_then(|()| resume(child.id()));
    if let Err(e) = placed {
        let _ = child.kill();
        let _ = child.wait();
        return Err(e);
    }
    Ok((child, job))
}

#[cfg(windows)]
const CREATE_SUSPENDED: u32 = 0x0000_0004;

/// Resume a suspended process's one thread.
///
/// `Child` exposes the process handle, not the thread's, so Toolhelp finds it.
#[cfg(windows)]
fn resume(pid: u32) -> io::Result<()> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
    };
    use windows::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

    let snapshot =
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) }.map_err(io::Error::other)?;
    let mut entry = THREADENTRY32 {
        dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    let mut resumed = 0;
    let mut more = unsafe { Thread32First(snapshot, &mut entry) }.is_ok();
    while more {
        if entry.th32OwnerProcessID == pid {
            if let Ok(thread) =
                unsafe { OpenThread(THREAD_SUSPEND_RESUME, false, entry.th32ThreadID) }
            {
                if unsafe { ResumeThread(thread) } != u32::MAX {
                    resumed += 1;
                }
                unsafe {
                    let _ = CloseHandle(thread);
                }
            }
        }
        more = unsafe { Thread32Next(snapshot, &mut entry) }.is_ok();
    }
    unsafe {
        let _ = CloseHandle(snapshot);
    }
    if resumed == 0 {
        return Err(io::Error::other(format!(
            "no thread of process {pid} resumed"
        )));
    }
    Ok(())
}

/// Spawn `cmd` as leader of its own process group.
#[cfg(unix)]
pub fn spawn(cmd: &mut Command) -> io::Result<(Child, Job)> {
    use std::os::unix::process::CommandExt;
    // Already-a-leader is the only failure and is harmless, so it is ignored.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let child = cmd.spawn()?;
    let pgid = child.id() as i32;
    Ok((child, Job { pgid }))
}
