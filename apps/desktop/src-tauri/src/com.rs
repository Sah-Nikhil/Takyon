//! One thread's COM apartment, initialised for as long as the scope is held.
//!
//! Shared because three callers need it: the application walk (`sources/apps.rs`),
//! the system-entries walk (`sources/system.rs`), and launching a control-panel
//! task by PIDL (`launch.rs`). Each opens a scope, does its shell work, drops it.
//!
//! **Apartment-threaded.** `AppsFolder` and the control-panel namespace are shell
//! namespace extensions, several of which deadlock when enumerated from an MTA.

/// COM initialised for the lifetime of one scope.
///
/// Once per unit of work, not per item. Idempotent: nesting one inside another
/// (`RPC_E_CHANGED_MODE`) is a working thread, left for the outer scope to close.
#[cfg(windows)]
pub struct ComScope {
    initialised: bool,
}

#[cfg(windows)]
impl ComScope {
    pub fn new() -> Self {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        // `RPC_E_CHANGED_MODE` means someone already initialised this thread into
        // the other apartment. That is a working COM thread, so carry on — but do
        // not uninitialise it on the way out, because we did not initialise it.
        ComScope {
            initialised: hr.is_ok(),
        }
    }
}

#[cfg(windows)]
impl Default for ComScope {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(windows)]
impl Drop for ComScope {
    fn drop(&mut self) {
        if self.initialised {
            unsafe { windows::Win32::System::Com::CoUninitialize() };
        }
    }
}
