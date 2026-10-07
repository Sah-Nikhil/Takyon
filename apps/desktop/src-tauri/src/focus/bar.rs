//! The floating bar: goal and time left, always on top, never takes focus.
//!
//! Built when a session starts and destroyed when it ends; never kept hidden
//! between sessions, so idle RSS does not carry a second webview. Built off
//! the main thread (CLAUDE.md: never create a window from the main thread).
//! Opens where it was last dragged, else centred at the top or bottom of the
//! work area (`focus.bar_place`). The pill never sits past [`MARGIN`] from
//! any edge. Above and below it the window holds a [`DOCK`] the hover buttons
//! slide into; a window region keeps the docks out of hit-testing until then.

use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::prefs::{self, Prefs};

pub const LABEL: &str = "focusbar";

/// Logical pill height. Width follows the pill's content ([`fit`]), within
/// [`MIN_WIDTH`]..[`MAX_WIDTH`]; [`START_WIDTH`] until the page measures.
const PILL_H: f64 = 40.0;
/// Room above and below the pill for the hover buttons: 30 px plus a 6 px gap.
/// Mirrored in `FocusBar.tsx`.
const DOCK: f64 = 36.0;
const START_WIDTH: f64 = 200.0;
const MIN_WIDTH: f64 = 120.0;
const MAX_WIDTH: f64 = 440.0;
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

/// Which dock the hover buttons slide into; None while they are tucked away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Up,
    Down,
}

/// The part of a `w`-wide window that takes the mouse, `(left, top, right,
/// bottom)` physical: the pill, plus the dock the buttons are out in.
pub fn region(w: i32, scale: f64, side: Option<Side>) -> (i32, i32, i32, i32) {
    let (dock, pill) = (physical(DOCK, scale), physical(PILL_H, scale));
    match side {
        None => (0, dock, w, dock + pill),
        Some(Side::Up) => (0, 0, w, dock + pill),
        Some(Side::Down) => (0, dock, w, 2 * dock + pill),
    }
}

/// Window top-left for a pill at `pill`, and back: the docks sit around it.
fn window_at((x, y): (i32, i32), scale: f64) -> (i32, i32) {
    (x, y - physical(DOCK, scale))
}

fn pill_at((x, y): (i32, i32), scale: f64) -> (i32, i32) {
    (x, y + physical(DOCK, scale))
}

fn window_size(w: i32, scale: f64) -> (u32, u32) {
    (w as u32, physical(2.0 * DOCK + PILL_H, scale) as u32)
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
        physical(PILL_H, scale),
    );
    if new.0 == old.width as i32 {
        return;
    }
    let pill = pill_at((recentre(pos.x, old.width as i32, new.0), pos.y), scale);
    let (x, y) = window_at(clamp(area, scale, new, pill), scale);
    let (w, h) = window_size(new.0, scale);
    let _ = window.set_size(tauri::PhysicalSize::new(w, h));
    let _ = window.set_position(PhysicalPosition::new(x, y));
    shape(
        &window,
        scale,
        *REVEALED.lock().unwrap_or_else(|p| p.into_inner()),
    );
}

/// The dock the buttons are out in now, so a refit keeps the region.
static REVEALED: std::sync::Mutex<Option<Side>> = std::sync::Mutex::new(None);

/// Hover: open the dock the buttons slide into (`Some`), or close both.
pub fn reveal(app: &AppHandle, side: Option<Side>) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    *REVEALED.lock().unwrap_or_else(|p| p.into_inner()) = side;
    if let Some((_, scale)) = work_area(&window) {
        shape(&window, scale, side);
    }
}

/// Clip the window, input and paint alike, to [`region`].
#[cfg(windows)]
fn shape(window: &tauri::WebviewWindow, scale: f64, side: Option<Side>) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{CreateRectRgn, SetWindowRgn};
    let (Ok(hwnd), Ok(size)) = (window.hwnd(), window.outer_size()) else {
        return;
    };
    let (l, t, r, b) = region(size.width as i32, scale, side);
    unsafe {
        // The window owns the region once set; no DeleteObject.
        let rgn = CreateRectRgn(l, t, r, b);
        SetWindowRgn(HWND(hwnd.0), Some(rgn), true);
    }
}

#[cfg(not(windows))]
fn shape(_: &tauri::WebviewWindow, _: f64, _: Option<Side>) {}

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

/// The pill's physical size now: the window's width, the pill's height. A
/// window not yet shown may report no width: the start width stands in.
fn size_of(window: &tauri::WebviewWindow, scale: f64) -> Option<(i32, i32)> {
    let s = window.outer_size().ok()?;
    let w = if s.width > 0 {
        s.width as i32
    } else {
        physical(START_WIDTH, scale)
    };
    Some((w, physical(PILL_H, scale)))
}

/// The dragged spot if there is one, pulled inside this work area (the
/// taskbar or resolution may have moved); else the chosen edge.
fn place(window: &tauri::WebviewWindow, prefs: &Prefs) {
    let Some((area, scale)) = work_area(window) else {
        return;
    };
    let Some(size) = size_of(window, scale) else {
        return;
    };
    let pill = match parse_position(prefs.get(prefs::FOCUS_BAR_POSITION).as_deref()) {
        Some((cx, y)) => clamp(area, scale, size, (cx - size.0 / 2, y)),
        None => edge(area, scale, size, prefs),
    };
    let (x, y) = window_at(pill, scale);
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

fn build(app: &AppHandle) {
    let prefs = app.state::<std::sync::Arc<Prefs>>().inner().clone();
    let builder =
        WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?window=bar".into()))
            .title("Takyon Focus")
            .inner_size(START_WIDTH, 2.0 * DOCK + PILL_H)
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
    *REVEALED.lock().unwrap_or_else(|p| p.into_inner()) = None;
    float(&window);
    // After showing: a region set on the hidden window did not hold.
    if let Some((_, scale)) = work_area(&window) {
        shape(&window, scale, None);
    }
    let held = window.clone();
    window.on_window_event(move |event| {
        let WindowEvent::Moved(p) = event else {
            return;
        };
        let Some((area, scale)) = work_area(&held) else {
            return;
        };
        let Some(size) = size_of(&held, scale) else {
            return;
        };
        // Dragged past a margin: put it back. The second Moved is inside.
        let pill = pill_at((p.x, p.y), scale);
        let inside = clamp(area, scale, size, pill);
        if inside != pill {
            let (x, y) = window_at(inside, scale);
            let _ = held.set_position(PhysicalPosition::new(x, y));
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

    /// At rest only the pill takes the mouse; a hover opens one dock too.
    #[test]
    fn v0_16_the_docks_take_the_mouse_only_while_the_buttons_are_out() {
        assert_eq!(region(200, 1.0, None), (0, 36, 200, 76));
        assert_eq!(region(200, 1.0, Some(Side::Up)), (0, 0, 200, 76));
        assert_eq!(region(200, 1.0, Some(Side::Down)), (0, 36, 200, 112));
        assert_eq!(region(300, 1.5, None), (0, 54, 300, 114));
        assert_eq!(window_at((10, 100), 1.0), (10, 64));
        assert_eq!(pill_at(window_at((10, 100), 1.5), 1.5), (10, 100));
    }

    /// The pill hugs its content: a short goal makes a short bar, and the
    /// bar grows and shrinks about its centre.
    #[test]
    fn v0_16_the_bar_fits_its_content_about_its_centre() {
        assert_eq!(fitted_width(186.4), 186.4);
        assert_eq!(fitted_width(40.0), 120.0);
        assert_eq!(fitted_width(900.0), 440.0);
        assert_eq!(fitted_width(f64::NAN), 200.0);
        // 200 wide at x 860 is centred on 960; 300 wide stays centred there.
        assert_eq!(recentre(860, 200, 300), 810);
        assert_eq!(recentre(810, 300, 200), 860);
    }
}
