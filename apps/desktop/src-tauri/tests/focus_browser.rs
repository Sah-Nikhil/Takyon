//! Focus website blocking against real installed browsers (v0.16, ADR-0035).
//!
//! `--ignored`: launches each browser this machine has, on a throwaway
//! profile, loads example.com (network), blocks it, opens a second tab, and
//! restores the first. CI's daily job runs it so a browser update that moves
//! the address bar is caught within a day. A Gecko profile showing a modal
//! (welcome, terms) has no address bar in its tree: skipped, said so.
//!
//! Safe on a desktop in use: waits for 20 s with no input, stops once anyone
//! else types or moves the mouse, and sends only navigate's Enter into its own
//! browser, in front. Busy desktop: a skip off CI, a failure on it.
//!
//! cargo test --test focus_browser -- --ignored --nocapture

#![cfg(windows)]

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use common::TempDir;
use takyon_lib::focus::browser::{engine_of, mta, Bar, Engine, Uia};
use takyon_lib::focus::site::{block_url, host_of, is_block_page};
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    IsWindowVisible, PostMessageW, SetForegroundWindow, WM_CLOSE,
};

/// `www.` on purpose: Chromium's bar hides it, so only the page's own
/// address (`Uia::page_url`) can bring it back exactly.
const ORIGINAL: &str = "https://www.example.com/takyon-live?x=1";

/// Tick of the last keyboard or mouse input, ours included.
fn last_input() -> u32 {
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    unsafe {
        let _ = GetLastInputInfo(&mut info);
    }
    info.dwTime
}

/// Wait up to a minute for 20 s with no input. False: someone is using it.
fn wait_idle() -> bool {
    poll(Duration::from_secs(60), || {
        let idle = unsafe { GetTickCount() }.wrapping_sub(last_input());
        (idle >= 20_000).then_some(())
    })
    .is_some()
}

/// The input tick as of our last action: any later input was a person.
struct Guard(u32);

impl Guard {
    fn check(&self) -> Result<(), Skip> {
        if last_input() == self.0 {
            Ok(())
        } else {
            Err(Skip(BUSY.into()))
        }
    }

    /// After an action of ours that injects input (navigate's Enter).
    fn ours(&mut self) {
        self.0 = last_input();
    }
}

const BUSY: &str = "desktop in use";

#[test]
#[ignore = "launches real browsers and loads example.com"]
fn v0_16_live_block_and_restore_in_every_installed_browser() {
    let ci = std::env::var_os("CI").is_some();
    if !wait_idle() {
        assert!(!ci, "a CI runner's desktop should be idle");
        eprintln!("[skip] {BUSY}: no 20 s without input; nothing launched");
        return;
    }
    let _com = mta();
    let uia = Uia::new().expect("UI Automation");
    let page = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("focus")
        .join("blocked.html");
    let mut passed = Vec::new();
    let only = std::env::var("TAKYON_LIVE_ONLY").unwrap_or_default();
    for exe in installed_browsers()
        .into_iter()
        .filter(|e| e.to_string_lossy().contains(&only))
    {
        let engine = engine_of(&exe).expect("candidate is a supported browser");
        let name = exe.display().to_string();
        let profile = TempDir::new(&format!("focus-live-{}", engine_label(&exe)));
        // Declared after `profile`, so dropped first: a panic still closes it.
        let _cleanup = Cleanup(profile.path().to_path_buf());
        let before: Vec<isize> = top_windows().into_iter().map(|(h, _, _)| h).collect();
        let mut child = launch(&exe, engine, profile.path());
        let Some(hwnd) = wait_window(engine, &before, Duration::from_secs(30)) else {
            eprintln!("[skip] {name}: no window with the page");
            kill_profile(profile.path());
            let _ = child.kill();
            let _ = child.wait();
            continue;
        };
        let outcome = drive(&uia, hwnd, engine, &exe, profile.path(), &page);
        close(hwnd);
        kill_profile(profile.path());
        let _ = child.kill();
        let _ = child.wait();
        match outcome {
            Ok(()) => passed.push(name),
            Err(Skip(why)) if why == BUSY && !ci => {
                eprintln!("[skip] {name}: {why}; stopping");
                return;
            }
            Err(Skip(why)) => eprintln!("[skip] {name}: {why}"),
        }
    }
    eprintln!("[pass] {passed:?}");
    assert!(!passed.is_empty(), "no browser could be driven");
}

struct Skip(String);

/// Ends every process on a throwaway profile when dropped, panics included.
struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        kill_profile(&self.0);
    }
}

fn engine_label(exe: &Path) -> String {
    exe.to_string_lossy()
        .bytes()
        .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32))
        .to_string()
}

/// Read, block, restore. Panics on a real failure; Skip for a modal, or for
/// a person at the machine.
fn drive(
    uia: &Uia,
    hwnd: isize,
    engine: Engine,
    exe: &Path,
    profile: &Path,
    page: &Path,
) -> Result<(), Skip> {
    let mut guard = Guard(last_input());
    let first = poll(Duration::from_secs(8), || match uia.read(hwnd, engine) {
        Bar::Url(url) => Some(url),
        _ => None,
    })
    .ok_or_else(|| Skip("address bar unreadable (modal or first-run screen)".into()))?;
    assert!(
        host_of(&first).is_some_and(|h| h.ends_with("example.com")),
        "read {first:?}"
    );
    // Gecko's bar shows the full URL already; Chromium needs the document.
    let address = uia.page_url(hwnd, &first);
    assert!(
        address.as_deref().map_or(true, |p| p == ORIGINAL),
        "page {address:?}"
    );
    if engine == Engine::Chromium {
        assert_eq!(
            address.as_deref(),
            Some(ORIGINAL),
            "Chromium's page address"
        );
    }

    front(hwnd, &guard)?;
    assert!(uia.navigate(hwnd, engine, &block_url(page, ORIGINAL, 0, "Live test")));
    guard.ours();
    let blocked = poll(Duration::from_secs(8), || match uia.read(hwnd, engine) {
        Bar::Url(url) if is_block_page(&url) => Some(url),
        _ => None,
    });
    assert!(blocked.is_some(), "tab never reached the block page");

    // A second tab without a keystroke: the browser hands the URL to its
    // running instance on the same profile, which opens it in front.
    second_tab(exe, engine, profile);
    let second = poll(Duration::from_secs(10), || match uia.read(hwnd, engine) {
        Bar::Url(url) if host_of(&url).as_deref() == Some("example.org") => Some(url),
        _ => None,
    });
    assert!(second.is_some(), "second tab never opened");

    // Retried like the runtime's walk: a strip read mid-switch shows no tabs.
    let mut restored = 0;
    for _ in 0..3 {
        front(hwnd, &guard)?;
        restored = uia.restore(hwnd, engine);
        guard.ours();
        if restored > 0 {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    if restored != 1 {
        eprintln!(
            "front={} bar={:?} hwnd={hwnd}",
            is_foreground(hwnd),
            uia.read(hwnd, engine)
        );
        for w in top_windows()
            .iter()
            .filter(|(_, c, _)| c.starts_with("Chrome") || c.starts_with("Mozilla"))
        {
            eprintln!("  window {w:?}");
        }
    }
    assert_eq!(restored, 1, "one blocked tab restored");
    let back = poll(Duration::from_secs(10), || match uia.read(hwnd, engine) {
        Bar::Url(url) if host_of(&url).is_some_and(|h| h.ends_with("example.com")) => Some(url),
        _ => None,
    });
    assert!(back.is_some(), "restored tab is not back on example.com");
    if engine == Engine::Chromium {
        let shown = back.unwrap();
        assert_eq!(
            uia.page_url(hwnd, &shown).as_deref(),
            Some(ORIGINAL),
            "restored exactly"
        );
    }
    Ok(())
}

fn poll<T>(timeout: Duration, mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let start = Instant::now();
    loop {
        if let Some(v) = f() {
            return Some(v);
        }
        if start.elapsed() > timeout {
            return None;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// Raise the browser for a step that types, or say why not. Never forces it:
/// a refused raise is the foreground lock, which means a person is about.
fn front(hwnd: isize, guard: &Guard) -> Result<(), Skip> {
    guard.check()?;
    unsafe {
        let _ = SetForegroundWindow(HWND(hwnd as *mut _));
    }
    poll(Duration::from_secs(3), || is_foreground(hwnd).then_some(()))
        .ok_or_else(|| Skip(format!("{BUSY}: Windows kept the foreground elsewhere")))?;
    guard.check()
}

fn second_tab(exe: &Path, engine: Engine, profile: &Path) {
    const SECOND: &str = "https://example.org/takyon-live-2";
    let mut cmd = Command::new(exe);
    match engine {
        Engine::Gecko => cmd.arg("-profile").arg(profile).args(["-new-tab", SECOND]),
        Engine::Chromium => cmd
            .arg(format!("--user-data-dir={}", profile.display()))
            .arg(SECOND),
    };
    let _ = cmd.spawn().map(|mut c| c.wait());
}

/// Every supported browser registered on this machine, plus Store Firefox.
fn installed_browsers() -> Vec<PathBuf> {
    let reg = Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Clients\StartMenuInternet",
            "/s",
            "/ve",
        ])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let store_firefox = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            "(Get-AppxPackage Mozilla.Firefox).InstallLocation",
        ])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let mut out: Vec<PathBuf> = reg
        .lines()
        .filter_map(|l| l.split("REG_SZ").nth(1))
        .map(|v| PathBuf::from(v.trim().trim_matches('"')))
        .chain((!store_firefox.is_empty()).then(|| {
            Path::new(&store_firefox).join(r"VFS\ProgramFiles\Firefox Package Root\firefox.exe")
        }))
        .filter(|p| p.exists() && engine_of(p).is_some())
        .collect();
    out.sort();
    out.dedup();
    out
}

fn launch(exe: &Path, engine: Engine, profile: &Path) -> std::process::Child {
    let mut cmd = Command::new(exe);
    match engine {
        Engine::Gecko => {
            std::fs::write(
                profile.join("user.js"),
                "user_pref(\"browser.shell.checkDefaultBrowser\", false);\n",
            )
            .unwrap();
            cmd.args(["-no-remote", "-profile"])
                .arg(profile)
                .args(["-new-window", ORIGINAL]);
        }
        Engine::Chromium => {
            cmd.arg(format!("--user-data-dir={}", profile.display()))
                .args([
                    "--no-first-run",
                    "--no-default-browser-check",
                    "--new-window",
                    ORIGINAL,
                ]);
        }
    }
    cmd.spawn().expect("browser starts")
}

/// The new browser window showing example.com: not in `before`, so a window
/// left by an earlier run, titled the same, is never picked.
fn wait_window(engine: Engine, before: &[isize], timeout: Duration) -> Option<isize> {
    let class = match engine {
        Engine::Chromium => "Chrome_WidgetWin_1",
        Engine::Gecko => "MozillaWindowClass",
    };
    poll(timeout, || {
        top_windows()
            .into_iter()
            .find(|(h, c, t)| !before.contains(h) && c == class && t.contains("Example Domain"))
            .map(|(h, _, _)| h)
    })
}

/// Every process run on the throwaway profile. Browsers hand off to brokers
/// and relaunch themselves, so the spawned pid's tree is not all of them.
fn kill_profile(profile: &Path) {
    let script = format!(
        "Get-CimInstance Win32_Process | Where-Object {{ $_.CommandLine -like '*{}*' }} | \
         ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }}",
        profile.display()
    );
    let _ = Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", &script])
        .output();
}

fn top_windows() -> Vec<(isize, String, String)> {
    unsafe extern "system" fn each(hwnd: HWND, lp: LPARAM) -> BOOL {
        let out = &mut *(lp.0 as *mut Vec<(isize, String, String)>);
        if IsWindowVisible(hwnd).as_bool() {
            let mut buf = [0u16; 512];
            let n = GetClassNameW(hwnd, &mut buf) as usize;
            let class = String::from_utf16_lossy(&buf[..n]);
            let n = GetWindowTextW(hwnd, &mut buf) as usize;
            out.push((hwnd.0 as isize, class, String::from_utf16_lossy(&buf[..n])));
        }
        BOOL(1)
    }
    let mut out = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut out as *mut _ as isize));
    }
    out
}

fn is_foreground(hwnd: isize) -> bool {
    unsafe { GetForegroundWindow().0 as isize == hwnd }
}

fn close(hwnd: isize) {
    let hwnd = HWND(hwnd as *mut _);
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
    std::thread::sleep(Duration::from_secs(2));
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output();
}
