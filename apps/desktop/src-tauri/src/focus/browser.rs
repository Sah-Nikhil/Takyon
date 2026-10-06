//! Driving a browser's own UI through UI Automation (ADR-0035).
//!
//! Address bar: Chromium by `ClassName` `OmniboxViewViews`, Gecko by
//! `AutomationId` `urlbar-input` (spike result). Run on an MTA thread; the
//! shell walk's `ComScope` is apartment-threaded and wrong for this.

use std::path::Path;

/// Which address-bar lookup a browser takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    Chromium,
    Gecko,
}

/// Engine of a supported browser, by image name; None = refuse in words.
///
/// Image name is not identity (Helium ships as `chrome.exe`) but the lookup
/// is per engine, so it is enough. UI copy names a browser by `ProductName`.
pub fn engine_of(image: &Path) -> Option<Engine> {
    let name = image.file_name()?.to_string_lossy().to_lowercase();
    match name.as_str() {
        "chrome.exe" | "msedge.exe" | "brave.exe" | "vivaldi.exe" | "opera.exe" | "arc.exe" => {
            Some(Engine::Chromium)
        }
        "firefox.exe" | "zen.exe" | "librewolf.exe" | "waterfox.exe" | "floorp.exe" => {
            Some(Engine::Gecko)
        }
        _ => None,
    }
}

/// What a browser's address bar says right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Bar {
    /// Displayed text, one of the four shapes `site::host_of` reads.
    Url(String),
    /// User is typing, or Gecko's element went stale: skip this tick.
    Busy,
    /// No address bar in the tree: a modal or first-run screen.
    Missing,
}

/// Tab title the restore walk looks for. Must match `focus/blocked.html`.
pub const BLOCK_TITLE: &str = "Blocked · Takyon";

#[cfg(windows)]
pub use uia::{mta, MtaScope, Uia};

#[cfg(windows)]
mod uia {
    use std::cell::RefCell;
    use std::time::Duration;

    use windows::core::BSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationLegacyIAccessiblePattern,
        IUIAutomationSelectionItemPattern, IUIAutomationValuePattern, TreeScope_Descendants,
        UIA_AutomationIdPropertyId, UIA_ClassNamePropertyId, UIA_ControlTypePropertyId,
        UIA_LegacyIAccessiblePatternId, UIA_SelectionItemPatternId, UIA_TabItemControlTypeId,
        UIA_ValuePatternId,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VK_RETURN,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    use super::{Bar, Engine, BLOCK_TITLE};
    use crate::focus::site::original_from;

    /// COM initialised multithreaded for the scope. UIA clients belong on MTA.
    pub struct MtaScope(bool);

    pub fn mta() -> MtaScope {
        MtaScope(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok())
    }

    impl Drop for MtaScope {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    /// A UIA client. One thread; caches the last address bar found.
    pub struct Uia {
        automation: IUIAutomation,
        cached: RefCell<Option<(isize, IUIAutomationElement)>>,
    }

    fn hwnd(h: isize) -> HWND {
        HWND(h as *mut core::ffi::c_void)
    }

    fn in_front(h: isize) -> bool {
        let front = unsafe { GetForegroundWindow() };
        front == hwnd(h)
    }

    impl Uia {
        pub fn new() -> Option<Uia> {
            let automation =
                unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.ok()?;
            Some(Uia {
                automation,
                cached: RefCell::new(None),
            })
        }

        /// Address bar of window `h`. `fresh` skips the cache: Gecko's goes stale.
        fn bar(&self, h: isize, engine: Engine, fresh: bool) -> Option<IUIAutomationElement> {
            if !fresh {
                if let Some((cached_h, el)) = self.cached.borrow().as_ref() {
                    if *cached_h == h {
                        return Some(el.clone());
                    }
                }
            }
            let (property, value) = match engine {
                Engine::Chromium => (UIA_ClassNamePropertyId, "OmniboxViewViews"),
                Engine::Gecko => (UIA_AutomationIdPropertyId, "urlbar-input"),
            };
            let el = unsafe {
                let window = self.automation.ElementFromHandle(hwnd(h)).ok()?;
                let condition = self
                    .automation
                    .CreatePropertyCondition(property, &VARIANT::from(value))
                    .ok()?;
                window.FindFirst(TreeScope_Descendants, &condition).ok()?
            };
            *self.cached.borrow_mut() = Some((h, el.clone()));
            Some(el)
        }

        fn value(el: &IUIAutomationElement) -> Option<String> {
            unsafe {
                let vp: IUIAutomationValuePattern =
                    el.GetCurrentPatternAs(UIA_ValuePatternId).ok()?;
                vp.CurrentValue().ok().map(|b| b.to_string())
            }
        }

        /// Read the bar. Never while it has keyboard focus: that is typing.
        pub fn read(&self, h: isize, engine: Engine) -> Bar {
            for fresh in [false, true] {
                let Some(el) = self.bar(h, engine, fresh) else {
                    continue;
                };
                let focused = unsafe { el.CurrentHasKeyboardFocus() }.map(|b| b.as_bool());
                match (focused, Self::value(&el)) {
                    (Ok(true), _) => return Bar::Busy,
                    (Ok(false), Some(v)) if !v.is_empty() => return Bar::Url(v),
                    (Ok(false), Some(_)) => {
                        self.cached.borrow_mut().take();
                        return Bar::Busy;
                    }
                    _ => {
                        self.cached.borrow_mut().take();
                    }
                }
            }
            Bar::Missing
        }

        /// Type `url` into the bar and press Enter. Only with `h` in front, and
        /// only once the bar holds focus, or the key lands somewhere else.
        ///
        /// Retried: a window activated a moment ago takes ~0.5 s to give the
        /// bar focus (Helium in the live test).
        pub fn navigate(&self, h: isize, engine: Engine, url: &str) -> bool {
            for _ in 0..8 {
                if !in_front(h) {
                    return false;
                }
                let Some(el) = self.bar(h, engine, true) else {
                    return false;
                };
                let set = unsafe {
                    el.SetFocus().is_ok()
                        && el
                            .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                            .and_then(|vp| vp.SetValue(&BSTR::from(url)))
                            .is_ok()
                };
                std::thread::sleep(Duration::from_millis(150));
                let focused = unsafe { el.CurrentHasKeyboardFocus() }.is_ok_and(|b| b.as_bool());
                if set && focused && in_front(h) {
                    press_enter();
                    return true;
                }
            }
            false
        }

        /// Send every `Blocked · Takyon` tab in `h` back where it came from.
        /// Switches tabs, so only with `h` in front. Returns tabs restored.
        pub fn restore(&self, h: isize, engine: Engine) -> usize {
            if !in_front(h) {
                return 0;
            }
            let mut restored = 0;
            for tab in self.blocked_tabs(h) {
                if !select(&tab) {
                    continue;
                }
                std::thread::sleep(Duration::from_millis(400));
                let shown = self.bar(h, engine, true).and_then(|el| Self::value(&el));
                if let Some(url) = shown.and_then(|v| original_from(&v)) {
                    if self.navigate(h, engine, &url) {
                        restored += 1;
                        std::thread::sleep(Duration::from_millis(300));
                    }
                }
            }
            restored
        }

        fn blocked_tabs(&self, h: isize) -> Vec<IUIAutomationElement> {
            let tab_type = VARIANT::from(UIA_TabItemControlTypeId.0);
            unsafe {
                let Ok(window) = self.automation.ElementFromHandle(hwnd(h)) else {
                    return Vec::new();
                };
                let Ok(condition) = self
                    .automation
                    .CreatePropertyCondition(UIA_ControlTypePropertyId, &tab_type)
                else {
                    return Vec::new();
                };
                let Ok(tabs) = window.FindAll(TreeScope_Descendants, &condition) else {
                    return Vec::new();
                };
                (0..tabs.Length().unwrap_or(0))
                    .filter_map(|i| tabs.GetElement(i).ok())
                    .filter(|t| {
                        t.CurrentName()
                            .is_ok_and(|n| n.to_string().starts_with(BLOCK_TITLE))
                    })
                    .collect()
            }
        }
    }

    /// Switch to a tab. SelectionItem worked in all five browsers tested;
    /// Legacy as a fallback for forks.
    fn select(tab: &IUIAutomationElement) -> bool {
        unsafe {
            if let Ok(sel) = tab.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(
                UIA_SelectionItemPatternId,
            ) {
                return sel.Select().is_ok();
            }
            tab.GetCurrentPatternAs::<IUIAutomationLegacyIAccessiblePattern>(
                UIA_LegacyIAccessiblePatternId,
            )
            .and_then(|la| la.DoDefaultAction())
            .is_ok()
        }
    }

    fn press_enter() {
        let key = |up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_RETURN,
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
        unsafe {
            SendInput(
                &[key(false), key(true)],
                std::mem::size_of::<INPUT>() as i32,
            )
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_16_supported_browsers_are_known_by_image_name() {
        for (image, want) in [
            (
                r"C:\Program Files\Google\Chrome\Application\chrome.exe",
                Engine::Chromium,
            ),
            (
                r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                Engine::Chromium,
            ),
            (
                r"C:\Program Files\imput\Helium\Application\chrome.exe",
                Engine::Chromium,
            ),
            (
                r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe",
                Engine::Chromium,
            ),
            (r"C:\x\vivaldi.exe", Engine::Chromium),
            (r"C:\x\opera.exe", Engine::Chromium),
            (r"C:\x\Arc.exe", Engine::Chromium),
            (
                r"C:\Program Files\WindowsApps\Mozilla.Firefox_157.0.0.0_x64__n80bbvh6b1yt2\VFS\ProgramFiles\Firefox Package Root\firefox.exe",
                Engine::Gecko,
            ),
            (r"C:\ZenBrowser\zen.exe", Engine::Gecko),
            (r"C:\x\librewolf.exe", Engine::Gecko),
            (r"C:\x\waterfox.exe", Engine::Gecko),
            (r"C:\x\floorp.exe", Engine::Gecko),
        ] {
            assert_eq!(engine_of(Path::new(image)), Some(want), "{image}");
        }
        for image in [
            r"C:\x\discord.exe",
            r"C:\x\iexplore.exe",
            r"C:\x\msedgewebview2.exe",
            r"C:\x\chrome_proxy.exe",
        ] {
            assert_eq!(engine_of(Path::new(image)), None, "{image}");
        }
    }
}
