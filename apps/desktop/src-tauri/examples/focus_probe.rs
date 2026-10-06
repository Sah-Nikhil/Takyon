//! What `scripts/verify-drive-v0.16.ts` sees of the desktop: windows, the
//! foreground, a browser's address bar and tabs, and Takyon's shutdown hold.
//! One JSON value per call on stdout.
//!
//! cargo run --example focus_probe -- <command> [args]

#[cfg(windows)]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).cloned().unwrap_or_default();
    let num = |i: usize| arg(i).parse::<isize>().unwrap_or(0);
    let out = match arg(0).as_str() {
        "windows" => win::windows(),
        "front" => win::front(),
        "raise" => win::raise(num(1)),
        "move" => win::move_to(num(1), num(2) as i32, num(3) as i32),
        "omnibox" => win::omnibox(num(1)),
        "tabs" => win::tabs(num(1)),
        "hold" => win::hold(num(1) as u32),
        "idle" => win::idle(),
        other => format!("{{\"error\":\"unknown command {other:?}\"}}"),
    };
    println!("{out}");
}

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
mod win {
    use takyon_lib::focus::browser::{engine_of, mta, Bar, Uia, BLOCK_TITLE};
    use takyon_lib::focus::procs;
    use windows::core::{BOOL, PWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
    use windows::Win32::System::Shutdown::ShutdownBlockReasonQuery;
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect,
        GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, SendMessageTimeoutW,
        SetForegroundWindow, SetWindowPos, GWL_EXSTYLE, SMTO_ABORTIFHUNG, SWP_NOACTIVATE,
        SWP_NOSIZE, SWP_NOZORDER, WM_QUERYENDSESSION,
    };

    fn json(s: &str) -> String {
        serde_json::to_string(s).unwrap()
    }

    fn info(hwnd: HWND) -> (u32, String, String) {
        let mut buf = [0u16; 512];
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let n = GetClassNameW(hwnd, &mut buf) as usize;
            let class = String::from_utf16_lossy(&buf[..n]);
            let n = GetWindowTextW(hwnd, &mut buf) as usize;
            (pid, class, String::from_utf16_lossy(&buf[..n]))
        }
    }

    fn all() -> Vec<HWND> {
        unsafe extern "system" fn each(hwnd: HWND, lp: LPARAM) -> BOOL {
            unsafe { (*(lp.0 as *mut Vec<HWND>)).push(hwnd) };
            BOOL(1)
        }
        let mut out: Vec<HWND> = Vec::new();
        unsafe {
            let _ = EnumWindows(Some(each), LPARAM(&mut out as *mut _ as isize));
        }
        out
    }

    /// Every top-level window, visible or not, with pid, class, title, rect.
    pub fn windows() -> String {
        let rows: Vec<String> = all()
            .into_iter()
            .map(|h| {
                let (pid, class, title) = info(h);
                let mut r = RECT::default();
                let ex = unsafe {
                    let _ = GetWindowRect(h, &mut r);
                    GetWindowLongPtrW(h, GWL_EXSTYLE)
                };
                let visible = unsafe { IsWindowVisible(h).as_bool() };
                format!(
                    "{{\"hwnd\":{},\"pid\":{pid},\"class\":{},\"title\":{},\"visible\":{visible},\"x\":{},\"y\":{},\"w\":{},\"h\":{},\"ex\":{}}}",
                    h.0 as isize,
                    json(&class),
                    json(&title),
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    ex
                )
            })
            .collect();
        format!("[{}]", rows.join(","))
    }

    pub fn front() -> String {
        let h = unsafe { GetForegroundWindow() };
        let (pid, class, title) = info(h);
        format!(
            "{{\"hwnd\":{},\"pid\":{pid},\"class\":{},\"title\":{}}}",
            h.0 as isize,
            json(&class),
            json(&title)
        )
    }

    pub fn raise(hwnd: isize) -> String {
        let h = HWND(hwnd as *mut _);
        unsafe {
            let _ = SetForegroundWindow(h);
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
        let ok = unsafe { GetForegroundWindow() } == h;
        format!("{{\"front\":{ok}}}")
    }

    pub fn move_to(hwnd: isize, x: i32, y: i32) -> String {
        let ok = unsafe {
            SetWindowPos(
                HWND(hwnd as *mut _),
                None,
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
            .is_ok()
        };
        format!("{{\"moved\":{ok}}}")
    }

    fn engine(hwnd: isize) -> Option<takyon_lib::focus::browser::Engine> {
        let (pid, _, _) = info(HWND(hwnd as *mut _));
        procs::describe(pid).and_then(|p| engine_of(&p.image))
    }

    pub fn omnibox(hwnd: isize) -> String {
        let _com = mta();
        let (Some(uia), Some(engine)) = (Uia::new(), engine(hwnd)) else {
            return "{\"bar\":null}".into();
        };
        match uia.read(hwnd, engine) {
            Bar::Url(u) => format!("{{\"bar\":{}}}", json(&u)),
            Bar::Busy => "{\"bar\":\"<busy>\"}".into(),
            Bar::Missing => "{\"bar\":null}".into(),
        }
    }

    /// Tab names in the strip, by walking for tab items.
    pub fn tabs(hwnd: isize) -> String {
        use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
        use windows::Win32::System::Variant::VARIANT;
        use windows::Win32::UI::Accessibility::{
            CUIAutomation, IUIAutomation, TreeScope_Descendants, UIA_ControlTypePropertyId,
            UIA_TabItemControlTypeId,
        };
        let _com = mta();
        let names: Vec<String> = (|| unsafe {
            let uia: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
            let root = uia.ElementFromHandle(HWND(hwnd as *mut _)).ok()?;
            let cond = uia
                .CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &VARIANT::from(UIA_TabItemControlTypeId.0),
                )
                .ok()?;
            let found = root.FindAll(TreeScope_Descendants, &cond).ok()?;
            let n = found.Length().ok()?;
            Some(
                (0..n)
                    .filter_map(|i| found.GetElement(i).ok()?.CurrentName().ok())
                    .map(|b| b.to_string())
                    .collect(),
            )
        })()
        .unwrap_or_default();
        let blocked = names.iter().filter(|n| n.contains(BLOCK_TITLE)).count();
        let list: Vec<String> = names.iter().map(|n| json(n)).collect();
        format!("{{\"tabs\":[{}],\"blocked\":{blocked}}}", list.join(","))
    }

    /// Takyon's hidden Focus window for `pid`: does it refuse a shutdown
    /// query, and what reason does it give Windows?
    pub fn hold(pid: u32) -> String {
        let Some(h) = all().into_iter().find(|&h| {
            let (p, class, _) = info(h);
            p == pid && class == "TakyonFocus"
        }) else {
            return "{\"window\":false}".into();
        };
        let mut answer = 0usize;
        unsafe {
            SendMessageTimeoutW(
                h,
                WM_QUERYENDSESSION,
                WPARAM(0),
                LPARAM(0),
                SMTO_ABORTIFHUNG,
                2_000,
                Some(&mut answer),
            );
        }
        let mut buf = [0u16; 256];
        let mut len = buf.len() as u32;
        let reason = unsafe {
            ShutdownBlockReasonQuery(h, Some(PWSTR(buf.as_mut_ptr())), &mut len)
                .ok()
                .map(|_| {
                    let end = buf.iter().position(|&c| c == 0).unwrap_or(0);
                    String::from_utf16_lossy(&buf[..end])
                })
        };
        format!(
            "{{\"window\":true,\"allows\":{},\"reason\":{}}}",
            answer != 0,
            reason.map_or("null".into(), |r| json(&r))
        )
    }

    pub fn idle() -> String {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        unsafe {
            let _ = GetLastInputInfo(&mut info);
            format!(
                "{{\"idleMs\":{}}}",
                GetTickCount().wrapping_sub(info.dwTime)
            )
        }
    }
}
