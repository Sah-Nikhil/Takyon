//! The floating bar: goal and time left, always on top, never takes focus.
//!
//! Built when a session starts and destroyed when it ends; never kept hidden
//! between sessions, so idle RSS does not carry a second webview. Built off
//! the main thread (CLAUDE.md: never create a window from the main thread).

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::prefs::{self, Prefs};

pub const LABEL: &str = "focusbar";

/// Logical size. Wide enough for a goal of a few words and the countdown.
const WIDTH: f64 = 360.0;
const HEIGHT: f64 = 52.0;

/// Show (building if absent) or close the bar. Never blocks the caller.
pub fn set(app: &AppHandle, shown: bool) {
    let app = app.clone();
    std::thread::spawn(move || {
        let existing = app.get_webview_window(LABEL);
        match (shown, existing) {
            (true, None) => build(&app),
            (false, Some(window)) => {
                let _ = window.destroy();
            }
            _ => {}
        }
    });
}

/// `"x,y"` physical pixels, as stored by [`build`]'s move handler.
pub fn parse_position(value: Option<&str>) -> Option<(i32, i32)> {
    let (x, y) = value?.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

fn build(app: &AppHandle) {
    let prefs = app.state::<std::sync::Arc<Prefs>>().inner().clone();
    let builder =
        WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?window=bar".into()))
            .title("Takyon Focus")
            .inner_size(WIDTH, HEIGHT)
            .resizable(false)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .shadow(false)
            .visible(false);
    // Transparent like the Palette: the bar draws its own rounded panel. macOS
    // needs Tauri's private-API feature for this; v0.12's port decides that.
    #[cfg(windows)]
    let builder = builder.transparent(true);
    let window = match builder.build() {
        Ok(w) => w,
        Err(e) => return eprintln!("[takyon] the Focus bar could not be built: {e}"),
    };
    match parse_position(prefs.get(prefs::FOCUS_BAR_POSITION).as_deref()) {
        Some((x, y)) => {
            let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
        }
        None => place_top_centre(&window),
    }
    no_activate(&window);
    let _ = window.show();
    let store = prefs.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Moved(p) = event {
            let _ = store.set(prefs::FOCUS_BAR_POSITION, &format!("{},{}", p.x, p.y));
        }
    });
}

fn place_top_centre(window: &tauri::WebviewWindow) {
    let Ok(Some(monitor)) = window.primary_monitor() else {
        return;
    };
    let scale = monitor.scale_factor();
    let width = (WIDTH * scale) as i32;
    let x = monitor.position().x + (monitor.size().width as i32 - width) / 2;
    let y = monitor.position().y + (16.0 * scale) as i32;
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

/// `WS_EX_NOACTIVATE` and `WS_EX_TOOLWINDOW`: clicks land, focus stays where
/// the user is typing, no Alt+Tab entry. Not a Tauri option, hence raw Win32.
#[cfg(windows)]
fn no_activate(window: &tauri::WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };
    let Ok(hwnd) = window.hwnd() else { return };
    let hwnd = HWND(hwnd.0);
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let wanted = style | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted);
    }
}

#[cfg(not(windows))]
fn no_activate(_: &tauri::WebviewWindow) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_16_a_stored_bar_position_reads_back() {
        assert_eq!(parse_position(Some("120,-8")), Some((120, -8)));
        assert_eq!(parse_position(Some(" 5 , 6 ")), Some((5, 6)));
        assert_eq!(parse_position(Some("x,6")), None);
        assert_eq!(parse_position(None), None);
    }
}
