//! Send a real keystroke, at the input layer.
//!
//! It has to be real input. A global hotkey is dispatched by the OS from the raw
//! input stream, so anything that posts messages to the foreground window types
//! into whatever is focused and never reaches Takyon. Both arms below inject at
//! the same layer a keyboard does.
//!
//! The whole point of the harness is that the number describes the real hotkey
//! path. A benchmark that quietly measured a synthetic show-window call would be
//! the easiest possible way to produce four reassuring numbers that mean nothing.

use crate::Key;

#[cfg(windows)]
pub fn send(key: Key) -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        keybd_event, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VK_C, VK_CONTROL, VK_ESCAPE, VK_F9,
        VK_MENU, VK_SPACE,
    };

    // Modifiers first, then the key, then everything released in reverse — the
    // order a keyboard produces.
    let chord: &[u16] = match key {
        Key::AltSpace => &[VK_MENU.0, VK_SPACE.0],
        Key::CtrlAltF9 => &[VK_CONTROL.0, VK_MENU.0, VK_F9.0],
        Key::Escape => &[VK_ESCAPE.0],
        Key::LetterC => &[VK_C.0],
    };

    unsafe {
        for vk in chord {
            keybd_event(*vk as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
        }
        for vk in chord.iter().rev() {
            keybd_event(*vk as u8, 0, KEYEVENTF_KEYUP, 0);
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn send(key: Key) -> Result<(), String> {
    use objc2_core_graphics::{CGEvent, CGEventFlags, CGEventTapLocation};

    // kVK_* virtual key codes. Not ASCII and not Windows' VK_*: these are
    // positional, from Carbon's `Events.h`.
    const KVK_SPACE: u16 = 49;
    const KVK_ESCAPE: u16 = 53;
    const KVK_C: u16 = 8;
    const KVK_F9: u16 = 101;
    const KVK_CONTROL: u16 = 59;
    const KVK_OPTION: u16 = 58;

    let (mods, code, flags): (&[u16], u16, CGEventFlags) = match key {
        Key::AltSpace => (&[KVK_OPTION], KVK_SPACE, CGEventFlags::MaskAlternate),
        Key::CtrlAltF9 => (
            &[KVK_CONTROL, KVK_OPTION],
            KVK_F9,
            CGEventFlags::MaskControl.union(CGEventFlags::MaskAlternate),
        ),
        Key::Escape => (&[], KVK_ESCAPE, CGEventFlags::empty()),
        Key::LetterC => (&[], KVK_C, CGEventFlags::empty()),
    };

    // Flags are set explicitly as well as the modifier keys being pressed.
    // A synthetic modifier keydown does not reliably update the flag state the
    // Carbon hotkey dispatcher reads, and a chord with no flags is a bare key.
    let post = |code: u16, down: bool, flags: CGEventFlags| -> Result<(), String> {
        let event = CGEvent::new_keyboard_event(None, code, down)
            .ok_or_else(|| format!("CGEventCreateKeyboardEvent failed for kVK {code}"))?;
        CGEvent::set_flags(Some(&event), flags);
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
        Ok(())
    };

    for m in mods {
        post(*m, true, flags)?;
    }
    post(code, true, flags)?;
    post(code, false, flags)?;
    for m in mods.iter().rev() {
        post(*m, false, CGEventFlags::empty())?;
    }
    Ok(())
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn send(_key: Key) -> Result<(), String> {
    Err("input injection is implemented for Windows and macOS only".into())
}
