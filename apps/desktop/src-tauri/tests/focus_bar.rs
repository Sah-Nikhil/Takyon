//! The floating bar's Win32 half (v0.16): shown without activation, kept out
//! of Alt+Tab. Driven on a plain window this test owns, because the bar itself
//! is a Tauri webview window and `MockRuntime` makes none. What a click on the
//! WebView2 inside does is Windows' call, below any message: hand run, §E.3.
//!
//! Active window, not foreground: the foreground lock decides whether a
//! background test may take the foreground at all, so it would pass a bar that
//! activates. A thread's active window has no such lock.

#![cfg(windows)]

use takyon_lib::focus::bar::float_quietly;
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowLongPtrW, IsWindowVisible,
    RegisterClassW, ShowWindow, GWL_EXSTYLE, SW_SHOW, WINDOW_EX_STYLE, WNDCLASSW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_POPUP,
};

unsafe extern "system" fn proc_(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(h, m, w, l) }
}

/// A popup like the bar: undecorated, top-level, hidden until shown.
fn popup() -> HWND {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let class = w!("TakyonFocusBarTest");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(proc_),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        });
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("takyon-focus-bar-test"),
            WS_POPUP,
            -2000,
            -2000,
            360,
            52,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap()
    }
}

#[test]
fn v0_16_the_bar_shows_without_becoming_the_active_window() {
    // Control: a plain show does activate, so the check below can fail.
    let control = popup();
    unsafe {
        let _ = ShowWindow(control, SW_SHOW);
        assert_eq!(GetActiveWindow(), control, "control did not activate");
        let _ = DestroyWindow(control);
    }

    let before = unsafe { GetActiveWindow() };
    let bar = popup();
    float_quietly(bar.0 as isize);
    unsafe {
        assert!(IsWindowVisible(bar).as_bool(), "not shown");
        assert_eq!(GetActiveWindow(), before, "the bar took activation");
        let ex = GetWindowLongPtrW(bar, GWL_EXSTYLE) as u32;
        assert_ne!(ex & WS_EX_NOACTIVATE.0, 0, "clicks would activate it");
        assert_ne!(ex & WS_EX_TOOLWINDOW.0, 0, "it would sit in Alt+Tab");
        let _ = DestroyWindow(bar);
    }
}
