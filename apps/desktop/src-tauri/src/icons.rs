//! Icon extraction into one memory-mapped blob (IMPLEMENTATION_PLAN §6).
//!
//! Bytes reach the webview through the `takyon-icon` URI scheme, not inside the
//! query response. §6 records why, and why the handler is asynchronous.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// The URI scheme the frontend fetches icons from.
///
/// Must match the string `api.ts` passes to `convertFileSrc`, and the `img-src`
/// entry in `tauri.conf.json`'s CSP. A test below checks all three agree, because
/// a mismatch fails at runtime as "no icons ever appear", with nothing in any log.
pub const SCHEME: &str = "takyon-icon";

/// Square edge, physical pixels, that icons are extracted at.
///
/// Rows are 44 logical tall and the icon takes ~24, so 64 covers a 2x display.
/// Row size looks soft on any modern laptop; 256 quadruples the blob and the
/// decode cost for pixels nobody sees.
pub const ICON_PX: u32 = 64;

/// Where an icon can be extracted from.
///
/// Two shapes because the app kinds differ — a Win32 app has a file, a packaged
/// one has only an AUMID with its assets inside the package.
/// `IShellItemImageFactory` handles both, so the icon path does not fork.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IconSource {
    File(PathBuf),
    Aumid(String),
}

impl IconSource {
    /// The string `SHCreateItemFromParsingName` understands.
    ///
    /// `shell:AppsFolder\<aumid>` names a packaged app as a shell item, and is the
    /// same string `launch.rs` uses to start one. Deliberately identical: if an app
    /// can be launched, its icon can be found.
    pub fn parsing_name(&self) -> String {
        match self {
            IconSource::File(path) => path.to_string_lossy().to_string(),
            IconSource::Aumid(aumid) => format!(r"shell:AppsFolder\{aumid}"),
        }
    }
}

/// FNV-1a, 64-bit.
///
/// Not `DefaultHasher`: `std`'s is documented as unspecified and free to change
/// between releases. A disk-persisted key that changes with the toolchain
/// invalidates every user's icon blob on an unrelated upgrade.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Cache key: the source, plus the mtime of whatever it was extracted from.
///
/// §6's "target path + mtime". The mtime half keeps the cache correct across an
/// app update; without it an updated app keeps its old icon forever. A packaged
/// app has no file to stat, so its key is the AUMID alone.
pub fn key_for(source: &IconSource) -> String {
    let mtime = match source {
        IconSource::File(path) => std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0),
        IconSource::Aumid(_) => 0,
    };
    let mut material = source.parsing_name().to_lowercase().into_bytes();
    material.extend_from_slice(&mtime.to_le_bytes());
    format!("{:016x}", fnv1a(&material))
}

/// Is this a key this module could have produced?
///
/// It arrives from the webview inside a URL and indexes a map, so traversal is
/// impossible regardless. Sixteen lowercase hex digits and nothing else is the
/// belt to that pair of braces, and costs one comparison.
pub fn is_valid_key(key: &str) -> bool {
    key.len() == 16 && key.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// The icon blob and everything needed to fill it.
///
/// Three layers, checked in order on a fetch: `ready` (this session, in memory),
/// `blob` (mapped from previous sessions), then extraction from the shell on the
/// fetching thread — a background one, since the protocol handler is async.
pub struct IconStore {
    /// Where each key can be re-extracted from. Registered when the app list is
    /// built, so a fetch never has to be told which app it is for.
    sources: RwLock<HashMap<String, IconSource>>,
    /// Icons produced this session and not yet flushed to disk.
    ready: RwLock<HashMap<String, Vec<u8>>>,
    /// When the last icon was extracted, for the flush debounce. `None` until one
    /// has been.
    extracted_at: RwLock<Option<Instant>>,
    /// The persisted blob, mapped. `None` until the first flush, and on any
    /// machine where the file cannot be created.
    blob: RwLock<Option<Blob>>,
    dir: Option<PathBuf>,
}

/// The mapped blob plus its offset table.
struct Blob {
    map: memmap2::Mmap,
    index: HashMap<String, (u64, u32)>,
}

/// Bumping this discards every cached icon.
///
/// The escape hatch for the one case the mtime key cannot cover: a packaged app
/// whose icon changed without any file we can stat changing.
const FORMAT_VERSION: u32 = 1;
const MAGIC: &[u8; 4] = b"TKI1";

/// How long extraction must be quiet before the blob is written.
///
/// Extraction is lazy — one icon per row as it is drawn — so there is no single
/// moment when "the icons are ready". A flush rewrites the file whole, so doing
/// it per row would rewrite it once per row.
pub const FLUSH_DEBOUNCE: Duration = Duration::from_millis(750);

/// Is there anything to write, and has extraction stopped?
///
/// The rule v0.2 got wrong: it flushed at a fixed moment (straight after the
/// walk) instead of after extraction, so the file never held an icon. See
/// `docs/tbd/v0.2.md` §10.
pub fn should_flush(pending: usize, idle: Duration) -> bool {
    pending > 0 && idle >= FLUSH_DEBOUNCE
}

impl Default for IconStore {
    fn default() -> Self {
        Self::new(crate::identity::data_dir())
    }
}

impl IconStore {
    pub fn new(dir: Option<PathBuf>) -> Self {
        let store = IconStore {
            sources: RwLock::new(HashMap::new()),
            ready: RwLock::new(HashMap::new()),
            extracted_at: RwLock::new(None),
            blob: RwLock::new(None),
            dir,
        };
        store.load();
        store
    }

    fn blob_path(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join("icons.bin"))
    }

    /// Map the blob written by a previous session.
    ///
    /// Any failure — missing, wrong version, truncated — leaves the store empty
    /// rather than erroring. A corrupt icon cache costs one re-extraction pass;
    /// treating it as fatal would refuse to start because a picture is wrong.
    fn load(&self) {
        let Some(path) = self.blob_path() else { return };
        let Ok(file) = std::fs::File::open(&path) else {
            return;
        };
        let Ok(map) = (unsafe { memmap2::Mmap::map(&file) }) else {
            return;
        };
        let Some(index) = parse_index(&map) else {
            return;
        };
        if let Ok(mut guard) = self.blob.write() {
            *guard = Some(Blob { map, index });
        }
    }

    /// Record where an icon can be extracted from, returning its key.
    ///
    /// Called once per discovered app. Returns `None` when there is no icon source
    /// at all, which is a legitimate state — the row renders its placeholder and
    /// never requests anything.
    pub fn register(&self, source: Option<IconSource>) -> Option<crate::entry::IconRef> {
        let source = source?;
        let key = key_for(&source);
        if let Ok(mut guard) = self.sources.write() {
            guard.insert(key.clone(), source);
        }
        Some(crate::entry::IconRef(key))
    }

    /// PNG bytes for a key, extracting on first use.
    ///
    /// `None` for an unknown key or an icon the shell will not give, and the caller
    /// 404s. The row is already on screen with its placeholder, so that is cosmetic
    /// rather than a failure the user sees.
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        if !is_valid_key(key) {
            return None;
        }
        if let Ok(guard) = self.ready.read() {
            if let Some(bytes) = guard.get(key) {
                return Some(bytes.clone());
            }
        }
        if let Ok(guard) = self.blob.read() {
            if let Some(blob) = guard.as_ref() {
                if let Some((offset, len)) = blob.index.get(key).copied() {
                    let start = offset as usize;
                    let end = start + len as usize;
                    if end <= blob.map.len() {
                        return Some(blob.map[start..end].to_vec());
                    }
                }
            }
        }

        let source = self.sources.read().ok()?.get(key)?.clone();
        let bytes = extract(&source)?;
        if let Ok(mut guard) = self.ready.write() {
            guard.insert(key.to_string(), bytes.clone());
        }
        if let Ok(mut guard) = self.extracted_at.write() {
            *guard = Some(Instant::now());
        }
        Some(bytes)
    }

    /// How many icons have been extracted this session but not yet persisted.
    pub fn pending(&self) -> usize {
        self.ready.read().map(|r| r.len()).unwrap_or(0)
    }

    /// How long since the last extraction, or [`Duration::MAX`] if there has been
    /// none. Paired with [`should_flush`].
    pub fn idle(&self) -> Duration {
        self.extracted_at
            .read()
            .ok()
            .and_then(|g| *g)
            .map(|at| at.elapsed())
            .unwrap_or(Duration::MAX)
    }

    /// Write everything known to `icons.bin` and re-map it.
    ///
    /// Temp file plus rename, because the live map is a view of the file being
    /// replaced. Truncating in place leaves mapped pages pointing at bytes that no
    /// longer mean anything — a fault on Windows, not a wrong answer.
    pub fn flush(&self) -> std::io::Result<()> {
        use std::io::Write;

        // Nothing new: the file on disk already says everything this store knows,
        // so rewriting it would only replace it with itself. v0.2 called this at
        // the one moment that was always true and wrote a 12-byte header instead
        // of an icon cache (tbd v0.2 §10).
        if self.pending() == 0 {
            return Ok(());
        }
        let Some(path) = self.blob_path() else {
            return Ok(());
        };
        let Some(dir) = self.dir.as_ref() else {
            return Ok(());
        };
        std::fs::create_dir_all(dir)?;

        // Everything from the old blob, plus everything new. Merged rather than
        // appended so that the file is rewritten whole and the index can never
        // disagree with the data.
        let mut all: Vec<(String, Vec<u8>)> = Vec::new();
        if let Ok(guard) = self.blob.read() {
            if let Some(blob) = guard.as_ref() {
                for (key, (offset, len)) in &blob.index {
                    let start = *offset as usize;
                    let end = start + *len as usize;
                    if end <= blob.map.len() {
                        all.push((key.clone(), blob.map[start..end].to_vec()));
                    }
                }
            }
        }
        if let Ok(guard) = self.ready.read() {
            for (key, bytes) in guard.iter() {
                if !all.iter().any(|(k, _)| k == key) {
                    all.push((key.clone(), bytes.clone()));
                }
            }
        }

        let tmp = path.with_extension("bin.tmp");
        {
            let mut file = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
            file.write_all(MAGIC)?;
            file.write_all(&FORMAT_VERSION.to_le_bytes())?;
            file.write_all(&(all.len() as u32).to_le_bytes())?;

            // The data starts after the header and the whole index, so every
            // offset can be computed before a single byte of image data is written.
            let index_bytes: usize = all
                .iter()
                .map(|(k, _)| 2 + k.len() + 8 + 4)
                .sum();
            let mut offset = (MAGIC.len() + 4 + 4 + index_bytes) as u64;

            for (key, bytes) in &all {
                file.write_all(&(key.len() as u16).to_le_bytes())?;
                file.write_all(key.as_bytes())?;
                file.write_all(&offset.to_le_bytes())?;
                file.write_all(&(bytes.len() as u32).to_le_bytes())?;
                offset += bytes.len() as u64;
            }
            for (_, bytes) in &all {
                file.write_all(bytes)?;
            }
            file.flush()?;
        }

        // Drop the old map before replacing the file underneath it. Windows will
        // refuse the rename outright while a view is open, which is the good
        // failure mode — but only if we let go first.
        if let Ok(mut guard) = self.blob.write() {
            *guard = None;
        }
        std::fs::rename(&tmp, &path)?;
        if let Ok(mut guard) = self.ready.write() {
            guard.clear();
        }
        self.load();
        Ok(())
    }
}

/// Read the offset table from a mapped blob.
///
/// `None` for anything that does not parse, treated as an empty cache. Every
/// length is checked against the map first: this file sits in a directory the
/// user can write to, so a malformed one really happens.
fn parse_index(map: &[u8]) -> Option<HashMap<String, (u64, u32)>> {
    if map.len() < 12 || &map[..4] != MAGIC {
        return None;
    }
    let version = u32::from_le_bytes(map[4..8].try_into().ok()?);
    if version != FORMAT_VERSION {
        return None;
    }
    let count = u32::from_le_bytes(map[8..12].try_into().ok()?) as usize;

    let mut index = HashMap::with_capacity(count);
    let mut pos = 12usize;
    for _ in 0..count {
        if pos + 2 > map.len() {
            return None;
        }
        let key_len = u16::from_le_bytes(map[pos..pos + 2].try_into().ok()?) as usize;
        pos += 2;
        if pos + key_len + 12 > map.len() {
            return None;
        }
        let key = std::str::from_utf8(&map[pos..pos + key_len]).ok()?.to_string();
        pos += key_len;
        let offset = u64::from_le_bytes(map[pos..pos + 8].try_into().ok()?);
        pos += 8;
        let len = u32::from_le_bytes(map[pos..pos + 4].try_into().ok()?);
        pos += 4;
        if offset as usize + len as usize > map.len() {
            return None;
        }
        index.insert(key, (offset, len));
    }
    Some(index)
}

/// Ask the shell for an icon and encode it as a PNG.
#[cfg(windows)]
pub fn extract(source: &IconSource) -> Option<Vec<u8>> {
    win::extract(source)
}

#[cfg(not(windows))]
pub fn extract(_source: &IconSource) -> Option<Vec<u8>> {
    None
}

#[cfg(windows)]
mod win {
    use super::*;
    use windows::core::HSTRING;
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
    };
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
    };

    pub fn extract(source: &IconSource) -> Option<Vec<u8>> {
        // The protocol handler runs on a thread WebView2 gave us, which has not
        // been initialised for COM. Doing it per extraction is cheap after the
        // first, and the alternative — a dedicated extraction thread — would
        // serialise every icon behind one worker for no gain.
        let initialised = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
        let result = extract_inner(source);
        if initialised {
            unsafe { CoUninitialize() };
        }
        result
    }

    fn extract_inner(source: &IconSource) -> Option<Vec<u8>> {
        unsafe {
            let name = HSTRING::from(source.parsing_name());
            let factory: IShellItemImageFactory =
                SHCreateItemFromParsingName(&name, None).ok()?;

            // ICONONLY, or the shell returns a *thumbnail*: a preview of an exe's
            // contents, or of the document a shortcut points at. BIGGERSIZEOK lets it
            // hand back its 256px asset rather than upscaling a 32px one.
            let bitmap: HBITMAP = factory
                .GetImage(
                    SIZE {
                        cx: ICON_PX as i32,
                        cy: ICON_PX as i32,
                    },
                    SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK,
                )
                .ok()?;

            let png = bitmap_to_png(bitmap);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            png
        }
    }

    /// Pull the pixels out of an `HBITMAP` and encode them.
    ///
    /// `GetDIBits` with a negative height asks for a top-down buffer. Without that
    /// the rows come back bottom-up, which produces an icon that is upside down —
    /// a bug that looks like a rendering problem in the frontend and is not.
    unsafe fn bitmap_to_png(bitmap: HBITMAP) -> Option<Vec<u8>> {
        let mut info = BITMAP::default();
        let read = windows::Win32::Graphics::Gdi::GetObjectW(
            HGDIOBJ(bitmap.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut info as *mut _ as *mut _),
        );
        if read == 0 || info.bmWidth <= 0 || info.bmHeight <= 0 {
            return None;
        }
        let width = info.bmWidth as u32;
        let height = info.bmHeight as u32;

        let mut header = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: info.bmWidth,
                biHeight: -info.bmHeight,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let dc = GetDC(None);
        let copied = GetDIBits(
            dc,
            bitmap,
            0,
            height,
            Some(pixels.as_mut_ptr().cast()),
            &mut header,
            DIB_RGB_COLORS,
        );
        ReleaseDC(None, dc);
        if copied == 0 {
            return None;
        }

        // GDI hands back BGRA; PNG wants RGBA.
        for px in pixels.chunks_exact_mut(4) {
            px.swap(0, 2);
        }

        // A shell icon with no alpha channel at all comes back fully transparent,
        // which renders as nothing and looks exactly like a failed extraction. If
        // every pixel is transparent, treat the whole thing as opaque instead —
        // that is what the shell meant by a 24-bit icon.
        if pixels.iter().skip(3).step_by(4).all(|&a| a == 0) {
            for px in pixels.chunks_exact_mut(4) {
                px[3] = 255;
            }
        }

        encode_png(&pixels, width, height)
    }

    fn encode_png(rgba: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().ok()?;
            writer.write_image_data(rgba).ok()?;
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_2_a_key_is_sixteen_lowercase_hex_digits() {
        let key = key_for(&IconSource::Aumid("Microsoft.Whatever_abc!App".into()));
        assert_eq!(key.len(), 16);
        assert!(is_valid_key(&key), "{key} should be a valid key");
    }

    /// The key arrives from the webview inside a URL. It indexes a map, so
    /// traversal is impossible regardless, but a key that could contain a path
    /// separator is one refactor away from being used as one.
    #[test]
    fn v0_2_a_key_from_the_webview_cannot_be_a_path() {
        assert!(!is_valid_key("../../../etc/passwd"));
        assert!(!is_valid_key(r"..\..\windows"));
        assert!(!is_valid_key(""));
        assert!(!is_valid_key("ABCDEF0123456789"), "uppercase is not produced");
        assert!(!is_valid_key("abc"));
        assert!(!is_valid_key("0123456789abcdef0"), "too long");
    }

    /// §6: keyed by target path **plus mtime**. Without the mtime half, updating
    /// an application leaves its old icon cached forever under a path that still
    /// matches.
    #[test]
    fn v0_2_the_key_changes_when_the_file_changes() {
        let dir = std::env::temp_dir().join("takyon-icon-key");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("app.exe");

        std::fs::write(&file, b"one").unwrap();
        let before = key_for(&IconSource::File(file.clone()));

        // Filesystem timestamps are coarse; set it explicitly rather than sleeping
        // and hoping the granularity cooperated.
        let later = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(2_000_000_000);
        let handle = std::fs::OpenOptions::new().write(true).open(&file).unwrap();
        handle.set_modified(later).unwrap();
        drop(handle);

        let after = key_for(&IconSource::File(file));
        assert_ne!(before, after, "an updated binary must not keep its old icon");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn v0_2_the_same_file_keys_the_same_way_regardless_of_casing() {
        // The same executable arrives from the Start Menu walk and from PATH with
        // different casing. Two keys would mean extracting the same icon twice and
        // storing both.
        let dir = std::env::temp_dir().join("takyon-icon-case");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Thing.exe");
        std::fs::write(&file, b"x").unwrap();

        let upper = key_for(&IconSource::File(file.clone()));
        let lower = key_for(&IconSource::File(PathBuf::from(
            file.to_string_lossy().to_uppercase(),
        )));
        assert_eq!(upper, lower);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A packaged app is named the same way for launching and for icon extraction.
    /// If those two ever diverge, an app that launches will have no icon and the
    /// reason will not be obvious from either side.
    #[test]
    fn v0_2_a_packaged_icon_source_is_named_the_way_it_is_launched() {
        let source = IconSource::Aumid("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".into());
        assert_eq!(
            source.parsing_name(),
            r"shell:AppsFolder\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"
        );
    }

    #[test]
    fn v0_2_registering_no_source_yields_no_icon_ref() {
        let store = IconStore::new(None);
        assert!(store.register(None).is_none());
        let icon = store.register(Some(IconSource::Aumid("A_b!c".into())));
        assert!(icon.is_some());
    }

    /// tbd v0.2 §10, as a regression test.
    ///
    /// v0.2 flushed once per launch, immediately after the walk — the one moment
    /// nothing has been extracted. `icons.bin` was 12 bytes on this machine after
    /// a full day of use: magic, version, and a count of zero.
    #[test]
    fn v0_3_a_flush_with_nothing_extracted_writes_no_blob() {
        let dir = std::env::temp_dir().join("takyon-icon-empty");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let store = IconStore::new(Some(dir.clone()));
        store.flush().unwrap();
        assert!(
            !dir.join("icons.bin").exists(),
            "an empty flush must not leave a 12-byte header behind"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The flush policy, both halves. Extraction happens lazily as rows are
    /// drawn, so the blob is written on a debounce after it stops rather than at
    /// one fixed moment.
    #[test]
    fn v0_3_icons_are_written_once_extraction_settles() {
        assert!(!should_flush(0, Duration::from_secs(60)), "nothing to write");
        assert!(
            !should_flush(4, Duration::ZERO),
            "still extracting — a flush per row rewrites the whole file per row"
        );
        assert!(should_flush(4, FLUSH_DEBOUNCE));
        assert!(should_flush(1, Duration::from_secs(60)));
    }

    /// The round trip the whole file exists for: write a blob, map it back, read
    /// the bytes out by key.
    #[test]
    fn v0_2_the_blob_round_trips_through_the_mapped_file() {
        let dir = std::env::temp_dir().join("takyon-icon-blob");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let store = IconStore::new(Some(dir.clone()));
        let key = "0123456789abcdef".to_string();
        store
            .ready
            .write()
            .unwrap()
            .insert(key.clone(), b"pretend-png-bytes".to_vec());
        store.flush().unwrap();
        assert_eq!(store.pending(), 0, "flush clears what it persisted");

        // A fresh store, as though the app had restarted.
        let reopened = IconStore::new(Some(dir.clone()));
        assert_eq!(reopened.get(&key).as_deref(), Some(&b"pretend-png-bytes"[..]));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn v0_2_several_icons_survive_one_flush() {
        let dir = std::env::temp_dir().join("takyon-icon-many");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let store = IconStore::new(Some(dir.clone()));
        for i in 0..8u64 {
            let key = format!("{i:016x}");
            let bytes = vec![i as u8; (i as usize + 1) * 10];
            store.ready.write().unwrap().insert(key, bytes);
        }
        store.flush().unwrap();

        let reopened = IconStore::new(Some(dir.clone()));
        for i in 0..8u64 {
            let key = format!("{i:016x}");
            let got = reopened.get(&key).expect("every key survives the round trip");
            assert_eq!(got.len(), (i as usize + 1) * 10);
            assert!(got.iter().all(|&b| b == i as u8));
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The blob lives in a directory the user can write to, so a corrupt one is
    /// something that happens rather than something to reason about. It must cost
    /// one re-extraction pass, never a failure to start.
    #[test]
    fn v0_2_a_corrupt_blob_is_treated_as_an_empty_cache() {
        assert!(parse_index(b"").is_none());
        assert!(parse_index(b"NOPE\x01\x00\x00\x00\x00\x00\x00\x00").is_none());
        // Right magic, wrong version — a format bump discards the cache.
        let mut wrong_version = MAGIC.to_vec();
        wrong_version.extend_from_slice(&99u32.to_le_bytes());
        wrong_version.extend_from_slice(&0u32.to_le_bytes());
        assert!(parse_index(&wrong_version).is_none());
        // Right header, index claims an entry the file does not contain.
        let mut truncated = MAGIC.to_vec();
        truncated.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        truncated.extend_from_slice(&1u32.to_le_bytes());
        assert!(parse_index(&truncated).is_none());
    }

    #[test]
    fn v0_2_an_index_pointing_past_the_end_of_the_file_is_rejected() {
        let key = "0123456789abcdef";
        let mut blob = MAGIC.to_vec();
        blob.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        blob.extend_from_slice(&1u32.to_le_bytes());
        blob.extend_from_slice(&(key.len() as u16).to_le_bytes());
        blob.extend_from_slice(key.as_bytes());
        blob.extend_from_slice(&0u64.to_le_bytes());
        blob.extend_from_slice(&9_000_000u32.to_le_bytes());
        assert!(parse_index(&blob).is_none());
    }

    /// An unknown key answers `None` so the fetch 404s and the row keeps its
    /// placeholder. Never a panic: this is reachable from the webview.
    #[test]
    fn v0_2_an_unknown_key_is_a_miss_rather_than_a_panic() {
        let store = IconStore::new(None);
        assert!(store.get("0123456789abcdef").is_none());
        assert!(store.get("nonsense").is_none());
    }

    /// The scheme name is written down in three places that cannot see each other:
    /// here, the CSP in `tauri.conf.json`, and `api.ts`. A mismatch shows up as
    /// "no icons, ever", with nothing in any log to say why.
    #[test]
    fn v0_2_the_icon_scheme_agrees_with_the_csp_and_the_frontend() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

        let conf: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("tauri.conf.json")).unwrap())
                .unwrap();
        let csp = conf["app"]["security"]["csp"].as_str().unwrap();
        assert!(
            csp.contains(&format!("http://{SCHEME}.localhost")),
            "the CSP must allow the icon scheme; got {csp}"
        );

        let api = std::fs::read_to_string(root.join("../src/api.ts")).unwrap();
        assert!(
            api.contains(&format!("\"{SCHEME}\"")),
            "api.ts must build icon URLs with the same scheme name"
        );
    }
}
