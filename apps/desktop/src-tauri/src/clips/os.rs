//! The OS clipboard, behind a trait (`docs/plans/v0.12-macos.md`, ADR-0025).
//!
//! `store.rs` owns `clips.db` and is portable. Everything that touches the
//! *system* clipboard is not: reading and writing `CF_UNICODETEXT`, the
//! `SendInput` paste chord, and the `AddClipboardFormatListener` watcher. Those
//! four calls are what a macOS target reimplements against `NSPasteboard` and
//! `CGEvent`, and they are gathered here so that port is one file rather than a
//! search.
//!
//! One implementor today. The trait exists to name the boundary, not to allow
//! swapping clipboards at runtime — [`host`] is a `cfg` choice, not a setting.

use std::sync::Arc;

use super::blocklist::Blocklist;
use super::paste::{Paste, FOCUS_SETTLE_MS};
use super::store::ClipStore;

/// The system clipboard: read, write, paste back, and watch for changes.
///
/// `Send + Sync` because the watcher is spawned from `setup` and paste-back runs
/// on a command thread. Every method reports its own failure as a string the
/// Palette can show; none of them panic on a clipboard another process holds.
pub trait ClipboardStore: Send + Sync {
    /// Text on the clipboard now, or `None` when it holds none.
    fn read_text(&self) -> Option<String>;

    /// Replace the clipboard with `text`.
    fn write_text(&self, text: &str) -> Result<(), String>;

    /// Replace the clipboard with an image, given as PNG bytes.
    ///
    /// Defaulted to a refusal: only Windows captures images (v0.14), so only
    /// Windows can hold one to put back. macOS is `docs/tbd/v0.14.md` §1.
    fn write_image(&self, _png: &[u8]) -> Result<(), String> {
        Err("Image clips cannot be pasted on this platform yet.".into())
    }

    /// Synthesise the paste chord into whatever holds focus.
    ///
    /// Separate from [`Self::paste_back`] because the chord is the half that can
    /// be refused — a UIPI boundary on Windows, Accessibility permission on
    /// macOS — and the clipboard write must already have happened by then.
    fn send_paste_chord(&self) -> Result<(), String>;

    /// Start capturing copies into `store`, honouring `blocklist`.
    ///
    /// Spawns its own thread and returns; the watcher lives for the process.
    fn spawn_watcher(&self, store: Arc<ClipStore>, blocklist: Arc<Blocklist>);

    /// Write, wait for focus to settle, then press the chord.
    ///
    /// Defaulted because the ordering is the safety rule, not a platform detail:
    /// a failed chord leaves the user one manual paste away, a failed write
    /// leaves them with nothing. See `paste.rs` for the delay.
    fn paste_back(&self, paste: &Paste<'_>) -> Result<(), String> {
        self.write(paste)?;
        std::thread::sleep(std::time::Duration::from_millis(FOCUS_SETTLE_MS));
        self.send_paste_chord()
    }

    /// Put a clip on the clipboard in its own format. File lists are the arm
    /// still to come.
    fn write(&self, paste: &Paste<'_>) -> Result<(), String> {
        match paste {
            Paste::Text(text) => self.write_text(text),
            Paste::Image(png) => self.write_image(png),
        }
    }
}

/// The clipboard for this target. A `cfg` choice resolved at compile time.
pub fn host() -> &'static dyn ClipboardStore {
    #[cfg(windows)]
    {
        &WindowsClipboard
    }
    #[cfg(target_os = "macos")]
    {
        &MacClipboard
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        &UnsupportedClipboard
    }
}

/// One process-wide clipboard lock, because Win32's is not one.
///
/// `OpenClipboard` refuses another *process* and admits a second *thread* of this
/// one, so the watcher reading while a command writes puts both inside
/// `EmptyClipboard`. Heap corruption, and the retry loop cannot see it coming.
#[cfg(windows)]
pub(crate) fn lock() -> std::sync::MutexGuard<'static, ()> {
    static CLIPBOARD: std::sync::Mutex<()> = std::sync::Mutex::new(());
    CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner())
}

/// Win32: `CF_UNICODETEXT`, `SendInput`, `AddClipboardFormatListener`.
///
/// A unit struct because there is nothing to hold: every call opens and closes
/// the clipboard, and the watcher owns its own message-only window.
#[cfg(windows)]
pub struct WindowsClipboard;

#[cfg(windows)]
impl ClipboardStore for WindowsClipboard {
    fn read_text(&self) -> Option<String> {
        super::watch::read_text()
    }

    fn write_text(&self, text: &str) -> Result<(), String> {
        crate::launch::copy_to_clipboard(text)
    }

    fn write_image(&self, png: &[u8]) -> Result<(), String> {
        let pixels = super::image::decode_png(png)
            .ok_or_else(|| "That image clip could not be decoded.".to_string())?;
        crate::launch::copy_image_to_clipboard(&super::image::to_dib(&pixels), png)
    }

    fn send_paste_chord(&self) -> Result<(), String> {
        super::paste::send_ctrl_v()
    }

    fn spawn_watcher(&self, store: Arc<ClipStore>, blocklist: Arc<Blocklist>) {
        super::watch::spawn(store, blocklist);
    }
}

/// macOS: `pbpaste` and `pbcopy`, which are `NSPasteboard` with a shell in front.
///
/// Deliberately not an `objc2` binding yet. Two processes per copy is more than
/// `NSPasteboard` costs, and the paths that use it are user-initiated rather than
/// on any latency budget — Copy and Copy path, never the Bangless walk.
#[cfg(target_os = "macos")]
pub struct MacClipboard;

#[cfg(target_os = "macos")]
impl ClipboardStore for MacClipboard {
    fn read_text(&self) -> Option<String> {
        use std::process::{Command, Stdio};

        let out = Command::new("/usr/bin/pbpaste")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        // Empty is `None`, matching Windows: an empty clipboard holds no text
        // rather than holding an empty string, and `acceptable` rejects it
        // anyway.
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        (!text.is_empty()).then_some(text)
    }

    fn write_text(&self, text: &str) -> Result<(), String> {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut child = Command::new("/usr/bin/pbcopy")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("could not run pbcopy: {e}"))?;

        // Dropped before the wait: `pbcopy` reads to EOF, so holding the pipe
        // open here is a deadlock rather than a slow write.
        {
            let stdin = child
                .stdin
                .as_mut()
                .ok_or_else(|| "pbcopy took no stdin".to_string())?;
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| format!("could not write to pbcopy: {e}"))?;
        }
        drop(child.stdin.take());

        match child.wait() {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(format!("pbcopy exited {}", status.code().unwrap_or(-1))),
            Err(e) => Err(format!("pbcopy could not be waited on: {e}")),
        }
    }

    fn send_paste_chord(&self) -> Result<(), String> {
        // `CGEventPost` needs the Accessibility permission and a Core Graphics
        // binding, neither of which exists yet. Refusing leaves the clip on the
        // clipboard and the user one Cmd+V away, which is the ordering `paste.rs`
        // is built around.
        Err(
            "Paste-back needs the Accessibility permission, which Takyon does not request yet."
                .into(),
        )
    }

    fn spawn_watcher(&self, _store: Arc<ClipStore>, _blocklist: Arc<Blocklist>) {
        // `NSPasteboard` has no change notification — the documented way is to
        // poll `changeCount`, which contradicts ADR-0003's idle-and-warm premise
        // as directly as `GetClipboardSequenceNumber` did on Windows. Left unbuilt
        // rather than built badly; `docs/plans/v0.12-macos.md` row 6 owns it.
    }
}

/// Every target that is neither Windows nor macOS, until one is written.
///
/// Refuses in words rather than silently doing nothing: clipboard history that
/// is merely absent reads as a broken feature, and this string reaches the
/// Palette.
#[cfg(not(any(windows, target_os = "macos")))]
pub struct UnsupportedClipboard;

#[cfg(not(any(windows, target_os = "macos")))]
impl ClipboardStore for UnsupportedClipboard {
    fn read_text(&self) -> Option<String> {
        None
    }

    fn write_text(&self, _text: &str) -> Result<(), String> {
        Err("the clipboard is not implemented on this platform".into())
    }

    fn send_paste_chord(&self) -> Result<(), String> {
        Err("paste-back is not implemented on this platform".into())
    }

    fn spawn_watcher(&self, _store: Arc<ClipStore>, _blocklist: Arc<Blocklist>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The defaulted `write` dispatches on kind rather than assuming text, and
    /// `paste_back` writes before it presses anything. Both are checked through a
    /// fake, because the real one would move the machine's clipboard.
    #[derive(Default)]
    struct Fake {
        written: std::sync::Mutex<Vec<String>>,
        chords: std::sync::atomic::AtomicUsize,
    }

    impl ClipboardStore for Fake {
        fn read_text(&self) -> Option<String> {
            None
        }
        fn write_text(&self, text: &str) -> Result<(), String> {
            self.written
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(text.to_string());
            Ok(())
        }
        fn send_paste_chord(&self) -> Result<(), String> {
            self.chords
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        }
        fn spawn_watcher(&self, _store: Arc<ClipStore>, _blocklist: Arc<Blocklist>) {}
    }

    #[test]
    fn v0_11_paste_back_writes_before_it_presses() {
        let fake = Fake::default();
        fake.paste_back(&Paste::Text("hello")).unwrap();

        let written = fake.written.lock().unwrap();
        assert_eq!(written.as_slice(), ["hello"]);
        assert_eq!(fake.chords.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    /// One test at a time on the real clipboard. `lock` covers one open/close,
    /// not a write-then-read, so two parallel round trips read each other's.
    #[cfg(windows)]
    fn real_clipboard() -> std::sync::MutexGuard<'static, ()> {
        static REAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
        REAL.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The real Win32 path, end to end, because the fake above proves only that
    /// the defaults call in the right order. `query.rs` and `paste.rs` both route
    /// through here now, so a broken `write_text` is a silent Copy that does
    /// nothing. Restores whatever was on the clipboard first.
    #[cfg(windows)]
    #[test]
    fn v0_11_the_windows_clipboard_round_trips() {
        let _real = real_clipboard();
        let clipboard = host();
        let before = clipboard.read_text();

        clipboard.write_text("takyon round trip").unwrap();
        assert_eq!(clipboard.read_text().as_deref(), Some("takyon round trip"));

        if let Some(before) = before {
            let _ = clipboard.write_text(&before);
        }
    }

    /// Copying a clip must never synthesise a keystroke: `COPY_CLIP` and `PASTE`
    /// are two actions, and the first one leaving the target window alone is the
    /// whole difference between them.
    #[test]
    fn v0_11_a_plain_write_presses_nothing() {
        let fake = Fake::default();
        fake.write(&Paste::Text("hello")).unwrap();

        assert_eq!(fake.chords.load(std::sync::atomic::Ordering::Relaxed), 0);
    }

    /// A platform with no image arm refuses in words and presses nothing,
    /// rather than pasting whatever text was on the clipboard before.
    #[test]
    fn v0_14_an_image_on_a_platform_without_images_is_refused() {
        let fake = Fake::default();
        assert!(fake.paste_back(&Paste::Image(&[1, 2, 3])).is_err());
        assert_eq!(fake.chords.load(std::sync::atomic::Ordering::Relaxed), 0);
        assert!(fake.written.lock().unwrap().is_empty());
    }

    /// The real Win32 image path: write a clip, read the DIB back as the
    /// watcher would, and get the same pixels. Restores the text first held.
    #[cfg(windows)]
    #[test]
    fn v0_14_the_windows_clipboard_round_trips_an_image() {
        use super::super::image::{decode_png, encode_png, Rgba};

        let _real = real_clipboard();
        let clipboard = host();
        let before = clipboard.read_text();
        let shot = Rgba {
            width: 3,
            height: 2,
            pixels: (0..24)
                .map(|i| if i % 4 == 3 { 255 } else { i * 10 })
                .collect(),
        };
        clipboard.write_image(&encode_png(&shot).unwrap()).unwrap();
        let read = super::super::watch::read_image();
        if let Some(before) = before {
            let _ = clipboard.write_text(&before);
        }
        assert_eq!(read, Some(shot.clone()));
        assert!(decode_png(&encode_png(&shot).unwrap()).is_some());
    }
}
