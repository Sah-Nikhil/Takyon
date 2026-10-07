//! The floating bar: goal and time left, always on top, never takes focus.
//!
//! Built when a session starts and destroyed when it ends; never kept hidden
//! between sessions, so idle RSS does not carry a second webview. Built off
//! the main thread (CLAUDE.md: never create a window from the main thread).
//! Opens where it was last dragged, else centred at the top or bottom of the
//! work area (`focus.bar_place`). Never sits past [`MARGIN`] from any edge.

use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::prefs::{self, Prefs};

pub const LABEL: &str = "focusbar";

/// Logical size: goal, icon and countdown, nothing to spare.
const WIDTH: f64 = 300.0;
const HEIGHT: f64 = 40.0;
/// Logical gap kept to every work-area edge, the taskbar's included.
const MARGIN: f64 = 16.0;

/// Which edge of the screen the bar opens on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Top,
    Bottom,
}

impl Place {
    /// Stored value; anything but `"top"` is the default, bottom.
    pub fn parse(value: Option<&str>) -> Place {
        match value {
            Some("top") => Place::Top,
            _ => Place::Bottom,
        }
    }
}

/// A rectangle in physical pixels: a monitor's work area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

fn physical(logical: f64, scale: f64) -> i32 {
    (logical * scale).round() as i32
}

/// Where the bar opens: centred, [`MARGIN`] in from the chosen edge.
pub fn spot(area: Area, scale: f64, place: Place) -> (i32, i32) {
    let (w, h, m) = (
        physical(WIDTH, scale),
        physical(HEIGHT, scale),
        physical(MARGIN, scale),
    );
    let x = area.x + (area.w - w) / 2;
    let y = match place {
        Place::Top => area.y + m,
        Place::Bottom => area.y + area.h - h - m,
    };
    (x, y)
}

/// `"x,y"` physical pixels, as the move handler stores it.
pub fn parse_position(value: Option<&str>) -> Option<(i32, i32)> {
    let (x, y) = value?.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// `pos` pulled back inside the work area, [`MARGIN`] from each edge. A
/// monitor too small for the bar plus margins gets it pinned top-left.
pub fn clamp(area: Area, scale: f64, (x, y): (i32, i32)) -> (i32, i32) {
    let (w, h, m) = (
        physical(WIDTH, scale),
        physical(HEIGHT, scale),
        physical(MARGIN, scale),
    );
    let fit = |v: i32, lo: i32, hi: i32| v.min(hi).max(lo);
    (
        fit(x, area.x + m, area.x + area.w - w - m),
        fit(y, area.y + m, area.y + area.h - h - m),
    )
}

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

/// `focus.bar_place` just changed: forget the dragged spot and move a
/// showing bar to the new edge.
pub fn replace(app: &AppHandle) {
    let prefs = app.state::<std::sync::Arc<Prefs>>().inner().clone();
    let _ = prefs.set(prefs::FOCUS_BAR_POSITION, "");
    if let Some(window) = app.get_webview_window(LABEL) {
        place(&window, &prefs);
    }
}

fn work_area(window: &tauri::WebviewWindow) -> Option<(Area, f64)> {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())?;
    let r = monitor.work_area();
    let area = Area {
        x: r.position.x,
        y: r.position.y,
        w: r.size.width as i32,
        h: r.size.height as i32,
    };
    Some((area, monitor.scale_factor()))
}

fn edge(area: Area, scale: f64, prefs: &Prefs) -> (i32, i32) {
    spot(
        area,
        scale,
        Place::parse(prefs.get(prefs::FOCUS_BAR_PLACE).as_deref()),
    )
}

/// The dragged spot if there is one, pulled inside this work area (the
/// taskbar or resolution may have moved); else the chosen edge.
fn place(window: &tauri::WebviewWindow, prefs: &Prefs) {
    let Some((area, scale)) = work_area(window) else {
        return;
    };
    let (x, y) = match parse_position(prefs.get(prefs::FOCUS_BAR_POSITION).as_deref()) {
        Some(dragged) => clamp(area, scale, dragged),
        None => edge(area, scale, prefs),
    };
    let _ = window.set_position(PhysicalPosition::new(x, y));
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
    place(&window, &prefs);
    float(&window);
    let held = window.clone();
    window.on_window_event(move |event| {
        let WindowEvent::Moved(p) = event else {
            return;
        };
        let Some((area, scale)) = work_area(&held) else {
            return;
        };
        // Dragged past a margin: put it back. The second Moved is inside.
        let inside = clamp(area, scale, (p.x, p.y));
        if inside != (p.x, p.y) {
            let _ = held.set_position(PhysicalPosition::new(inside.0, inside.1));
            return;
        }
        // Only a drag is remembered: our own placement lands on the edge.
        if inside != edge(area, scale, &prefs) {
            let _ = prefs.set(
                prefs::FOCUS_BAR_POSITION,
                &format!("{},{}", inside.0, inside.1),
            );
        }
    });
}

/// Show without taking focus. Tauri's `show()` is `SW_SHOW`, which activates.
#[cfg(windows)]
fn float(window: &tauri::WebviewWindow) {
    if let Ok(hwnd) = window.hwnd() {
        float_quietly(hwnd.0 as isize);
    }
}

#[cfg(not(windows))]
fn float(window: &tauri::WebviewWindow) {
    let _ = window.show();
}

/// `WS_EX_NOACTIVATE` and `WS_EX_TOOLWINDOW`, then `SW_SHOWNOACTIVATE`:
/// clicks land, focus stays where the user is typing, no Alt+Tab entry.
/// Not Tauri options, hence raw Win32. Public for `tests/focus_bar.rs`.
#[cfg(windows)]
pub fn float_quietly(hwnd: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, ShowWindow, GWL_EXSTYLE, SW_SHOWNOACTIVATE,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };
    let hwnd = HWND(hwnd as *mut _);
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let wanted = style | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1920x1080 screen with a 48 px taskbar at the bottom.
    const AREA: Area = Area {
        x: 0,
        y: 0,
        w: 1920,
        h: 1032,
    };

    #[test]
    fn v0_16_the_bar_opens_centred_at_the_bottom_unless_told_top() {
        assert_eq!(Place::parse(None), Place::Bottom);
        assert_eq!(Place::parse(Some("bottom")), Place::Bottom);
        assert_eq!(Place::parse(Some("top")), Place::Top);
        assert_eq!(Place::parse(Some("left")), Place::Bottom);
        // 300 wide, 40 high, 16 from the edge, at 100 %.
        assert_eq!(spot(AREA, 1.0, Place::Bottom), (810, 976));
        assert_eq!(spot(AREA, 1.0, Place::Top), (810, 16));
        // 150 %: everything in physical pixels, still centred on the area.
        assert_eq!(spot(AREA, 1.5, Place::Bottom), (735, 948));
    }

    /// A second monitor left of the primary has negative coordinates.
    #[test]
    fn v0_16_the_bar_opens_on_the_monitor_it_is_given() {
        let left = Area {
            x: -1280,
            y: 0,
            w: 1280,
            h: 1024,
        };
        assert_eq!(spot(left, 1.0, Place::Top), (-790, 16));
    }

    #[test]
    fn v0_16_a_dragged_spot_reads_back_and_empty_means_none() {
        assert_eq!(parse_position(Some("120,-8")), Some((120, -8)));
        assert_eq!(parse_position(Some(" 5 , 6 ")), Some((5, 6)));
        assert_eq!(parse_position(Some("x,6")), None);
        assert_eq!(parse_position(Some("")), None);
        assert_eq!(parse_position(None), None);
    }

    #[test]
    fn v0_16_a_dragged_bar_stays_a_margin_inside_every_edge() {
        assert_eq!(
            clamp(AREA, 1.0, (500, 400)),
            (500, 400),
            "inside: untouched"
        );
        assert_eq!(clamp(AREA, 1.0, (500, -30)), (500, 16), "off the top");
        assert_eq!(
            clamp(AREA, 1.0, (500, 1010)),
            (500, 976),
            "into the taskbar"
        );
        assert_eq!(clamp(AREA, 1.0, (-200, 400)), (16, 400));
        assert_eq!(clamp(AREA, 1.0, (1800, 400)), (1604, 400));
        let tiny = Area {
            x: 0,
            y: 0,
            w: 200,
            h: 50,
        };
        assert_eq!(clamp(tiny, 1.0, (40, 40)), (16, 16));
    }
}
