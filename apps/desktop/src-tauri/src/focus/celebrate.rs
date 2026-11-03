//! End-of-session celebration: confetti from the screen's two bottom corners,
//! Raycast's confetti cannon as the model, or a toast when `focus.confetti`
//! is off or motion is reduced.
//!
//! One short-lived window per celebration, built off the main thread (CLAUDE.md)
//! and destroyed after [`Style::lifetime_ms`], so idle RSS carries no second
//! webview. Never takes focus, never takes the mouse: confetti covers the
//! monitor while the user may be mid-click. Unlike Raycast's, it steals no focus.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{
    AppHandle, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};

use super::bar::{clamp, parse_position, spot, Area, Place, PILL_H};
use crate::prefs::{self, Prefs};

pub const LABEL: &str = "focuscelebrate";

/// Logical toast size; the page draws its panel inside it.
const TOAST_W: f64 = 360.0;
const TOAST_H: f64 = 64.0;

/// Which celebration a session end gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Confetti,
    Toast,
}

impl Style {
    /// `focus.confetti` (default on), demoted to a toast when motion is reduced
    /// in Takyon or in Windows: a screen of moving paper is exactly that motion.
    pub fn choose(confetti: bool, reduce_motion: bool) -> Style {
        if !confetti || reduce_motion {
            Style::Toast
        } else {
            Style::Confetti
        }
    }

    fn slug(self) -> &'static str {
        match self {
            Style::Confetti => "confetti",
            Style::Toast => "toast",
        }
    }

    /// Page load (measured ~1.2 s cold), then the animation (`CONFETTI_MS`,
    /// `TOAST_MS`), with slack for a slow machine. The page fades itself out
    /// before this; the tail is an invisible window, not a lingering one.
    pub fn lifetime_ms(self) -> u64 {
        match self {
            Style::Confetti => 7_000,
            Style::Toast => 6_500,
        }
    }
}

/// The page URL. The goal travels in the query, so the page needs no IPC.
pub fn url(style: Style, goal: &str) -> String {
    format!(
        "index.html?window=celebrate&style={}&goal={}",
        style.slug(),
        crate::search::fetch::percent_encode(goal)
    )
}

/// A full-monitor window one pixel short: Windows takes a topmost window
/// covering the whole monitor for a fullscreen app (taskbar, notifications).
pub fn confetti_rect(monitor: Area) -> Area {
    Area {
        h: (monitor.h - 1).max(1),
        ..monitor
    }
}

/// Where the toast goes: centred on the bar's dragged spot (`"cx,y"`, pill
/// centre and top) if it has one, else the bar's edge. Held inside `work`.
pub fn toast_rect(work: Area, scale: f64, place: Place, dragged: Option<(i32, i32)>) -> Area {
    let size = (
        (TOAST_W * scale).round() as i32,
        (TOAST_H * scale).round() as i32,
    );
    let (x, y) = match dragged {
        Some((cx, top)) => {
            let middle = top + (PILL_H * scale / 2.0).round() as i32;
            clamp(work, scale, size, (cx - size.0 / 2, middle - size.1 / 2))
        }
        None => spot(work, scale, place, size),
    };
    Area {
        x,
        y,
        w: size.0,
        h: size.1,
    }
}

/// Celebrations built so far. Held across replace-and-build and across the
/// timer's check, so a stalled older thread never closes or outlives a newer
/// window: whoever bumped it last owns the one window under [`LABEL`].
static GENERATION: Mutex<u64> = Mutex::new(0);

/// Celebrate a session that ended while the user was here. Never blocks.
pub fn fire(app: &AppHandle, goal: String) {
    let app = app.clone();
    std::thread::spawn(move || {
        let prefs = app.state::<Arc<Prefs>>().inner().clone();
        let reduce = prefs::flag(&prefs, prefs::UI_REDUCE_MOTION, false) || system_reduces_motion();
        let style = Style::choose(prefs::flag(&prefs, prefs::FOCUS_CONFETTI, true), reduce);
        let mine = {
            let mut generation = GENERATION.lock().unwrap_or_else(|p| p.into_inner());
            *generation += 1;
            if let Some(old) = app.get_webview_window(LABEL) {
                let _ = old.destroy();
            }
            if build(&app, &prefs, style, &goal).is_none() {
                return;
            }
            *generation
        };
        std::thread::sleep(Duration::from_millis(style.lifetime_ms()));
        let generation = GENERATION.lock().unwrap_or_else(|p| p.into_inner());
        if *generation == mine {
            if let Some(window) = app.get_webview_window(LABEL) {
                let _ = window.destroy();
            }
        }
    });
}

/// The monitor holding `at`, else the one under the cursor (where the user is
/// looking), else the primary.
fn monitor(app: &AppHandle, at: Option<(i32, i32)>) -> Option<Monitor> {
    let point = |x: f64, y: f64| app.monitor_from_point(x, y).ok().flatten();
    at.and_then(|(x, y)| point(f64::from(x), f64::from(y)))
        .or_else(|| {
            let p = app.cursor_position().ok()?;
            point(p.x, p.y)
        })
        .or_else(|| app.primary_monitor().ok().flatten())
}

fn area(position: PhysicalPosition<i32>, size: PhysicalSize<u32>) -> Area {
    Area {
        x: position.x,
        y: position.y,
        w: size.width as i32,
        h: size.height as i32,
    }
}

fn build(app: &AppHandle, prefs: &Prefs, style: Style, goal: &str) -> Option<tauri::WebviewWindow> {
    // Toast: where the Focus bar sat, dragged spot included; the eye is there.
    let dragged = match style {
        Style::Toast => parse_position(prefs.get(prefs::FOCUS_BAR_POSITION).as_deref()),
        Style::Confetti => None,
    };
    let monitor = monitor(app, dragged)?;
    let scale = monitor.scale_factor();
    let rect = match style {
        Style::Confetti => confetti_rect(area(*monitor.position(), *monitor.size())),
        Style::Toast => {
            let work = monitor.work_area();
            let place = Place::parse(prefs.get(prefs::FOCUS_BAR_PLACE).as_deref());
            toast_rect(area(work.position, work.size), scale, place, dragged)
        }
    };
    let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App(url(style, goal).into()))
        .title("Takyon Focus")
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .shadow(false)
        .visible(false);
    #[cfg(windows)]
    let builder = builder.transparent(true);
    let window = match builder.build() {
        Ok(w) => w,
        Err(e) => {
            eprintln!("[takyon] the Focus celebration could not be built: {e}");
            return None;
        }
    };
    let _ = window.set_size(PhysicalSize::new(rect.w as u32, rect.h as u32));
    let _ = window.set_position(PhysicalPosition::new(rect.x, rect.y));
    // Before floating: it ORs its styles over this one's WS_EX_TRANSPARENT.
    let _ = window.set_ignore_cursor_events(true);
    super::bar::float(&window);
    Some(window)
}

/// Windows' "Animation effects" switch, which Chromium reads as
/// `prefers-reduced-motion`. Off = reduce.
#[cfg(windows)]
fn system_reduces_motion() -> bool {
    use windows::core::BOOL;
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    let mut on = BOOL(1);
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(&mut on as *mut BOOL as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    read.is_ok() && !on.as_bool()
}

#[cfg(not(windows))]
fn system_reduces_motion() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_16_confetti_when_on_and_a_toast_when_off() {
        assert_eq!(Style::choose(true, false), Style::Confetti);
        assert_eq!(Style::choose(false, false), Style::Toast);
    }

    #[test]
    fn v0_16_reduced_motion_gets_the_toast_even_with_confetti_on() {
        assert_eq!(Style::choose(true, true), Style::Toast);
        assert_eq!(Style::choose(false, true), Style::Toast);
    }

    #[test]
    fn v0_16_the_goal_reaches_the_page_as_one_query_value() {
        assert_eq!(
            url(Style::Confetti, "Write report & ship"),
            "index.html?window=celebrate&style=confetti&goal=Write%20report%20%26%20ship"
        );
        assert_eq!(
            url(Style::Toast, ""),
            "index.html?window=celebrate&style=toast&goal="
        );
    }

    /// 1920x1032 work area (48 px taskbar) at 100 %: a 360x64 toast.
    const WORK: Area = Area {
        x: 0,
        y: 0,
        w: 1920,
        h: 1032,
    };

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Area {
        Area { x, y, w, h }
    }

    #[test]
    fn v0_16_the_toast_sits_at_the_bar_edge_when_the_bar_was_never_dragged() {
        let at = |place| toast_rect(WORK, 1.0, place, None);
        assert_eq!(at(Place::Bottom), rect(780, 952, 360, 64));
        assert_eq!(at(Place::Top), rect(780, 16, 360, 64));
    }

    /// Centred on the dragged pill, pulled back inside when that would spill.
    #[test]
    fn v0_16_the_toast_follows_a_dragged_bar_and_stays_inside_the_work_area() {
        let at = |pos| toast_rect(WORK, 1.0, Place::Bottom, Some(pos));
        assert_eq!(at((400, 300)), rect(220, 288, 360, 64));
        assert_eq!(at((60, 300)).x, 16, "dragged hard left");
        assert_eq!(at((400, 1010)).y, 952, "into the taskbar");
        // 150 %: 540x96, centred on a pill whose middle is 30 px below its top.
        assert_eq!(
            toast_rect(WORK, 1.5, Place::Bottom, Some((960, 500))),
            rect(690, 482, 540, 96)
        );
    }

    /// One pixel short of fullscreen, on whichever monitor, negative included.
    #[test]
    fn v0_16_confetti_covers_the_monitor_but_is_never_fullscreen() {
        let left = Area {
            x: -1920,
            y: 0,
            w: 1920,
            h: 1080,
        };
        assert_eq!(
            confetti_rect(left),
            Area {
                x: -1920,
                y: 0,
                w: 1920,
                h: 1079
            }
        );
    }
}
