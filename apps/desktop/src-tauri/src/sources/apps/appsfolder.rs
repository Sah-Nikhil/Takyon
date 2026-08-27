//! UWP and Store applications, from the shell's `AppsFolder` virtual folder.
//!
//! **A UWP app is not a file**: no path for `CreateProcess`, nothing on disk the
//! other three paths could find. An Application User Model ID identifies it and
//! `shell:AppsFolder\<aumid>` starts it.
//!
//! `AppsFolder` also mirrors Win32 apps from the Start Menu. Where `lnk.rs`
//! already found one it is **skipped** by title: a real path makes a better
//! Entry, and taking both gives one app two ids and splits its Frecency.
//!
//! The rest are kept, and are the majority — 74 of this machine's 112 AUMIDs are
//! Win32. They are not Store apps and must not be labelled as such; see
//! [`is_packaged`].

/// One `AppsFolder` entry. Packaged or Win32 — the folder lists both, and
/// `is_packaged` is what separates them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellApp {
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

/// Is this AUMID a **packaged** app, or a Win32 registration?
///
/// Package family name: `<name>_<publisher>!<appid>`, publisher 13 lowercase
/// alphanumerics. Not "contains a `!`" — `Start ADB Server` has one and is not
/// packaged. 74 of this machine's 112 are Win32; tbd v0.2 §9.
pub fn is_packaged(aumid: &str) -> bool {
    let Some((family, app_id)) = aumid.split_once('!') else {
        return false;
    };
    let Some((name, publisher)) = family.rsplit_once('_') else {
        return false;
    };
    !name.is_empty()
        && !app_id.is_empty()
        && publisher.len() == 13
        && publisher
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// What goes under the title for an `AppsFolder` row.
///
/// `None` for a Win32 registration: there is no path to show and no package to
/// name, so the row carries its title alone rather than something untrue.
pub fn subtitle(aumid: &str) -> Option<String> {
    is_packaged(aumid).then(|| "Store app".to_string())
}

/// Is this an entry a person would want to launch?
///
/// `AppsFolder` carries furniture — hidden shell entries, helper registrations,
/// and installer debris the `.lnk` walk already dropped. Short for the same
/// reason as the noise filter it calls: over-filtering hides real applications.
pub fn is_interesting(name: &str) -> bool {
    !name.trim().is_empty() && !super::noise::is_noise(name)
}

#[cfg(windows)]
mod com {
    use super::*;
    use windows::Win32::UI::Shell::{
        IEnumShellItems, IShellItem, SHGetKnownFolderItem, BHID_EnumItems, FOLDERID_AppsFolder,
        KF_FLAG_DEFAULT, SIGDN_DESKTOPABSOLUTEPARSING, SIGDN_NORMALDISPLAY,
    };

    /// Enumerate `AppsFolder`, keeping everything launchable by AUMID.
    ///
    /// The caller owns COM initialisation for this thread; see `sources/apps.rs`.
    pub fn discover() -> Vec<ShellApp> {
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
                out.push(ShellApp {
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
pub fn discover() -> Vec<ShellApp> {
    com::discover()
}

#[cfg(not(windows))]
pub fn discover() -> Vec<ShellApp> {
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

    /// Store rows keep their label; Win32 rows get no second line rather than a
    /// wrong one. There is nothing true to say about a row with no path and no
    /// package.
    #[test]
    fn v0_3_only_a_packaged_entry_is_subtitled_store_app() {
        assert_eq!(
            subtitle("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App").as_deref(),
            Some("Store app")
        );
        assert_eq!(subtitle("Microsoft.Windows.Explorer"), None);
        assert_eq!(subtitle("CNEventWindowClass"), None);
    }

    /// Only a package family name means "Store app". Live packaged entries from
    /// this machine.
    #[test]
    fn v0_3_a_package_family_name_is_a_packaged_app() {
        assert!(is_packaged("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"));
        assert!(is_packaged(
            "windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel"
        ));
    }

    /// 74 of this machine's 112 AUMIDs are Win32 registrations. Calling any of
    /// them `Store app` is the lie tbd v0.2 §9 recorded — `explorer` returned
    /// "File Explorer — Store app", `AMD Software: Adrenalin Edition` likewise.
    #[test]
    fn v0_3_a_win32_registration_is_not_a_packaged_app() {
        for aumid in [
            "Microsoft.Windows.Explorer",
            "Microsoft.Windows.ControlPanel",
            "Microsoft.Windows.Shell.RunDialog",
            "CNEventWindowClass",
            "Microsoft.Office.WINWORD.EXE.15",
            "com.squirrel.Discord.Discord",
            "Microsoft.AutoGenerated.{EEE53C58-9AEE-6DD6-73D6-CB0EAD7994EE}",
            "A-Volute.NahimicCompanion.nahimic",
        ] {
            assert!(!is_packaged(aumid), "{aumid}");
        }
    }

    /// The case that rules out "contains a `!`". Real entry on this machine:
    /// it has both a `!` and a `_`, and is not packaged. The publisher id is
    /// 13 lowercase alphanumerics or it is not a package family name.
    #[test]
    fn v0_3_an_exclamation_mark_alone_does_not_make_a_package() {
        assert!(!is_packaged("664~fWx6w8A2!1wg'A4D>(FpPBCfr_+uUuDiMG4YH"));
        assert!(!is_packaged("Thing_short!App"));
        assert!(!is_packaged("Thing_8WEKYB3D8BBWE!App"), "publisher ids are lowercase");
        assert!(!is_packaged("NoBang_8wekyb3d8bbwe"));
    }

    /// Installer debris reaches the Palette through **this** path, not `lnk.rs`.
    ///
    /// "Uninstall Node.js" is filtered from the `.lnk` walk and comes straight
    /// back as `Microsoft.AutoGenerated.{A9EBB164-…}`, above the real Node.js for
    /// the query `node`. Live entries from this machine (tbd v0.2 §9).
    #[test]
    fn v0_3_the_noise_filter_covers_the_appsfolder_path_too() {
        assert!(!is_interesting("Uninstall Node.js"));
        assert!(is_interesting("Node.js command prompt"));
        assert!(is_interesting("Windows Software Development Kit"));
    }
}
