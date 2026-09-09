//! Sum the memory of a process and everything below it, as JSON.
//!
//! The webview is not one process. On Windows a single Takyon runs a Rust host,
//! a WebView2 browser process, a renderer and a GPU process — and the renderer
//! is a child of the browser process, not of us, so a one-level walk misses it.
//! Measuring only the main process would report roughly the Rust binary's
//! footprint and quietly claim the 150 MB budget was met with room to spare.
//!
//! macOS is the same shape with a worse hazard, and `untrackedWebKitHelpers`
//! exists to measure it rather than assume it — see `webkit_outside_tree`.

use serde_json::{json, Value};
use std::collections::{HashSet, VecDeque};

/// One process, as both arms report it.
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub working_set: u64,
    pub private_bytes: u64,
}

/// Every pid reachable downward from `root`, `root` included.
///
/// Breadth-first with a visited set: pids are recycled, so a snapshot can
/// contain a cycle and walking one naively never returns.
pub fn descendants(all: &[Proc], root: u32) -> HashSet<u32> {
    let mut seen = HashSet::new();
    seen.insert(root);
    let mut frontier = VecDeque::from([root]);
    while let Some(parent) = frontier.pop_front() {
        for p in all.iter().filter(|p| p.ppid == parent) {
            if seen.insert(p.pid) {
                frontier.push_back(p.pid);
            }
        }
    }
    seen
}

/// Every pid whose executable name matches `stem`, ignoring case and extension.
///
/// The harness needs this before it spawns anything: `single-instance` makes a
/// second Takyon hand off and exit, so a bench run against an already-running
/// copy measures a process that died immediately and blames the hotkey.
pub fn pids_named(all: &[Proc], stem: &str) -> Vec<u32> {
    all.iter()
        .filter(|p| {
            std::path::Path::new(&p.name)
                .file_stem()
                .map(|s| s.eq_ignore_ascii_case(stem))
                .unwrap_or(false)
        })
        .map(|p| p.pid)
        .collect()
}

/// Roll the tree up into the JSON `scripts/bench.ts` reads.
pub fn summarise(all: &[Proc], root: u32, extra: Value) -> Value {
    let seen = descendants(all, root);
    let counted: Vec<&Proc> = all.iter().filter(|p| seen.contains(&p.pid)).collect();
    let mut out = json!({
        "processes": counted.len(),
        "workingSet": counted.iter().map(|p| p.working_set).sum::<u64>(),
        "privateBytes": counted.iter().map(|p| p.private_bytes).sum::<u64>(),
        "breakdown": counted
            .iter()
            .map(|p| json!({ "pid": p.pid, "name": p.name, "workingSet": p.working_set }))
            .collect::<Vec<_>>(),
    });
    if let (Some(o), Some(e)) = (out.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    out
}

#[cfg(windows)]
pub fn tree(root: u32) -> Result<Value, String> {
    Ok(summarise(&snapshot()?, root, json!({})))
}

#[cfg(windows)]
fn snapshot() -> Result<Vec<Proc>, String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
    };

    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
            .map_err(|e| format!("CreateToolhelp32Snapshot failed: {e}"))?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut ok = Process32FirstW(snap, &mut entry).is_ok();
        while ok {
            let len = entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len());
            out.push(Proc {
                pid: entry.th32ProcessID,
                ppid: entry.th32ParentProcessID,
                name: String::from_utf16_lossy(&entry.szExeFile[..len]),
                working_set: 0,
                private_bytes: 0,
            });
            ok = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);

        // A process that exited between the snapshot and here reports zero
        // rather than failing the run, which is what the PowerShell it replaces
        // did with -ErrorAction SilentlyContinue.
        for p in out.iter_mut() {
            let Ok(handle) = OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
                false,
                p.pid,
            ) else {
                continue;
            };
            let mut counters = PROCESS_MEMORY_COUNTERS_EX::default();
            let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
            if GetProcessMemoryInfo(handle, &mut counters as *mut _ as *mut _, size).is_ok() {
                p.working_set = counters.WorkingSetSize as u64;
                p.private_bytes = counters.PrivateUsage as u64;
            }
            let _ = CloseHandle(handle);
        }
    }
    Ok(out)
}

#[cfg(target_os = "macos")]
pub fn tree(root: u32) -> Result<Value, String> {
    let all = snapshot()?;
    let strays = webkit_outside_tree(&all, root);
    // `privateBytes` has no macOS counterpart worth reporting: RSS is what `ps`
    // gives, and phys_footprint needs a task port. Flagged rather than faked, so
    // the harness does not print a committed-versus-resident gap of zero.
    let extra = json!({
        "privateBytesAvailable": false,
        "untrackedWebKitHelpers": strays,
    });
    Ok(summarise(&all, root, extra))
}

/// `ps` once, for the whole table. Shell is fine here: unlike input injection
/// there is no precision requirement, and libproc would need a task port to
/// report another process's memory at all.
#[cfg(target_os = "macos")]
fn snapshot() -> Result<Vec<Proc>, String> {
    let out = std::process::Command::new("/bin/ps")
        .args(["-axo", "pid=,ppid=,rss=,comm="])
        .output()
        .map_err(|e| format!("ps failed to start: {e}"))?;
    if !out.status.success() {
        return Err(format!("ps exited {}", out.status));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(parse_ps_line)
        .collect())
}

/// One `ps -axo pid=,ppid=,rss=,comm=` row. `comm` is a path and holds spaces,
/// so it is whatever remains after the three numbers.
#[cfg(target_os = "macos")]
fn parse_ps_line(line: &str) -> Option<Proc> {
    let mut it = line.split_whitespace();
    let pid = it.next()?.parse().ok()?;
    let ppid = it.next()?.parse().ok()?;
    let rss_kb: u64 = it.next()?.parse().ok()?;
    let name = it.collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return None;
    }
    Some(Proc {
        pid,
        ppid,
        name,
        working_set: rss_kb * 1024,
        private_bytes: 0,
    })
}

/// WebKit helpers on the machine that the tree walk does not reach.
///
/// WKWebView's content, networking and GPU processes are XPC services, and an
/// XPC service is reparented to launchd rather than staying our child. If this
/// is non-zero the macOS RSS figure is under-reported and the walk needs a
/// responsible-pid lookup. Reported rather than guessed: it decides whether the
/// 150 MB budget can be quoted on macOS at all.
#[cfg(target_os = "macos")]
fn webkit_outside_tree(all: &[Proc], root: u32) -> usize {
    let seen = descendants(all, root);
    all.iter()
        .filter(|p| p.name.contains("com.apple.WebKit") && !seen.contains(&p.pid))
        .count()
}

#[cfg(any(windows, target_os = "macos"))]
pub fn pids(stem: &str) -> Result<Value, String> {
    Ok(json!(pids_named(&snapshot()?, stem)))
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn tree(_root: u32) -> Result<Value, String> {
    Err("process-tree memory is implemented for Windows and macOS only".into())
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn pids(_stem: &str) -> Result<Value, String> {
    Err("process lookup is implemented for Windows and macOS only".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, ppid: u32, ws: u64) -> Proc {
        Proc {
            pid,
            ppid,
            name: format!("p{pid}"),
            working_set: ws,
            private_bytes: ws / 2,
        }
    }

    #[test]
    fn a_grandchild_is_counted() {
        // The WebView2 renderer's shape: a child of the browser process, not of
        // us. A one-level walk is exactly the bug this guards.
        let all = vec![p(1, 0, 10), p(2, 1, 20), p(3, 2, 30), p(9, 0, 99)];
        let seen = descendants(&all, 1);
        assert_eq!(seen.len(), 3);
        assert!(!seen.contains(&9));
    }

    #[test]
    fn a_cycle_in_the_table_terminates() {
        let all = vec![p(1, 2, 10), p(2, 1, 20)];
        assert_eq!(descendants(&all, 1).len(), 2);
    }

    #[test]
    fn totals_cover_the_tree_and_nothing_else() {
        let all = vec![p(1, 0, 10), p(2, 1, 20), p(9, 0, 99)];
        let v = summarise(&all, 1, json!({}));
        assert_eq!(v["processes"], 2);
        assert_eq!(v["workingSet"], 30);
        assert_eq!(v["privateBytes"], 15);
        assert_eq!(v["breakdown"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_name_matches_with_or_without_an_extension() {
        let named = |pid, name: &str| Proc {
            pid,
            ppid: 0,
            name: name.into(),
            working_set: 0,
            private_bytes: 0,
        };
        let all = vec![
            named(1, "takyon.exe"),
            named(2, "/Applications/Takyon.app/Contents/MacOS/takyon"),
            named(3, "takyon-bench.exe"),
            named(4, "explorer.exe"),
        ];
        assert_eq!(pids_named(&all, "takyon"), vec![1, 2]);
        assert!(pids_named(&all, "nothing").is_empty());
    }

    #[test]
    fn extra_fields_are_merged_in() {
        let all = vec![p(1, 0, 10)];
        let v = summarise(&all, 1, json!({ "privateBytesAvailable": false }));
        assert_eq!(v["privateBytesAvailable"], false);
        assert_eq!(v["processes"], 1);
    }

    #[test]
    fn an_unknown_root_reports_nothing_rather_than_failing() {
        let all = vec![p(1, 0, 10)];
        let v = summarise(&all, 404, json!({}));
        assert_eq!(v["processes"], 0);
        assert_eq!(v["workingSet"], 0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn ps_rows_parse_including_a_path_with_spaces() {
        let row = "  501   1  204800 /Applications/Some App.app/Contents/MacOS/Some App";
        let got = parse_ps_line(row).expect("row should parse");
        assert_eq!(got.pid, 501);
        assert_eq!(got.ppid, 1);
        assert_eq!(got.working_set, 204800 * 1024);
        assert!(got.name.ends_with("Some App"));
        assert!(parse_ps_line("PID PPID RSS COMM").is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_launchd_owned_webkit_helper_is_counted_as_untracked() {
        let mut all = vec![p(1, 0, 10), p(2, 1, 20)];
        all.push(Proc {
            pid: 7,
            ppid: 1,
            name: "/System/.../com.apple.WebKit.WebContent".into(),
            working_set: 300,
            private_bytes: 0,
        });
        // ppid 1 is launchd, not us, so it is outside the tree rooted at pid 1's
        // own subtree only because that subtree is what we walk.
        assert_eq!(webkit_outside_tree(&all, 2), 1);
        assert_eq!(webkit_outside_tree(&all, 1), 0);
    }
}
