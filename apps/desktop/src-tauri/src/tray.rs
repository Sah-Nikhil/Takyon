//! The tray icon.
//!
//! It is not a nicety here, it is the only visible surface the app has. The
//! Palette is a hidden window with no taskbar button; without a tray icon, an
//! app whose hotkey failed to register is a process you can only end from Task
//! Manager. That is why the tray is built in the same phase as autostart rather
//! than a later one.

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::identity::DISPLAY_NAME;

const TRAY_ID: &str = "main";

/// Both polarities, compiled in.
///
/// Embedded rather than resolved from the resource directory because the tray is
/// built during startup, and the login-to-responsive budget has no room for two
/// file reads and a path resolution that can fail differently in dev and release.
/// The names describe the *taskbar* they are drawn on, not the glyph: `tray-dark`
/// is the light glyph that goes on a dark taskbar.
const TRAY_DARK: &[u8] = include_bytes!("../icons/tray-dark.png");
const TRAY_LIGHT: &[u8] = include_bytes!("../icons/tray-light.png");

/// May the tray icon be hidden right now?
///
/// No, when the hotkey is not registered. The Palette has no taskbar button, so
/// with a dead hotkey the tray is the only way in and the only way out — hiding
/// it there strands the user in Task Manager.
pub fn may_hide(hotkey_registered: bool) -> bool {
    hotkey_registered
}

/// Show or hide the tray icon (v0.6's Launcher page).
pub fn set_visible(app: &AppHandle, visible: bool) -> Result<(), String> {
    if !visible && !may_hide(app.state::<crate::hotkey::HotkeyState>().get().registered) {
        return Err(
            "The tray icon is the only way in while the hotkey is unregistered. \
             Rebind the hotkey first."
                .into(),
        );
    }
    let Some(icon) = app.tray_by_id(TRAY_ID) else {
        return Err("there is no tray icon to change".into());
    };
    icon.set_visible(visible).map_err(|e| e.to_string())
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "tray_open", format!("Open {DISPLAY_NAME}"), true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "tray_settings", "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray_quit", format!("Quit {DISPLAY_NAME}"), true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&open, &settings, &PredefinedMenuItem::separator(app)?, &quit],
    )?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(DISPLAY_NAME)
        .menu(&menu)
        // Windows convention: left-click acts, right-click opens the menu. Left
        // opening the menu instead would leave no cheap way into the Palette when
        // the hotkey is the thing that is broken.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "tray_open" => {
                let bench = app.state::<crate::bench::Bench>();
                crate::window::show(app, &bench);
            }
            "tray_settings" => crate::settings::open(app),
            "tray_quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                let bench = app.state::<crate::bench::Bench>();
                crate::window::show(app, &bench);
            }
        });

    if let Ok(icon) = current_icon() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;

    #[cfg(windows)]
    watch_taskbar_theme(app.clone());

    Ok(())
}

fn current_icon() -> tauri::Result<Image<'static>> {
    let bytes = if taskbar_is_light() { TRAY_LIGHT } else { TRAY_DARK };
    Image::from_bytes(bytes)
}

/// Is the *taskbar* light?
///
/// Deliberately `SystemUsesLightTheme` and not `AppsUseLightTheme`, and not
/// Tauri's window theme either. Windows lets those disagree — "Choose your mode:
/// Custom" is a supported setting — and the notification area follows the system
/// one. Reading the app theme would give a monochrome glyph that vanishes into
/// the taskbar for everyone running the mixed mode.
#[cfg(windows)]
fn taskbar_is_light() -> bool {
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ, RRF_RT_REG_DWORD,
    };

    unsafe {
        let mut key = HKEY::default();
        let opened = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            None,
            KEY_READ,
            &mut key,
        );
        if opened != ERROR_SUCCESS {
            // The key is absent on Windows Server SKUs with no theme service.
            // Dark is the safer guess: the default taskbar is dark.
            return false;
        }

        let mut value: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        let read = RegGetValueW(
            key,
            PCWSTR::null(),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut std::ffi::c_void),
            Some(&mut size),
        );
        let _ = RegCloseKey(key);

        read == ERROR_SUCCESS && value == 1
    }
}

#[cfg(not(windows))]
fn taskbar_is_light() -> bool {
    false
}

/// Swap the glyph when the system theme changes, without polling.
///
/// `RegNotifyChangeKeyValue` in its synchronous form blocks the calling thread
/// until the key changes, which is exactly what a dedicated thread wants. A timer
/// would burn a wakeup every interval forever to catch an event that happens a
/// handful of times in a machine's life, on a process whose whole argument is that
/// it costs nothing while idle.
///
/// The notification is one-shot, so it is re-armed each iteration.
#[cfg(windows)]
fn watch_taskbar_theme(app: AppHandle) {
    use windows::core::w;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegNotifyChangeKeyValue, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_NOTIFY,
        REG_NOTIFY_CHANGE_LAST_SET,
    };

    std::thread::spawn(move || unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            None,
            KEY_NOTIFY,
            &mut key,
        ) != ERROR_SUCCESS
        {
            return;
        }

        loop {
            if RegNotifyChangeKeyValue(key, false, REG_NOTIFY_CHANGE_LAST_SET, None, false)
                != ERROR_SUCCESS
            {
                break;
            }
            if let (Some(tray), Ok(icon)) = (app.tray_by_id(TRAY_ID), current_icon()) {
                let _ = tray.set_icon(Some(icon));
            }
        }

        let _ = RegCloseKey(key);
    });
}

/// Re-register the login entry against the *current* executable path.
///
/// An update, or a per-user to per-machine reinstall, can leave the `Run` value
/// pointing at a path that no longer exists — and it fails *silently at boot*, the
/// one place nobody is watching. Ported from tesseract, where this was learned.
///
/// Re-registers rather than comparing first: `AutoLaunchManager` wraps the
/// registered target in a private field with no getter, so there is nothing to
/// compare against without re-deriving the registry layout ourselves. `enable()`
/// is idempotent and writes `current_exe()`, so the write *is* the comparison.
///
/// It only ever runs when `is_enabled()` already says on, so it corrects a stale
/// path and never re-enables something the user turned off. That also makes it
/// safe against Windows' `StartupApproved` flag: `auto-launch` reads that flag, so
/// an entry disabled from Task Manager reports `false` here and is left alone.
#[cfg(not(debug_assertions))]
pub fn self_heal_autostart(app: &AppHandle) {
    use tauri_plugin_autostart::ManagerExt;

    let manager = app.autolaunch();
    match manager.is_enabled() {
        Ok(true) => {
            if let Err(e) = manager.enable() {
                eprintln!("[takyon] could not refresh the autostart entry: {e}");
            }
        }
        Ok(false) => {}
        Err(e) => eprintln!("[takyon] could not read the autostart entry: {e}"),
    }
}

/// Never in a dev build. A debug binary that registers itself points the `Run` key
/// at `target\debug\`, which then launches a dev build every login and survives
/// uninstalling the real app.
#[cfg(debug_assertions)]
pub fn self_heal_autostart(_app: &AppHandle) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both polarities have to exist and be real PNGs. They are generated by
    /// `bun run --cwd brand build`, so the failure this catches is someone running
    /// `tauri icon`, which overwrites the generated set with Tauri's default
    /// artwork and drops the tray pair entirely.
    #[test]
    fn v0_1_both_tray_polarities_are_embedded_pngs() {
        const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G'];
        assert!(TRAY_DARK.starts_with(PNG_MAGIC), "tray-dark.png is not a PNG");
        assert!(TRAY_LIGHT.starts_with(PNG_MAGIC), "tray-light.png is not a PNG");
        assert_ne!(
            TRAY_DARK, TRAY_LIGHT,
            "the two polarities are identical; one taskbar theme will show nothing"
        );
    }

    /// Reading the theme must not panic or hang, whatever the machine's registry
    /// looks like. It runs during startup, inside the 500 ms budget.
    #[test]
    fn v0_1_reading_the_taskbar_theme_always_answers() {
        let _ = taskbar_is_light();
    }
}
