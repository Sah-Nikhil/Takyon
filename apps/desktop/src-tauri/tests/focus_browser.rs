//! Focus website blocking against real installed browsers (v0.16, ADR-0035).
//!
//! `--ignored`: launches each browser this machine has, on a throwaway
//! profile, loads example.com (network), blocks it, opens a second tab, and
//! restores the first. CI's daily job runs it so a browser update that moves
//! the address bar is caught within a day. A Gecko profile showing a modal
//! (welcome, terms) has no address bar in its tree: skipped, said so.
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
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_CONTROL, VK_MENU,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    IsWindowVisible, PostMessageW, SetForegroundWindow, WM_CLOSE,
};

const ORIGINAL: &str = "https://example.com/takyon-live?x=1";

#[test]
#[ignore = "launches real browsers and loads example.com"]
fn v0_16_live_block_and_restore_in_every_installed_browser() {
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
        let before: Vec<isize> = top_windows().into_iter().map(|(h, _, _)| h).collect();
        let mut child = launch(&exe, engine, profile.path());
        let Some(hwnd) = wait_window(engine, &before, Duration::from_secs(30)) else {
            eprintln!("[skip] {name}: no window with the page");
            kill_profile(profile.path());
            let _ = child.kill();
            let _ = child.wait();
            continue;
        };
        let outcome = drive(&uia, hwnd, engine, &page);
        close(hwnd);
        kill_profile(profile.path());
        let _ = child.kill();
        let _ = child.wait();
        match outcome {
            Ok(()) => passed.push(name),
            Err(Skip(why)) => eprintln!("[skip] {name}: {why}"),
        }
    }
    eprintln!("[pass] {passed:?}");
    assert!(!passed.is_empty(), "no browser could be driven");
}

struct Skip(String);

fn engine_label(exe: &Path) -> String {
    exe.to_string_lossy()
        .bytes()
        .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32))
        .to_string()
}

/// Read, block, restore. Panics on a real failure; Skip only for a modal.
fn drive(uia: &Uia, hwnd: isize, engine: Engine, page: &Path) -> Result<(), Skip> {
    let first = poll(Duration::from_secs(8), || match uia.read(hwnd, engine) {
        Bar::Url(url) => Some(url),
        _ => None,
    })
    .ok_or_else(|| Skip("address bar unreadable (modal or first-run screen)".into()))?;
    assert_eq!(
        host_of(&first).as_deref(),
        Some("example.com"),
        "read {first:?}"
    );

    bring_front(hwnd);
    assert!(is_foreground(hwnd), "browser must be in front to navigate");
    assert!(uia.navigate(hwnd, engine, &block_url(page, ORIGINAL, 0, "Live test")));
    let blocked = poll(Duration::from_secs(8), || match uia.read(hwnd, engine) {
        Bar::Url(url) if is_block_page(&url) => Some(url),
        _ => None,
    });
    assert!(blocked.is_some(), "tab never reached the block page");

    bring_front(hwnd);
    assert!(
        is_foreground(hwnd),
        "lost the foreground; not sending Ctrl+T"
    );
    chord(&[VK_CONTROL, VIRTUAL_KEY(b'T' as u16)]);
    std::thread::sleep(Duration::from_millis(1200));
    bring_front(hwnd);
    assert!(uia.navigate(hwnd, engine, "https://example.org/takyon-live-2"));
    std::thread::sleep(Duration::from_millis(2500));

    bring_front(hwnd);
    let restored = uia.restore(hwnd, engine);
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
        Bar::Url(url) if host_of(&url).as_deref() == Some("example.com") => Some(url),
        _ => None,
    });
    assert!(back.is_some(), "restored tab is not back on example.com");
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

/// The test owns this window; Takyon itself never raises a browser.
fn bring_front(hwnd: isize) {
    poll(Duration::from_secs(5), || {
        // Foreground lock: a background process may raise a window only
        // right after it sent input. An Alt tap is that input.
        chord(&[VK_MENU]);
        unsafe {
            let _ = SetForegroundWindow(HWND(hwnd as *mut _));
        }
        is_foreground(hwnd).then_some(())
    });
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

fn chord(keys: &[VIRTUAL_KEY]) {
    let key = |vk: VIRTUAL_KEY, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let mut inputs: Vec<INPUT> = keys.iter().map(|k| key(*k, false)).collect();
    inputs.extend(keys.iter().rev().map(|k| key(*k, true)));
    unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
}
