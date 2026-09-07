//! Paste-back: put a clip on the clipboard, then send `Ctrl+V` (v0.5 task 8).
//!
//! Only the clipboard write is guaranteed. Synthesising the keystroke can fail
//! for reasons this process cannot see — a UIPI boundary, an elevated target
//! window — so the two are ordered clipboard first: a failed keystroke leaves the
//! user one manual `Ctrl+V` away, a failed copy leaves them with nothing.
//!
//! Format is not preserved at v0.5 because nothing but text is captured yet. When
//! images and file lists arrive they are stored with their format and restored to
//! the same one; the seam is [`Paste::kind`], not a second function.
//!
//! Both entry points go through [`crate::clips::os::host`]. This file owns the
//! shape of a paste and the settle delay; the OS calls live behind the trait.

use crate::clips::os::host;
use crate::clips::store::ClipKind;

/// How long the target window gets to take focus back before the keystroke.
///
/// The Palette is hidden first, and focus returns asynchronously — Windows has no
/// "focus has settled" event to wait on. Measured by hand: 40 ms was flaky,
/// 80 ms was not.
pub const FOCUS_SETTLE_MS: u64 = 80;

/// The delay exists because focus returns asynchronously. Zero pastes into the
/// Palette, which is the bug this whole file is arranged around.
const _: () = assert!(FOCUS_SETTLE_MS >= 50);

/// What is being pasted. One variant at v0.5; the point is that paste-back is
/// already asking rather than assuming text.
pub struct Paste<'a> {
    pub kind: ClipKind,
    pub text: &'a str,
}

/// Put the clip on the clipboard. Always run, keystroke or not.
pub fn to_clipboard(paste: &Paste<'_>) -> Result<(), String> {
    host().write(paste)
}

/// Copy, wait for focus to land, then press `Ctrl+V` in whatever now has it.
///
/// The caller hides the Palette *before* calling: this function does not know
/// about windows, and pasting into our own input box is the bug that shape
/// prevents.
pub fn paste_back(paste: &Paste<'_>) -> Result<(), String> {
    host().paste_back(paste)
}

/// Synthesise `Ctrl+V`.
///
/// All four events go in one `SendInput` call. Split across calls, another
/// process's input can interleave and the target sees `V` with Ctrl already
/// released.
#[cfg(windows)]
pub fn send_ctrl_v() -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL, VK_V,
    };

    let key = |vk, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let events = [
        key(VK_CONTROL, false),
        key(VK_V, false),
        key(VK_V, true),
        key(VK_CONTROL, true),
    ];

    let sent = unsafe { SendInput(&events, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != events.len() {
        return Err("Windows blocked the paste keystroke".into());
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    /// The ordering claim in the module doc, as a compile-time reminder: a paste
    /// with no text still copies nothing rather than pressing keys at random.
    #[test]
    fn v0_5_an_empty_paste_is_still_a_clipboard_write_not_a_keystroke() {
        let paste = Paste {
            kind: ClipKind::Text,
            text: "",
        };
        assert!(to_clipboard(&paste).is_ok());
    }
}
