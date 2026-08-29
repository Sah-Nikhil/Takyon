//! The version stamped into an executable, for telling two copies apart.
//!
//! Windows binaries carry a `VS_FIXEDFILEINFO` resource. Reading it is file I/O,
//! so it happens **once at discovery** beside the icon key, never per keystroke.
//! Two installs of one tool — `node.exe` under nvm and under Program Files — are
//! different applications the user cannot tell apart by name, and the version is
//! the only thing on the machine that separates them.

use std::path::Path;

/// Trim a four-part file version to something worth reading.
///
/// `22.11.0.0` is `22.11`, `1.2.3.0` is `1.2.3`. Trailing zeroes carry no
/// information and cost the width the title needs.
pub fn tidy(raw: &str) -> String {
    let parts: Vec<&str> = raw.split('.').collect();
    let mut end = parts.len();
    while end > 1 && parts[end - 1] == "0" {
        end -= 1;
    }
    parts[..end].join(".")
}

/// The file version of an executable, already tidied.
#[cfg(windows)]
pub fn of(path: &Path) -> Option<String> {
    win::read(path).map(|v| tidy(&v)).filter(|v| v != "0")
}

#[cfg(not(windows))]
pub fn of(_path: &Path) -> Option<String> {
    None
}

#[cfg(windows)]
mod win {
    use super::*;
    use windows::core::HSTRING;
    use windows::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
    };

    /// `VS_FIXEDFILEINFO`, not the string table.
    ///
    /// The string table is per-language and a binary may carry none, several, or
    /// one in a language nobody here reads. The fixed block is one shape, always
    /// in the same place, and is what Explorer's own Details tab shows.
    pub fn read(path: &Path) -> Option<String> {
        let wide = HSTRING::from(path.as_os_str());
        let size = unsafe { GetFileVersionInfoSizeW(&wide, None) };
        if size == 0 {
            return None;
        }

        let mut buffer = vec![0u8; size as usize];
        unsafe { GetFileVersionInfoW(&wide, None, size, buffer.as_mut_ptr().cast()) }.ok()?;

        let mut fixed: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut len: u32 = 0;
        unsafe {
            VerQueryValueW(
                buffer.as_ptr().cast(),
                &HSTRING::from("\\"),
                &mut fixed,
                &mut len,
            )
        }
        .ok()
        .ok()?;
        if fixed.is_null() || (len as usize) < std::mem::size_of::<VS_FIXEDFILEINFO>() {
            return None;
        }

        let info = unsafe { &*(fixed as *const VS_FIXEDFILEINFO) };
        let (ms, ls) = (info.dwFileVersionMS, info.dwFileVersionLS);
        Some(format!(
            "{}.{}.{}.{}",
            ms >> 16,
            ms & 0xFFFF,
            ls >> 16,
            ls & 0xFFFF
        ))
    }
}

/// Is a version worth showing for this group of same-named executables?
///
/// Only when they disagree. `powershell.exe` ships identically in `System32` and
/// `SysWOW64`, and stamping `6.2.26100.8875` on both rows adds width and no
/// information — ADR-0016's rule, applied to a version rather than a path.
pub fn tells_apart(versions: &[Option<String>]) -> bool {
    let mut seen = std::collections::HashSet::new();
    for v in versions {
        seen.insert(v.as_deref());
    }
    seen.len() > 1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two Node installs, which is the case this exists for.
    #[test]
    fn v0_3_differing_versions_are_worth_showing() {
        assert!(tells_apart(&[Some("24.14.1".into()), Some("26.7".into())]));
    }

    /// The 64-bit and 32-bit copies of one Windows binary.
    #[test]
    fn v0_3_one_binary_shipped_twice_shows_no_version() {
        let same = Some("6.2.26100.8875".to_string());
        assert!(!tells_apart(&[same.clone(), same]));
    }

    /// One copy carrying a version and one carrying none still tells them apart.
    #[test]
    fn v0_3_a_missing_version_is_itself_a_difference() {
        assert!(tells_apart(&[Some("1.0".into()), None]));
        assert!(!tells_apart(&[None, None]));
    }

    /// Trailing zeroes are padding, not precision.
    #[test]
    fn v0_3_a_version_drops_its_trailing_zeroes() {
        assert_eq!(tidy("22.11.0.0"), "22.11");
        assert_eq!(tidy("1.2.3.0"), "1.2.3");
        assert_eq!(tidy("10.0.26100.1"), "10.0.26100.1");
    }

    /// Never to nothing: a version of all zeroes is absent, not "".
    #[test]
    fn v0_3_an_all_zero_version_collapses_to_a_single_zero() {
        assert_eq!(tidy("0.0.0.0"), "0");
    }

    /// A file with no version resource has none, and that is not an error.
    #[test]
    fn v0_3_something_without_a_version_resource_reports_none() {
        assert_eq!(of(std::path::Path::new(r"C:\this\does\not\exist.exe")), None);
    }
}
