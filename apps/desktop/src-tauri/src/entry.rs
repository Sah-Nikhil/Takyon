//! Core types and the [`Source`] trait (IMPLEMENTATION_PLAN §2).
//!
//! One type crosses the IPC boundary: [`Entry`]. Keeping the rest internal is what
//! keeps TBC-0002's no-webview escape hatch cheap.
//!
//! **Nothing here may know about the UI.** Easiest place to break that — a stray
//! `is_selected` in the shared vocabulary looks harmless.

use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;

/// How long a Source has to answer one keystroke (§3).
///
/// Miss it and contribute nothing for that keystroke — no partial results, no
/// late insertion. Makes v0.3's Stability rule fall out of the pipeline.
pub const SOURCE_BUDGET: Duration = Duration::from_millis(20);

/// How many Entries the Palette is ever sent (§3).
pub const MAX_ENTRIES: usize = 12;

/// How many a Source hands the pipeline before Frecency has been applied.
///
/// Wider than [`MAX_ENTRIES`]: usage folds in after the fan-out, so cutting to
/// twelve on match quality alone could discard a much-used Entry one step before
/// its lift. `rank::FRECENCY_LIFT` bounds that lift, so wider is enough.
pub const SOURCE_SHORTLIST: usize = 64;

/// The shortlist must leave the pipeline something to choose from. A `const`
/// block, so narrowing it below the visible limit fails the build rather than a
/// test run nobody did.
const _: () = assert!(SOURCE_SHORTLIST > MAX_ENTRIES);

/// Stable identity for an Entry, and the Frecency key from v0.3.
///
/// §2: resolved target path for an App, full path for a File, row id for a Clip.
/// **Never a hash of the display name** — names change on update, silently
/// resetting learned ranking. Build via [`EntryId::for_launch`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct EntryId(pub String);

impl EntryId {
    /// Canonical id for something launchable.
    ///
    /// Lowercased: one exe reached two ways must be one id or Frecency splits.
    /// AUMIDs and Steam ids are prefixed instead. **Arguments join it where a
    /// shortcut has them** — ADR-0014 amended at v0.3, `|` is illegal in a path.
    pub fn for_launch(target: &LaunchTarget) -> Self {
        EntryId(match target {
            LaunchTarget::Exe { path, args, .. } => {
                let path = path.to_string_lossy().to_lowercase();
                // Argument-free stays byte-identical to v0.2, so no learned id is
                // invalidated. Blank counts as absent — an empty `GetArguments`
                // must not fork the id.
                match args.as_deref().map(str::trim).filter(|a| !a.is_empty()) {
                    Some(args) => format!("{path}|{}", args.to_lowercase()),
                    None => path,
                }
            }
            LaunchTarget::Aumid(aumid) => format!("aumid:{aumid}"),
            LaunchTarget::SteamGame(app_id) => format!("steam:{app_id}"),
        })
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What an Entry is, and how it sorts against other kinds.
///
/// §3: **Apps always sort above documents.** Applied after scoring, because a
/// file legitimately can outscore an app — it just must not win.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    App,
    File,
    Folder,
    Clip,
    Calc,
    Recent,
}

impl EntryKind {
    /// Sort rank, lower first. Only `App` exists at v0.2; the rest are written
    /// down once here rather than rediscovered per Source.
    pub fn tier(self) -> u8 {
        match self {
            // Wins outright: an expression is unambiguous. Nobody typing `17*23`
            // meant an app.
            EntryKind::Calc => 0,
            EntryKind::App => 1,
            EntryKind::Folder => 2,
            EntryKind::File => 3,
            EntryKind::Recent => 4,
            // Unreachable from Bangless (ADR-0006). Last anyway, so a future
            // mistake surfaces at the bottom rather than promoting a secret.
            EntryKind::Clip => 5,
        }
    }
}

/// Key into the icon blob. Not a path, not bytes.
///
/// The frontend turns it into a `takyon-icon://` URL. Bytes in the query response
/// would hit the IPC serialiser every keystroke against a 30 ms budget. See
/// `icons.rs` and IMPLEMENTATION_PLAN §6.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IconRef(pub String);

/// How to start something. Three shapes because Windows has three.
///
/// A UWP app has no path for `CreateProcess`; a Steam game must go through the
/// client for its DRM and cloud-save handshake. Collapsing these into "a command
/// line" is what makes UWP look impossible later.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchTarget {
    Exe {
        path: PathBuf,
        args: Option<String>,
        working_dir: Option<PathBuf>,
    },
    /// An Application User Model ID, launched via `shell:AppsFolder\<aumid>`.
    Aumid(String),
    /// A Steam app id, launched via `steam://rungameid/<id>`.
    SteamGame(u32),
}

/// One actionable row (CONTEXT.md: Entry, never "result").
///
/// `camelCase` on the wire to match `packages/shared/src/ipc.ts`; a contract test
/// asserts they agree.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: EntryId,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    pub kind: EntryKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<IconRef>,
    pub score: f32,
    /// Ids only. Labels live in `actions.rs`, so the query response does not
    /// re-ship the same strings every keystroke.
    pub actions: Vec<ActionId>,
    /// Shown beside the title where two same-named executables disagree — two
    /// Node installs, two R installs. Absent everywhere else, which is most
    /// rows: see `version.rs` for why it is not read for everything.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Id of one `Ctrl+K` action.
///
/// Newtype not enum: Modes contribute actions too (v0.8, v0.9), and an enum would
/// mean every Mode editing a central type it does not own.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct ActionId(pub &'static str);

impl ActionId {
    pub fn as_str(&self) -> &'static str {
        self.0
    }
}

/// One row of the `Ctrl+K` menu, as the frontend needs to draw it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub id: ActionId,
    pub label: String,
    /// Accelerator text (`Ctrl+Enter`), or absent. Task 9: discoverable inside
    /// the menu rather than folklore.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accelerator: Option<String>,
}

/// Which Source produced an Entry. Diagnostics and budget accounting, never
/// ranking.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct SourceId(pub &'static str);

/// One keystroke, chewed once for every Source.
///
/// Five Sources by v0.7 all want the same lowercased needle. Per-Source would be
/// five allocations per keystroke for one result.
#[derive(Clone, Debug)]
pub struct Query {
    /// Exactly what the user typed, for display and for Modes that need it raw.
    pub raw: String,
    /// Lowercased and trimmed. What matching actually runs against.
    pub needle: String,
}

impl Query {
    pub fn new(raw: &str) -> Self {
        Query {
            raw: raw.to_string(),
            needle: raw.trim().to_lowercase(),
        }
    }

    /// Empty query returns nothing, not everything.
    ///
    /// ADR-0001: the Palette opens empty. Listing every app would also snap the
    /// window to full height on every summon — TBC-0006's predicted jank.
    pub fn is_empty(&self) -> bool {
        self.needle.is_empty()
    }
}

/// A producer of Entries for Bangless queries (CONTEXT.md: Source, never
/// "provider").
pub trait Source: Send + Sync {
    fn id(&self) -> SourceId;

    /// Answer within `budget`, or return nothing.
    ///
    /// Passed rather than assumed so a Source chooses how to degrade — checking
    /// between chunks beats being cut off mid-write. `query.rs` enforces it
    /// regardless, so ignoring it is slow, not dangerous.
    fn query(&self, q: &Query, budget: Duration) -> Vec<Entry>;

    /// Which actions this Source offers for one of its own Entries.
    fn actions(&self, entry: &Entry) -> Vec<Action>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §2's rule as an assertion. Reaching for `hash(entry.title)` fails here.
    #[test]
    fn v0_2_an_app_id_is_its_target_path_not_its_name() {
        let target = LaunchTarget::Exe {
            path: PathBuf::from(r"C:\Program Files\Microsoft VS Code\Code.exe"),
            args: None,
            working_dir: None,
        };
        let id = EntryId::for_launch(&target);
        assert!(id.as_str().contains("code.exe"));
        assert!(
            !id.as_str().contains("visual studio"),
            "the id must not be derived from the display name"
        );
    }

    /// Start Menu and PATH spell the same exe with different casing. One id, or
    /// it appears twice and splits its Frecency from v0.3.
    #[test]
    fn v0_2_the_same_exe_under_different_casing_is_one_id() {
        let a = EntryId::for_launch(&LaunchTarget::Exe {
            path: PathBuf::from(r"C:\Windows\System32\notepad.exe"),
            args: None,
            working_dir: None,
        });
        let b = EntryId::for_launch(&LaunchTarget::Exe {
            path: PathBuf::from(r"c:\windows\system32\NOTEPAD.EXE"),
            args: None,
            working_dir: None,
        });
        assert_eq!(a, b);
    }

    /// Amended at v0.3: arguments **are** identity where they exist.
    ///
    /// Nine Start Menu shortcuts here point at `cmd.exe` and are nine different
    /// applications. Was `v0_2_launch_arguments_do_not_change_identity`; ADR-0014
    /// and `docs/tbd/v0.2.md` §9 carry the measurement.
    #[test]
    fn v0_3_launch_arguments_are_part_of_identity() {
        let path = PathBuf::from(r"C:\Windows\System32\cmd.exe");
        let id = |args: Option<&str>| {
            EntryId::for_launch(&LaunchTarget::Exe {
                path: path.clone(),
                args: args.map(|a| a.to_string()),
                working_dir: None,
            })
        };
        assert_ne!(
            id(Some(r"/k C:\vs\VsDevCmd.bat")),
            id(Some(r"/k C:\kicad\env.bat"))
        );
        assert_ne!(id(None), id(Some("/k thing.bat")));
    }

    /// The argument-free id stays **byte-identical** to what v0.2 wrote.
    ///
    /// Everything already learned is keyed on it, so folding arguments in has to
    /// be additive rather than a re-spelling of every id that exists.
    #[test]
    fn v0_3_an_argument_free_id_is_unchanged_by_the_amendment() {
        let id = EntryId::for_launch(&LaunchTarget::Exe {
            path: PathBuf::from(r"C:\Program Files\Microsoft VS Code\Code.exe"),
            args: None,
            working_dir: None,
        });
        assert_eq!(id.as_str(), r"c:\program files\microsoft vs code\code.exe");
    }

    /// Blank arguments are no arguments. A shortcut storing an empty string must
    /// not get a different id from the same shortcut storing nothing.
    #[test]
    fn v0_3_blank_arguments_are_the_same_as_none() {
        let mk = |args: Option<&str>| {
            EntryId::for_launch(&LaunchTarget::Exe {
                path: PathBuf::from(r"C:\app\thing.exe"),
                args: args.map(|a| a.to_string()),
                working_dir: None,
            })
        };
        assert_eq!(mk(None), mk(Some("")));
        assert_eq!(mk(None), mk(Some("   ")));
    }

    /// Working directory is not identity. Two shortcuts differing only in where
    /// they start are one application.
    #[test]
    fn v0_3_the_working_directory_does_not_change_identity() {
        let mk = |dir: Option<&str>| {
            EntryId::for_launch(&LaunchTarget::Exe {
                path: PathBuf::from(r"C:\app\thing.exe"),
                args: Some("--safe-mode".into()),
                working_dir: dir.map(PathBuf::from),
            })
        };
        assert_eq!(mk(None), mk(Some(r"C:\app")));
    }

    /// No path, so the id is the AUMID — stable across updates by design, unlike
    /// the display name beside it.
    #[test]
    fn v0_2_uwp_and_steam_ids_are_namespaced_away_from_paths() {
        let uwp = EntryId::for_launch(&LaunchTarget::Aumid(
            "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".into(),
        ));
        let steam = EntryId::for_launch(&LaunchTarget::SteamGame(440));
        assert!(uwp.as_str().starts_with("aumid:"));
        assert!(steam.as_str().starts_with("steam:"));
        // A Windows path can never begin with either prefix, so no scheme collides.
        assert_ne!(uwp, steam);
    }

    /// §3: Apps always sort above documents, never interleaved by raw score.
    #[test]
    fn v0_2_apps_outrank_documents_by_kind() {
        assert!(EntryKind::App.tier() < EntryKind::File.tier());
        assert!(EntryKind::App.tier() < EntryKind::Folder.tier());
        assert!(EntryKind::App.tier() < EntryKind::Recent.tier());
    }

    /// ADR-0006: Clips never reach a Bangless list. If one ever does, it lands at
    /// the bottom where it is visible, not beside the top Entry.
    #[test]
    fn v0_2_clips_rank_last_if_one_ever_leaks_into_a_bangless_list() {
        for kind in [
            EntryKind::App,
            EntryKind::File,
            EntryKind::Folder,
            EntryKind::Calc,
            EntryKind::Recent,
        ] {
            assert!(kind.tier() < EntryKind::Clip.tier());
        }
    }

    /// ADR-0001: the Palette opens empty and stays empty until something is typed.
    #[test]
    fn v0_2_whitespace_only_input_is_an_empty_query() {
        assert!(Query::new("").is_empty());
        assert!(Query::new("   \t ").is_empty());
        assert!(!Query::new("  code ").is_empty());
        assert_eq!(Query::new("  CoDe ").needle, "code");
    }

    /// `?:` in TypeScript means absent, not null. `null` would make every
    /// optional-chained read in the UI succeed with the wrong answer.
    #[test]
    fn v0_2_absent_optionals_serialise_as_absent_not_null() {
        let e = Entry {
            id: EntryId("x".into()),
            title: "Thing".into(),
            subtitle: None,
            kind: EntryKind::App,
            icon: None,
            score: 1.0,
            actions: vec![],
            version: None,
        };
        let v: serde_json::Value = serde_json::to_value(&e).unwrap();
        assert!(v.get("subtitle").is_none());
        assert!(v.get("icon").is_none());
        assert_eq!(v["kind"].as_str(), Some("app"));
    }
}
