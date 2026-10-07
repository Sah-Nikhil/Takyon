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

/// Logical height. Width follows the pill's content ([`fit`]), within
/// [`MIN_WIDTH`]..[`MAX_WIDTH`]; [`START_WIDTH`] until the page measures.
const HEIGHT: f64 = 40.0;
const START_WIDTH: f64 = 200.0;
const MIN_WIDTH: f64 = 120.0;
const MAX_WIDTH: f64 = 420.0;
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

/// Where a bar of physical `size` opens: centred, [`MARGIN`] in from the edge.
pub fn spot(area: Area, scale: f64, place: Place, (w, h): (i32, i32)) -> (i32, i32) {
    let m = physical(MARGIN, scale);
    let x = area.x + (area.w - w) / 2;
    let y = match place {
        Place::Top => area.y + m,
        Place::Bottom => area.y + area.h - h - m,
    };
    (x, y)
}

/// `"cx,y"`: the dragged bar's centre and top, physical pixels.
pub fn parse_position(value: Option<&str>) -> Option<(i32, i32)> {
    let (x, y) = value?.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// `pos` pulled back inside the work area, [`MARGIN`] from each edge. A
/// monitor too small for the bar plus margins gets it pinned top-left.
pub fn clamp(area: Area, scale: f64, (w, h): (i32, i32), (x, y): (i32, i32)) -> (i32, i32) {
    let m = physical(MARGIN, scale);
    let fit = |v: i32, lo: i32, hi: i32| v.min(hi).max(lo);
    (
        fit(x, area.x + m, area.x + area.w - w - m),
        fit(y, area.y + m, area.y + area.h - h - m),
    )
}

/// The width the pill asked for, held to [`MIN_WIDTH`]..[`MAX_WIDTH`].
pub fn fitted_width(requested: f64) -> f64 {
    if requested.is_finite() {
        requested.clamp(MIN_WIDTH, MAX_WIDTH)
    } else {
        START_WIDTH
    }
}

/// Left edge after a resize from `old_w` to `new_w` that keeps the centre.
pub fn recentre(x: i32, old_w: i32, new_w: i32) -> i32 {
    x + (old_w - new_w) / 2
}

/// Resize the showing bar to the pill's `width` (logical), centre kept,
/// still inside the margins. The page calls this when its content changes.
pub fn fit(app: &AppHandle, width: f64) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    let (Ok(pos), Ok(old)) = (window.outer_position(), window.outer_size()) else {
        return;
    };
    let Some((area, scale)) = work_area(&window) else {
        return;
    };
    let new = (
        physical(fitted_width(width), scale),
        physical(HEIGHT, scale),
    );
    if new.0 == old.width as i32 {
        return;
    }
    let x = recentre(pos.x, old.width as i32, new.0);
    let (x, y) = clamp(area, scale, new, (x, pos.y));
    let _ = window.set_size(tauri::PhysicalSize::new(new.0 as u32, new.1 as u32));
    let _ = window.set_position(PhysicalPosition::new(x, y));
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

fn edge(area: Area, scale: f64, size: (i32, i32), prefs: &Prefs) -> (i32, i32) {
    spot(
        area,
        scale,
        Place::parse(prefs.get(prefs::FOCUS_BAR_PLACE).as_deref()),
        size,
    )
}

/// The window's physical size now.
fn size_of(window: &tauri::WebviewWindow) -> Option<(i32, i32)> {
    let s = window.outer_size().ok()?;
    Some((s.width as i32, s.height as i32))
}

/// The dragged spot if there is one, pulled inside this work area (the
/// taskbar or resolution may have moved); else the chosen edge.
fn place(window: &tauri::WebviewWindow, prefs: &Prefs) {
    let (Some((area, scale)), Some(size)) = (work_area(window), size_of(window)) else {
        return;
    };
    let (x, y) = match parse_position(prefs.get(prefs::FOCUS_BAR_POSITION).as_deref()) {
        Some((cx, y)) => clamp(area, scale, size, (cx - size.0 / 2, y)),
        None => edge(area, scale, size, prefs),
    };
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

fn build(app: &AppHandle) {
    let prefs = app.state::<std::sync::Arc<Prefs>>().inner().clone();
    let builder =
        WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?window=bar".into()))
            .title("Takyon Focus")
            .inner_size(START_WIDTH, HEIGHT)
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
        let (Some((area, scale)), Some(size)) = (work_area(&held), size_of(&held)) else {
            return;
        };
        // Dragged past a margin: put it back. The second Moved is inside.
        let inside = clamp(area, scale, size, (p.x, p.y));
        if inside != (p.x, p.y) {
            let _ = held.set_position(PhysicalPosition::new(inside.0, inside.1));
            return;
        }
        // Only a drag is remembered: our own placement lands on the edge.
        // Stored as the centre: the width refits, the centre is what stays.
        if inside != edge(area, scale, size, &prefs) {
            let _ = prefs.set(
                prefs::FOCUS_BAR_POSITION,
                &format!("{},{}", inside.0 + size.0 / 2, inside.1),
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

    /// 300 by 40 at 100 %, 450 by 60 at 150 %.
    const AT_100: (i32, i32) = (300, 40);
    const AT_150: (i32, i32) = (450, 60);

    #[test]
    fn v0_16_the_bar_opens_centred_at_the_bottom_unless_told_top() {
        assert_eq!(Place::parse(None), Place::Bottom);
        assert_eq!(Place::parse(Some("bottom")), Place::Bottom);
        assert_eq!(Place::parse(Some("top")), Place::Top);
        assert_eq!(Place::parse(Some("left")), Place::Bottom);
        assert_eq!(spot(AREA, 1.0, Place::Bottom, AT_100), (810, 976));
        assert_eq!(spot(AREA, 1.0, Place::Top, AT_100), (810, 16));
        // 150 %: everything in physical pixels, still centred on the area.
        assert_eq!(spot(AREA, 1.5, Place::Bottom, AT_150), (735, 948));
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
        assert_eq!(spot(left, 1.0, Place::Top, AT_100), (-790, 16));
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
        let at = |pos| clamp(AREA, 1.0, AT_100, pos);
        assert_eq!(at((500, 400)), (500, 400), "inside: untouched");
        assert_eq!(at((500, -30)), (500, 16), "off the top");
        assert_eq!(at((500, 1010)), (500, 976), "into the taskbar");
        assert_eq!(at((-200, 400)), (16, 400));
        assert_eq!(at((1800, 400)), (1604, 400));
        let tiny = Area {
            x: 0,
            y: 0,
            w: 200,
            h: 50,
        };
        assert_eq!(clamp(tiny, 1.0, AT_100, (40, 40)), (16, 16));
    }

    /// The pill hugs its content: a short goal makes a short bar, and the
    /// bar grows and shrinks about its centre.
    #[test]
    fn v0_16_the_bar_fits_its_content_about_its_centre() {
        assert_eq!(fitted_width(186.4), 186.4);
        assert_eq!(fitted_width(40.0), 120.0);
        assert_eq!(fitted_width(900.0), 420.0);
        assert_eq!(fitted_width(f64::NAN), 200.0);
        // 200 wide at x 860 is centred on 960; 300 wide stays centred there.
        assert_eq!(recentre(860, 200, 300), 810);
        assert_eq!(recentre(810, 300, 200), 860);
    }
}
