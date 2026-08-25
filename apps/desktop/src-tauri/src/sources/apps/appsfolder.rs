//! UWP and Store applications, from the shell's `AppsFolder` virtual folder.
//!
//! **A UWP app is not a file**: no path for `CreateProcess`, nothing on disk the
//! other three paths could find. An Application User Model ID identifies it and
//! `shell:AppsFolder\<aumid>` starts it.
//!
//! `AppsFolder` also mirrors Win32 apps from the Start Menu. Those are **skipped**
//! — `lnk.rs` finds them with a real path, which makes a better Entry, and taking
//! both would give one app two ids and split its Frecency from v0.3.

/// One packaged application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackagedApp {
    pub name: String,
    /// The Application User Model ID. Stable across app updates by design, which
    /// is what makes it a sound [`crate::entry::EntryId`] where the display name
    /// beside it is not.
    pub aumid: String,
}

/// Does this parsing name identify a packaged app rather than a file?
///
/// **"Not a path", not "contains a `!`".** A live enumeration here returns
/// `A-Volute.NahimicCompanion.nahimic` — real, launchable, no separator. Keying
/// off `!` drops those with nothing to explain the absence.
pub fn is_aumid(parsing_name: &str) -> bool {
    if parsing_name.trim().is_empty() {
        return false;
    }
    // A shell URI, such as `shell:::{GUID}` for a control-panel item. Real, and
    // not launchable by AUMID.
    if parsing_name.starts_with("shell:") || parsing_name.starts_with("::{") {
        return false;
    }
    // Any path separator at all. This covers UNC (`\\server\share`), drive paths
    // (`C:\...`, `C:/...`) and relative ones in a single check — and an AUMID
    // legitimately cannot contain either separator, so nothing is lost by being
    // broad here.
    !parsing_name.contains('\\') && !parsing_name.contains('/')
}

/// Is this a packaged app a person would want to launch?
///
/// `AppsFolder` carries some furniture — hidden shell entries, per-app helper
/// registrations. Short for the same reason as `lnk.rs`'s noise filter:
/// over-filtering hides real applications.
pub fn is_interesting(name: &str) -> bool {
    !name.trim().is_empty()
}

#[cfg(windows)]
mod com {
    use super::*;
    use windows::Win32::UI::Shell::{
        IEnumShellItems, IShellItem, SHGetKnownFolderItem, BHID_EnumItems, FOLDERID_AppsFolder,
        KF_FLAG_DEFAULT, SIGDN_DESKTOPABSOLUTEPARSING, SIGDN_NORMALDISPLAY,
    };

    /// Enumerate `AppsFolder`, keeping only the packaged entries.
    ///
    /// The caller owns COM initialisation for this thread; see `sources/apps.rs`.
    pub fn discover() -> Vec<PackagedApp> {
        unsafe {
            let Ok(folder) =
                SHGetKnownFolderItem::<IShellItem>(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None)
            else {
                return Vec::new();
            };
            let Ok(items) = folder.BindToHandler::<Option<&windows::Win32::System::Com::IBindCtx>, IEnumShellItems>(
                None,
                &BHID_EnumItems,
            ) else {
                return Vec::new();
            };

            let mut out = Vec::new();
            loop {
                let mut fetched = [None; 1];
                let mut count = 0u32;
                if items.Next(&mut fetched, Some(&mut count)).is_err() || count == 0 {
                    break;
                }
                let Some(item) = fetched[0].take() else { break };

                // The parsing name is what identifies the app; the display name is
                // what the user types. Both are needed, and either can fail
                // independently for a broken registration.
                let Ok(parsing) = display_name(&item, SIGDN_DESKTOPABSOLUTEPARSING) else {
                    continue;
                };
                if !is_aumid(&parsing) {
                    continue;
                }
                let Ok(name) = display_name(&item, SIGDN_NORMALDISPLAY) else {
                    continue;
                };
                if !is_interesting(&name) {
                    continue;
                }
                out.push(PackagedApp {
                    name,
                    aumid: parsing,
                });
            }
            out
        }
    }

    /// Read one of an item's names, freeing the shell's buffer.
        ///
        /// `GetDisplayName` allocates with the COM task allocator and the caller must
        /// release it. Two names for ~200 items is 400 calls per pass, so a leak here
        /// only shows up on a machine with a lot of apps.
    unsafe fn display_name(
        item: &IShellItem,
        kind: windows::Win32::UI::Shell::SIGDN,
    ) -> windows::core::Result<String> {
        use windows::Win32::System::Com::CoTaskMemFree;

        let raw = item.GetDisplayName(kind)?;
        let value = raw.to_string().unwrap_or_default();
        CoTaskMemFree(Some(raw.0 as *const _));
        Ok(value)
    }
}

#[cfg(windows)]
pub fn discover() -> Vec<PackagedApp> {
    com::discover()
}

#[cfg(not(windows))]
pub fn discover() -> Vec<PackagedApp> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_2_a_real_aumid_is_recognised() {
        assert!(is_aumid("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"));
        assert!(is_aumid(
            "windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel"
        ));
        // Observed on this machine, from the live AppsFolder enumeration.
        assert!(is_aumid("A-Volute.NahimicCompanion.nahimic"));
    }

    /// The Win32 entries in `AppsFolder` are `lnk.rs`'s job. Taking them here too
    /// would give the same application two ids, and split its Frecency from v0.3.
    #[test]
    fn v0_2_a_filesystem_path_is_not_an_aumid() {
        assert!(!is_aumid(r"C:\Program Files\App\app.exe"));
        assert!(!is_aumid(r"C:/Program Files/App/app.exe"));
        assert!(!is_aumid(r"\\server\share\app.exe"));
        assert!(!is_aumid(""));
    }

    /// `!` is a legal character in a Windows filename, so its presence cannot be
    /// the test. This path is exactly the kind of thing a person names a folder.
    #[test]
    fn v0_2_a_path_containing_an_exclamation_mark_is_still_a_path() {
        assert!(!is_aumid(r"C:\Games\Wow! Studio\game.exe"));
    }

    /// The counterpart, and the reason the filter is "not a path" rather than
    /// "contains a `!`": a live enumeration on this machine returns this entry,
    /// which is a real launchable app with no separator in its id.
    #[test]
    fn v0_2_an_aumid_without_a_separator_is_still_an_aumid() {
        assert!(is_aumid("A-Volute.NahimicCompanion.nahimic"));
        assert!(is_aumid("9C39BA3AF7ED4288;PrivateBrowsingAUMID"));
    }

    #[test]
    fn v0_2_shell_guid_items_are_not_launchable_by_aumid() {
        assert!(!is_aumid("::{20D04FE0-3AEA-1069-A2D8-08002B30309D}"));
        assert!(!is_aumid("shell:::{21EC2020-3AEA-1069-A2DD-08002B30309D}"));
    }

    #[test]
    fn v0_2_an_unnamed_entry_is_skipped() {
        assert!(!is_interesting(""));
        assert!(!is_interesting("   "));
        assert!(is_interesting("Calculator"));
    }
}
