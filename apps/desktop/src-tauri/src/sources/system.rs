//! System entries: Windows settings pages and control-panel tasks (task 8).
//!
//! Two halves, one Source (CONTEXT.md: System entry). Settings pages are a
//! **curated `ms-settings:` table** — there is no public enumeration API, and
//! Raycast's ~950 "catalog" entries are a shipped list, not something discovered.
//! Control-panel tasks come from the **All Tasks shell folder**
//! (`::{ED7BA470-8E54-465E-825C-99712043E01C}`), enumerated through the same
//! `IEnumShellItems` path `appsfolder.rs` already walks — no new Windows surface.
//!
//! Both produce [`EntryKind::System`], ranked below applications and never
//! interleaved (the task-4 kind rule). Ids are stable across reinstalls —
//! `ms-settings:<page>` and `system:<task name>`, minted here like recents.

use std::sync::RwLock;
use std::time::{Duration, Instant};

use crate::actions;
use crate::entry::{
    Action, Entry, EntryId, EntryKind, LaunchTarget, Query, Source, SourceId, SOURCE_SHORTLIST,
};
use crate::rank::{self, Haystack};

pub const SOURCE_ID: SourceId = SourceId("system");

/// The curated settings catalog: `(title, ms-settings page, keywords)`.
///
/// Grown from use, not derived. Keywords are exact-match aliases, so `wifi`
/// reaches Network though the title never says it. Page slugs are the stable
/// part; titles track Windows' own copy and may drift between versions.
const SETTINGS: &[(&str, &str, &[&str])] = &[
    ("Bluetooth", "bluetooth", &["bluetooth"]),
    ("Wi-Fi", "network-wifi", &["wifi", "wireless"]),
    ("Network & internet", "network-status", &["network", "internet", "ethernet"]),
    ("Display", "display", &["display", "screen", "resolution", "monitor"]),
    ("Night light", "nightlight", &["nightlight"]),
    ("Sound", "sound", &["sound", "audio", "volume", "speakers", "microphone"]),
    ("Notifications", "notifications", &["notifications", "alerts"]),
    ("Focus assist", "quiethours", &["focus"]),
    ("Power & battery", "powersleep", &["power", "battery", "sleep"]),
    ("Storage", "storagesense", &["storage", "disk"]),
    ("Multitasking", "multitasking", &["multitasking", "snap"]),
    ("Apps & features", "appsfeatures", &["apps", "uninstall", "programs"]),
    ("Default apps", "defaultapps", &["default apps"]),
    ("Startup apps", "startupapps", &["startup"]),
    ("Date & time", "dateandtime", &["date", "time", "clock", "timezone"]),
    ("Language & region", "regionlanguage", &["language", "region", "locale"]),
    ("Typing", "typing", &["typing", "autocorrect"]),
    ("Themes", "themes", &["themes", "theme"]),
    ("Background", "personalization-background", &["background", "wallpaper"]),
    ("Colors", "personalization-colors", &["colors", "accent", "dark mode"]),
    ("Lock screen", "lockscreen", &["lock screen"]),
    ("Taskbar", "taskbar", &["taskbar"]),
    ("Start", "personalization-start", &["start menu"]),
    ("Windows Update", "windowsupdate", &["update", "updates", "upgrade"]),
    ("Sign-in options", "signinoptions", &["sign in", "password", "pin", "hello"]),
    ("Your info", "yourinfo", &["account", "accounts"]),
    ("Mouse", "mousetouchpad", &["mouse", "touchpad", "pointer"]),
    ("Accessibility", "easeofaccess", &["accessibility"]),
    ("Privacy", "privacy", &["privacy"]),
    ("About", "about", &["about", "pc info", "system info"]),
    ("Recovery", "recovery", &["recovery", "reset"]),
    ("Activation", "activation", &["activation", "license"]),
    ("For developers", "developers", &["developer"]),
    ("Clipboard", "clipboard", &["clipboard"]),
    ("Remote Desktop", "remotedesktop", &["remote desktop", "rdp"]),
];

/// The All Tasks ("God Mode") shell folder — every control-panel task in one flat
/// list with human-readable names. `shell:` prefix so `SHCreateItemFromParsingName`
/// resolves it.
pub const ALL_TASKS_FOLDER: &str = "shell:::{ED7BA470-8E54-465E-825C-99712043E01C}";

/// One system entry, held for launch the way a [`crate::sources::recents::Recent`]
/// is.
#[derive(Clone, Debug)]
pub struct SystemEntry {
    pub id: EntryId,
    pub title: String,
    pub target: LaunchTarget,
    pub hay: Haystack,
    /// `System` for a curated page, `SystemTask` for a control-panel task. The
    /// two rank differently, so the Source has to say which this is.
    pub kind: EntryKind,
}

/// Build the settings half from the curated table. Pure — no shell, no COM — so
/// the id rule and keyword wiring are testable directly.
pub fn settings_catalog() -> Vec<SystemEntry> {
    SETTINGS
        .iter()
        .map(|(title, page, keywords)| {
            let mut hay = Haystack::new(title, None);
            // Exact-match keywords (TIER_KEYWORD): `wifi` reaches Network though
            // the title never says it. Not `aliases` — that rung is the user's
            // own naming, and it must outrank one we shipped.
            hay.keywords = keywords.iter().map(|k| k.to_lowercase()).collect();
            SystemEntry {
                id: EntryId(format!("ms-settings:{page}")),
                title: title.to_string(),
                target: LaunchTarget::Uri(format!("ms-settings:{page}")),
                hay,
                kind: EntryKind::System,
            }
        })
        .collect()
}

/// Build one control-panel task from its display name and captured PIDL. Pure, so
/// the id and title rules are testable without the shell folder.
pub fn task_from(name: &str, pidl: Vec<u8>) -> Option<SystemEntry> {
    let name = name.trim();
    if name.is_empty() || pidl.is_empty() {
        return None;
    }
    Some(SystemEntry {
        id: EntryId(format!("system:{}", name.to_lowercase())),
        title: name.to_string(),
        target: LaunchTarget::ShellItem(pidl),
        hay: Haystack::new(name, None),
        kind: EntryKind::SystemTask,
    })
}

/// The System entries Source.
pub struct SystemSource {
    items: RwLock<Vec<SystemEntry>>,
    /// `None` until the first walk completes — an empty list before then is "not
    /// read yet", not "no system entries".
    ready: RwLock<bool>,
}

impl Default for SystemSource {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemSource {
    pub fn new() -> Self {
        SystemSource {
            items: RwLock::new(Vec::new()),
            ready: RwLock::new(false),
        }
    }

    /// Build the catalog: curated settings plus the control-panel walk. Blocking
    /// (COM); call it off the query path, once at startup — the set is static.
    pub fn refresh(&self) {
        let mut items = settings_catalog();
        items.extend(control_panel_tasks());
        if let Ok(mut guard) = self.items.write() {
            *guard = items;
        }
        if let Ok(mut guard) = self.ready.write() {
            *guard = true;
        }
    }

    pub fn len(&self) -> usize {
        self.items.read().map(|i| i.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Has the first walk completed? An empty list before it has is "not read
    /// yet", not "no system entries" — the same distinction `apps.is_indexing`
    /// draws, though no UI surfaces it yet.
    pub fn is_ready(&self) -> bool {
        self.ready.read().map(|r| *r).unwrap_or(false)
    }

    /// Look up one system entry by id, for launching.
    pub fn find(&self, id: &EntryId) -> Option<SystemEntry> {
        self.items.read().ok()?.iter().find(|s| &s.id == id).cloned()
    }

    /// Populate without touching the shell. The seam the tests use.
    #[doc(hidden)]
    pub fn set_for_test(&self, items: Vec<SystemEntry>) {
        if let Ok(mut guard) = self.items.write() {
            *guard = items;
        }
        if let Ok(mut guard) = self.ready.write() {
            *guard = true;
        }
    }
}

impl Source for SystemSource {
    fn id(&self) -> SourceId {
        SOURCE_ID
    }

    fn query(&self, q: &Query, budget: Duration) -> Vec<Entry> {
        if q.is_empty() {
            return Vec::new();
        }
        let deadline = Instant::now() + budget;
        let Ok(items) = self.items.read() else {
            return Vec::new();
        };

        let mut out = Vec::new();
        for (i, item) in items.iter().enumerate() {
            if i % 64 == 0 && Instant::now() > deadline {
                break;
            }
            let Some(score) = rank::score(q, &item.hay) else {
                continue;
            };
            out.push(Entry {
                id: item.id.clone(),
                title: item.title.clone(),
                subtitle: None,
                kind: item.kind,
                icon: None,
                score,
                actions: actions::for_system(),
                version: None,
            });
        }
        rank::order(out, SOURCE_SHORTLIST)
    }

    fn actions(&self, entry: &Entry) -> Vec<Action> {
        actions::for_entry(entry)
    }
}

/// Enumerate the All Tasks folder into control-panel task entries.
///
/// Same COM path as `appsfolder.rs`: bind the folder, walk `IEnumShellItems`,
/// read each item's display and parsing names. Deduped by id, because the folder
/// lists one task under several categories. The caller owns COM for this thread.
#[cfg(windows)]
pub fn control_panel_tasks() -> Vec<SystemEntry> {
    let _com = crate::com::ComScope::new();
    com::discover()
}

#[cfg(not(windows))]
pub fn control_panel_tasks() -> Vec<SystemEntry> {
    Vec::new()
}

#[cfg(windows)]
mod com {
    use super::*;
    use windows::core::HSTRING;
    use windows::Win32::System::Com::{CoTaskMemFree, IBindCtx};
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{
        IEnumShellItems, IShellItem, ILGetSize, SHCreateItemFromParsingName,
        SHGetIDListFromObject, BHID_EnumItems, SIGDN_NORMALDISPLAY,
    };

    pub fn discover() -> Vec<SystemEntry> {
        unsafe {
            let folder: IShellItem = match SHCreateItemFromParsingName(
                &HSTRING::from(ALL_TASKS_FOLDER),
                None,
            ) {
                Ok(f) => f,
                Err(_) => return Vec::new(),
            };
            let Ok(items) =
                folder.BindToHandler::<Option<&IBindCtx>, IEnumShellItems>(None, &BHID_EnumItems)
            else {
                return Vec::new();
            };

            let mut out = Vec::new();
            let mut seen = std::collections::HashSet::new();
            loop {
                let mut fetched = [None; 1];
                let mut count = 0u32;
                if items.Next(&mut fetched, Some(&mut count)).is_err() || count == 0 {
                    break;
                }
                let Some(item) = fetched[0].take() else { break };

                let Ok(name) = display_name(&item, SIGDN_NORMALDISPLAY) else {
                    continue;
                };
                let Some(pidl) = pidl_bytes(&item) else {
                    continue;
                };
                let Some(entry) = task_from(&name, pidl) else {
                    continue;
                };
                if seen.insert(entry.id.clone()) {
                    out.push(entry);
                }
            }
            out
        }
    }

    /// The item's absolute PIDL, copied to bytes and the shell's copy freed.
    ///
    /// An All Tasks item has no reparseable name, so its id-list is the only
    /// launch handle. `SHGetIDListFromObject` allocates with the task allocator;
    /// `ILGetSize` gives the byte length to copy before freeing.
    unsafe fn pidl_bytes(item: &IShellItem) -> Option<Vec<u8>> {
        let pidl: *mut ITEMIDLIST = SHGetIDListFromObject(item).ok()?;
        if pidl.is_null() {
            return None;
        }
        let len = ILGetSize(Some(pidl)) as usize;
        let bytes = std::slice::from_raw_parts(pidl as *const u8, len).to_vec();
        CoTaskMemFree(Some(pidl as *const _));
        (!bytes.is_empty()).then_some(bytes)
    }

    /// Read one of an item's names, freeing the shell's buffer. Same contract as
    /// `appsfolder.rs`'s: `GetDisplayName` allocates with the task allocator.
    unsafe fn display_name(
        item: &IShellItem,
        kind: windows::Win32::UI::Shell::SIGDN,
    ) -> windows::core::Result<String> {
        let raw = item.GetDisplayName(kind)?;
        let value = raw.to_string().unwrap_or_default();
        CoTaskMemFree(Some(raw.0 as *const _));
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_3_a_settings_page_is_keyed_by_its_ms_settings_uri() {
        let catalog = settings_catalog();
        let bt = catalog.iter().find(|s| s.title == "Bluetooth").unwrap();
        assert_eq!(bt.id.as_str(), "ms-settings:bluetooth");
        assert!(matches!(&bt.target, LaunchTarget::Uri(u) if u == "ms-settings:bluetooth"));
    }

    /// The exit criterion, as a unit: `bluetooth` reaches the Bluetooth page.
    #[test]
    fn v0_3_bluetooth_reaches_the_bluetooth_settings_page() {
        let source = SystemSource::new();
        source.set_for_test(settings_catalog());
        let entries = source.query(&Query::new("bluetooth"), Duration::from_millis(20));
        assert_eq!(entries[0].id.as_str(), "ms-settings:bluetooth");
        assert_eq!(entries[0].kind, EntryKind::System);
    }

    /// A keyword the title never carries still reaches the page — `wifi` → Wi-Fi.
    #[test]
    fn v0_3_a_keyword_reaches_a_page_its_title_does_not_name() {
        let source = SystemSource::new();
        source.set_for_test(settings_catalog());
        let entries = source.query(&Query::new("wifi"), Duration::from_millis(20));
        assert_eq!(entries[0].id.as_str(), "ms-settings:network-wifi");
    }

    /// A control-panel task is keyed by name and launched by its captured PIDL.
    #[test]
    fn v0_3_a_control_panel_task_is_keyed_by_name_and_launched_by_pidl() {
        let t = task_from("Change how your keyboard works", vec![1, 2, 3, 4]).unwrap();
        assert_eq!(t.id.as_str(), "system:change how your keyboard works");
        assert!(matches!(t.target, LaunchTarget::ShellItem(_)));
    }

    /// An unnamed task, or one with no id-list, is not an entry.
    #[test]
    fn v0_3_a_task_without_a_name_or_pidl_is_dropped() {
        assert!(task_from("", vec![1, 2]).is_none());
        assert!(task_from("Something", Vec::new()).is_none());
    }

    /// A system entry competes with applications on merit — same rank tier, so
    /// score decides — and still sits above incidental documents.
    #[test]
    fn v0_3_a_system_entry_competes_with_applications_on_merit() {
        assert_eq!(EntryKind::System.tier(), EntryKind::App.tier());
        assert!(EntryKind::System.tier() < EntryKind::File.tier());
        assert!(EntryKind::System.tier() < EntryKind::Recent.tier());
    }

    /// A settings page has no file, so it is offered Open alone — no reveal, no
    /// copy path, nothing to elevate.
    #[test]
    fn v0_3_a_system_entry_is_offered_open_only() {
        let source = SystemSource::new();
        source.set_for_test(settings_catalog());
        let entries = source.query(&Query::new("display"), Duration::from_millis(20));
        assert_eq!(entries[0].actions, vec![actions::OPEN]);
        assert!(!entries[0].actions.contains(&actions::REVEAL));
        assert!(!entries[0].actions.contains(&actions::RUN_AS_ADMIN));
    }

    #[test]
    fn v0_3_an_empty_query_returns_no_system_entries() {
        let source = SystemSource::new();
        source.set_for_test(settings_catalog());
        assert!(source
            .query(&Query::new(""), Duration::from_millis(20))
            .is_empty());
    }

    /// Every curated page has a non-empty slug and a unique id — a copy-paste
    /// slip in the table becomes a failing test rather than a dead row.
    #[test]
    fn v0_3_the_settings_catalog_is_well_formed() {
        let catalog = settings_catalog();
        let mut ids = std::collections::HashSet::new();
        for entry in &catalog {
            assert!(entry.id.as_str().starts_with("ms-settings:"));
            assert!(entry.id.as_str().len() > "ms-settings:".len());
            assert!(ids.insert(entry.id.clone()), "duplicate id {:?}", entry.id);
        }
        assert!(catalog.len() >= 20);
    }
}
