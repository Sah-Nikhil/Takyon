//! `ReadDirectoryChangesW` per root, and what to do when it drops events (§5).
//!
//! One thread per root, blocked on an overlapped read. Names only: the filter is
//! `FILE_NAME | DIR_NAME`, so a file being written produces nothing and a file
//! being created produces one event.
//!
//! **Overflow is a correctness path, not an edge case.** Under a `git checkout`
//! or an npm install the buffer fills routinely, and Windows reports it by
//! returning zero bytes. The answer is never to carry on — it is to mark the root
//! stale and rescan it (ADR-0007).

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use super::roots::is_excluded;

/// Bytes per root's change buffer.
///
/// 64 KB is the documented ceiling for a network path and holds roughly a
/// thousand names locally. Bigger buffers make overflow rarer; they cannot make
/// it impossible, which is why the overflow path exists at all.
#[cfg(windows)]
const BUFFER_BYTES: usize = 64 * 1024;

/// One thing that happened under a root.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Added(PathBuf, bool),
    Removed(PathBuf),
    Renamed {
        from: PathBuf,
        to: PathBuf,
        is_dir: bool,
    },
    /// Events were dropped. Whatever this root holds is now unknown, and the
    /// index must say so rather than serve what it happens to remember.
    Overflow(PathBuf),
}

/// Raw `FILE_NOTIFY_INFORMATION` actions, as Windows spells them.
pub const FILE_ACTION_ADDED: u32 = 1;
pub const FILE_ACTION_REMOVED: u32 = 2;
pub const FILE_ACTION_MODIFIED: u32 = 3;
pub const FILE_ACTION_RENAMED_OLD_NAME: u32 = 4;
pub const FILE_ACTION_RENAMED_NEW_NAME: u32 = 5;

/// One record out of the change buffer: an action and a root-relative path.
#[derive(Clone, Debug, PartialEq)]
pub struct Notification {
    pub action: u32,
    pub name: String,
}

/// Decode a `FILE_NOTIFY_INFORMATION` chain.
///
/// Pure, so the record walk is testable without provoking the filesystem. A
/// malformed or truncated buffer stops the walk rather than reading past it —
/// the offsets come from the kernel, but the length does not always agree.
pub fn parse_notifications(buf: &[u8]) -> Vec<Notification> {
    let mut out = Vec::new();
    let mut at = 0usize;
    loop {
        if at + 12 > buf.len() {
            break;
        }
        let next = u32_at(buf, at) as usize;
        let action = u32_at(buf, at + 4);
        let name_bytes = u32_at(buf, at + 8) as usize;

        let start = at + 12;
        let end = start + name_bytes;
        if end > buf.len() || name_bytes % 2 != 0 {
            break;
        }
        let wide: Vec<u16> = buf[start..end]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        out.push(Notification {
            action,
            name: String::from_utf16_lossy(&wide),
        });

        if next == 0 {
            break;
        }
        // A non-advancing offset would spin forever on a corrupt buffer.
        if next < 12 {
            break;
        }
        at += next;
    }
    out
}

/// Turn decoded records into [`Change`]s under one root.
///
/// Rename arrives as two records — old name then new — and they are paired here
/// so the overlay sees one move rather than a delete and an unrelated create.
/// `is_dir` is asked of the filesystem, which is why this is not pure.
pub fn to_changes(root: &Path, notifications: &[Notification], exclude: &[String]) -> Vec<Change> {
    let mut out = Vec::new();
    let mut pending_rename: Option<PathBuf> = None;

    for note in notifications {
        let path = root.join(&note.name);
        if excluded_anywhere(&path, root, exclude) {
            continue;
        }
        match note.action {
            // A modify is not an index event: the name did not change, and names
            // are all this index holds.
            FILE_ACTION_MODIFIED => {}
            FILE_ACTION_ADDED => out.push(Change::Added(path.clone(), path.is_dir())),
            FILE_ACTION_REMOVED => out.push(Change::Removed(path)),
            FILE_ACTION_RENAMED_OLD_NAME => pending_rename = Some(path),
            FILE_ACTION_RENAMED_NEW_NAME => match pending_rename.take() {
                Some(from) => out.push(Change::Renamed {
                    is_dir: path.is_dir(),
                    from,
                    to: path,
                }),
                // The old-name record can be lost across two reads. Treat the
                // arrival alone as a creation rather than dropping it.
                None => out.push(Change::Added(path.clone(), path.is_dir())),
            },
            _ => {}
        }
    }
    // An old name with no new name is a move out of the root, which is a removal
    // from this index's point of view.
    if let Some(from) = pending_rename {
        out.push(Change::Removed(from));
    }
    out
}

/// Whether any segment below the root is excluded.
///
/// Below the root only: a root under `C:\build` must not exclude itself, and the
/// walk applies the same rule.
fn excluded_anywhere(path: &Path, root: &Path, exclude: &[String]) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    relative
        .components()
        .any(|c| is_excluded(&c.as_os_str().to_string_lossy(), exclude))
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Watch every root until dropped.
///
/// Each root gets its own thread and its own handle, so one unreadable root costs
/// nothing but itself. Dropping signals the stop event and joins.
#[cfg(windows)]
pub struct Watcher {
    stop: windows::Win32::Foundation::HANDLE,
    threads: Vec<std::thread::JoinHandle<()>>,
}

#[cfg(windows)]
// SAFETY: the only shared field is a Windows event handle, which the API
// documents as usable from any thread; nothing else crosses.
unsafe impl Send for Watcher {}
#[cfg(windows)]
unsafe impl Sync for Watcher {}

#[cfg(windows)]
impl Watcher {
    /// Start watching. Changes arrive on `tx` until the Watcher is dropped.
    pub fn start(roots: Vec<PathBuf>, exclude: Vec<String>, tx: Sender<Change>) -> Option<Watcher> {
        use windows::Win32::System::Threading::CreateEventW;

        // SAFETY: a manual-reset, initially-unset, unnamed event with default
        // security. Closed in `Drop`.
        let stop = unsafe { CreateEventW(None, true, false, None) }.ok()?;

        let threads = roots
            .into_iter()
            .map(|root| {
                let tx = tx.clone();
                let exclude = exclude.clone();
                let stop = stop.0 as usize;
                std::thread::spawn(move || {
                    watch_root(root, exclude, tx, windows::Win32::Foundation::HANDLE(stop as _))
                })
            })
            .collect();

        Some(Watcher { stop, threads })
    }
}

#[cfg(windows)]
impl Drop for Watcher {
    fn drop(&mut self) {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::SetEvent;

        // SAFETY: `stop` is a live event handle owned by this struct.
        unsafe {
            let _ = SetEvent(self.stop);
        }
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
        // SAFETY: every thread has joined, so nothing else holds the handle.
        unsafe {
            let _ = CloseHandle(self.stop);
        }
    }
}

/// One root's watch loop: overlapped read, wait for it or for the stop event.
///
/// Overlapped rather than blocking, because a blocking `ReadDirectoryChangesW`
/// cannot be interrupted — the thread would sit in the kernel until something
/// changed, which on a quiet folder means until shutdown deadlocks.
#[cfg(windows)]
fn watch_root(
    root: PathBuf,
    exclude: Vec<String>,
    tx: Sender<Change>,
    stop: windows::Win32::Foundation::HANDLE,
) {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, ReadDirectoryChangesW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED,
        FILE_LIST_DIRECTORY, FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::Threading::{CreateEventW, WaitForMultipleObjects, INFINITE};
    use windows::Win32::System::IO::{GetOverlappedResult, OVERLAPPED};

    let wide: Vec<u16> = root
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // SAFETY: `wide` is NUL-terminated and outlives the call; the handle is
    // closed before returning.
    let handle = match unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_LIST_DIRECTORY.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
            None,
        )
    } {
        Ok(handle) => handle,
        // An unwatchable root is not fatal: it stays indexed as walked, and the
        // next rebuild picks up whatever changed meanwhile.
        Err(_) => return,
    };

    // SAFETY: manual-reset, unset, unnamed. Closed with the directory handle.
    let Ok(event) = (unsafe { CreateEventW(None, true, false, None) }) else {
        // SAFETY: `handle` came from CreateFileW above and is not yet closed.
        unsafe {
            let _ = CloseHandle(handle);
        }
        return;
    };

    let mut buffer = vec![0u8; BUFFER_BYTES];
    loop {
        let mut overlapped = OVERLAPPED {
            hEvent: event,
            ..Default::default()
        };

        // SAFETY: buffer and overlapped both outlive the wait below, which is
        // what makes an overlapped read sound here.
        let queued = unsafe {
            ReadDirectoryChangesW(
                handle,
                buffer.as_mut_ptr().cast(),
                BUFFER_BYTES as u32,
                true,
                FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_DIR_NAME,
                None,
                Some(&mut overlapped),
                None,
            )
        };
        if queued.is_err() {
            let _ = tx.send(Change::Overflow(root.clone()));
            break;
        }

        // SAFETY: both handles are live for the duration of the wait.
        let waited = unsafe { WaitForMultipleObjects(&[event, stop], false, INFINITE) };
        if waited != WAIT_OBJECT_0 {
            // Either the stop event or a failed wait. Either way, stop reading.
            break;
        }

        let mut bytes = 0u32;
        // SAFETY: the read has completed, so the overlapped record is done being
        // written by the kernel.
        let ok = unsafe { GetOverlappedResult(handle, &overlapped, &mut bytes, false) };
        if ok.is_err() {
            let _ = tx.send(Change::Overflow(root.clone()));
            break;
        }
        // Zero bytes with a successful read is how the buffer overflow is
        // reported. Nothing usable came back, and events were lost.
        if bytes == 0 {
            if tx.send(Change::Overflow(root.clone())).is_err() {
                break;
            }
            continue;
        }

        let notifications = parse_notifications(&buffer[..bytes as usize]);
        let mut disconnected = false;
        for change in to_changes(&root, &notifications, &exclude) {
            if tx.send(change).is_err() {
                disconnected = true;
                break;
            }
        }
        if disconnected {
            break;
        }
    }

    // SAFETY: the loop has exited, so no pending read refers to either handle.
    unsafe {
        let _ = CloseHandle(event);
        let _ = CloseHandle(handle);
    }
}

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

#[cfg(not(windows))]
pub struct Watcher;

#[cfg(not(windows))]
impl Watcher {
    pub fn start(_: Vec<PathBuf>, _: Vec<String>, _: Sender<Change>) -> Option<Watcher> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `FILE_NOTIFY_INFORMATION` chain the way the kernel lays one out.
    fn buffer(records: &[(u32, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (i, (action, name)) in records.iter().enumerate() {
            let wide: Vec<u16> = name.encode_utf16().collect();
            let name_bytes = wide.len() * 2;
            let size = 12 + name_bytes;
            // Records are 4-byte aligned; the last one carries a zero offset.
            let padded = size.div_ceil(4) * 4;
            let next = if i + 1 == records.len() { 0 } else { padded };

            out.extend_from_slice(&(next as u32).to_le_bytes());
            out.extend_from_slice(&action.to_le_bytes());
            out.extend_from_slice(&(name_bytes as u32).to_le_bytes());
            for unit in &wide {
                out.extend_from_slice(&unit.to_le_bytes());
            }
            out.resize(out.len() + (padded - size), 0);
        }
        out
    }

    /// The record walk, against a buffer shaped exactly as Windows writes one.
    #[test]
    fn v0_7_the_change_buffer_decodes_to_records() {
        let buf = buffer(&[
            (FILE_ACTION_ADDED, r"src\main.rs"),
            (FILE_ACTION_REMOVED, "notes.md"),
        ]);
        let notes = parse_notifications(&buf);
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].action, FILE_ACTION_ADDED);
        assert_eq!(notes[0].name, r"src\main.rs");
        assert_eq!(notes[1].name, "notes.md");
    }

    /// The offsets come from the kernel, the length does not always agree. A
    /// short buffer must stop the walk, never read past it.
    #[test]
    fn v0_7_a_truncated_change_buffer_stops_rather_than_overruns() {
        let buf = buffer(&[(FILE_ACTION_ADDED, "verylongfilename.rs")]);
        for cut in [0, 4, 11, 13, buf.len() - 2] {
            let _ = parse_notifications(&buf[..cut]);
        }
        assert!(parse_notifications(&buf[..8]).is_empty());
    }

    /// A zero next-offset ends the chain. A tiny one would loop forever, which is
    /// the shape a corrupt buffer takes.
    #[test]
    fn v0_7_a_nonadvancing_offset_terminates_the_walk() {
        let mut buf = buffer(&[(FILE_ACTION_ADDED, "a.rs"), (FILE_ACTION_ADDED, "b.rs")]);
        buf[0..4].copy_from_slice(&4u32.to_le_bytes());
        assert_eq!(parse_notifications(&buf).len(), 1);
    }

    /// Rename is two records. Paired, or the overlay sees a delete and an
    /// unrelated create and the file changes identity.
    #[test]
    fn v0_7_a_rename_pairs_its_two_records() {
        let root = Path::new(r"C:\Data");
        let notes = vec![
            Notification {
                action: FILE_ACTION_RENAMED_OLD_NAME,
                name: "draft.md".into(),
            },
            Notification {
                action: FILE_ACTION_RENAMED_NEW_NAME,
                name: "final.md".into(),
            },
        ];
        let changes = to_changes(root, &notes, &[]);
        assert_eq!(changes.len(), 1);
        assert!(matches!(
            &changes[0],
            Change::Renamed { from, to, .. }
                if from == Path::new(r"C:\Data\draft.md") && to == Path::new(r"C:\Data\final.md")
        ));
    }

    /// A move out of the watched tree gives an old name and no new one. That is a
    /// removal here, not a dropped event.
    #[test]
    fn v0_7_an_unpaired_old_name_is_a_removal() {
        let notes = vec![Notification {
            action: FILE_ACTION_RENAMED_OLD_NAME,
            name: "gone.md".into(),
        }];
        let changes = to_changes(Path::new(r"C:\Data"), &notes, &[]);
        assert_eq!(changes, vec![Change::Removed(PathBuf::from(r"C:\Data\gone.md"))]);
    }

    /// Names are all this index holds, so a write to an existing file is not an
    /// event. Under a build it is most of the traffic.
    #[test]
    fn v0_7_a_modification_is_not_an_index_change() {
        let notes = vec![Notification {
            action: FILE_ACTION_MODIFIED,
            name: "main.rs".into(),
        }];
        assert!(to_changes(Path::new(r"C:\Data"), &notes, &[]).is_empty());
    }

    /// Exclusions apply to events as they do to the walk, or an npm install
    /// refills the index with everything the walk was careful to skip.
    #[test]
    fn v0_7_events_under_an_excluded_directory_are_dropped() {
        let exclude = vec!["node_modules".to_string()];
        let notes = vec![
            Notification {
                action: FILE_ACTION_ADDED,
                name: r"node_modules\left-pad\index.js".into(),
            },
            Notification {
                action: FILE_ACTION_ADDED,
                name: "wanted.rs".into(),
            },
        ];
        let changes = to_changes(Path::new(r"C:\Data"), &notes, &exclude);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], Change::Added(p, _) if p.ends_with("wanted.rs")));
    }

    /// The root's own name is never the thing excluded — a root at `C:\build` is
    /// a deliberate choice, and the rule is about what lies beneath it.
    #[test]
    fn v0_7_the_roots_own_name_does_not_exclude_it() {
        let exclude = vec!["build".to_string()];
        let notes = vec![Notification {
            action: FILE_ACTION_ADDED,
            name: "thing.rs".into(),
        }];
        let changes = to_changes(Path::new(r"C:\build"), &notes, &exclude);
        assert_eq!(changes.len(), 1);
    }
}
