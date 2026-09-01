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
            LaunchTarget::Game { launcher, id } => {
                format!("{}:{}", launcher.slug(), id.to_lowercase())
            }
            // System entries mint their own ids (`system:` / `ms-settings:`) in
            // `sources/system.rs`, like recents — PIDL and URI are launch detail,
            // not identity, and a PIDL is per-session. These arms keep the
            // constructor total; a system entry never reaches them.
            LaunchTarget::ShellItem(_) => "shell-item".to_string(),
            LaunchTarget::Uri(uri) => uri.to_lowercase(),
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
    /// A curated Windows settings page (v0.3 task 8) — `ms-settings:bluetooth`.
    /// A destination you ask for by name, so it shares the App tier.
    System,
    /// A control-panel task from the All Tasks folder — "Change how your keyboard
    /// works". 198 of them, all long sentences that only ever match by word
    /// prefix, so they sit below every app rather than competing with one.
    SystemTask,
}

impl EntryKind {
    /// Sort rank, lower first. Only `App` exists at v0.2; the rest are written
    /// down once here rather than rediscovered per Source.
    pub fn tier(self) -> u8 {
        match self {
            // Wins outright: an expression is unambiguous. Nobody typing `17*23`
            // meant an app.
            EntryKind::Calc => 0,
            // App and System share a tier: both are launch destinations, so
            // neither gates the other. They do not compete evenly — see
            // `weight`, which handicaps System by 20% after Frecency.
            EntryKind::App | EntryKind::System => 1,
            // Below every app, unlike the curated pages above. Nobody types three
            // letters hoping for "Change the way currency is displayed".
            EntryKind::SystemTask => 2,
            EntryKind::Folder => 3,
            EntryKind::File => 4,
            EntryKind::Recent => 5,
            // Unreachable from Bangless (ADR-0006). Last anyway, so a future
            // mistake surfaces at the bottom rather than promoting a secret.
            EntryKind::Clip => 6,
        }
    }

    /// How much this Kind counts once Frecency has had its say.
    ///
    /// `dis` matched Discord and the Display page equally, and a 0.3% Frecency
    /// gap decided the top row — a coin flip. System takes a 20% handicap: an
    /// app is what a launcher is for. Numbers in `docs/tbd/v0.3.md` §10.
    pub fn weight(self) -> f32 {
        match self {
            EntryKind::System | EntryKind::SystemTask => 0.8,
            _ => 1.0,
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

/// Which game launcher a game belongs to.
///
/// The slug is half the EntryId, so it is frozen: changing one resets that
/// launcher's Frecency. `steam` is byte-identical to the v0.2 id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GameLauncher {
    Steam,
    Epic,
}

impl GameLauncher {
    /// EntryId namespace. Never change one of these.
    pub fn slug(self) -> &'static str {
        match self {
            GameLauncher::Steam => "steam",
            GameLauncher::Epic => "epic",
        }
    }

    /// Second line on the Entry: which launcher this came from.
    pub fn label(self) -> &'static str {
        match self {
            GameLauncher::Steam => "Steam",
            GameLauncher::Epic => "Epic",
        }
    }

    /// URI that starts the game through its own launcher.
    ///
    /// `rungameid`, not `run`: `run` skips the user's launch options, which is
    /// how mods and controller profiles get applied. Epic's `silent=true`
    /// suppresses the launcher window.
    pub fn uri(self, id: &str) -> String {
        match self {
            GameLauncher::Steam => format!("steam://rungameid/{id}"),
            GameLauncher::Epic => {
                format!("com.epicgames.launcher://apps/{id}?action=launch&silent=true")
            }
        }
    }
}

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
    /// A game, started through its launcher's URI rather than its executable:
    /// run directly most refuse to start, and none gets cloud saves or playtime.
    /// The id is the launcher's own — Steam app id, Epic `AppName`.
    Game { launcher: GameLauncher, id: String },
    /// A shell item as its absolute PIDL in bytes (task 8 control-panel tasks).
    /// No path, no AUMID, no reparseable name — an All Tasks item is positional,
    /// so the PIDL captured at enumeration is the only handle. Per-session, never
    /// persisted; launched by its default verb through `SEE_MASK_IDLIST`.
    ShellItem(Vec<u8>),
    /// A URI the shell knows how to open — `ms-settings:bluetooth` (task 8's
    /// settings pages). Launched straight through `ShellExecuteW`, like a
    /// `steam://` URL.
    Uri(String),
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

    /// The two launch URIs, asserted as literals rather than rebuilt the way the
    /// code builds them. A typo here is a game that does not start.
    #[test]
    fn v0_3_each_launcher_starts_a_game_through_its_own_uri() {
        assert_eq!(GameLauncher::Steam.uri("440"), "steam://rungameid/440");
        assert_eq!(
            GameLauncher::Epic.uri("0a2d9f64"),
            "com.epicgames.launcher://apps/0a2d9f64?action=launch&silent=true"
        );
    }

    /// The two halves of the System Source rank differently, and the split is the
    /// whole point: a page you ask for by name, a task you never do.
    #[test]
    fn v0_3_a_curated_page_shares_the_app_tier_and_a_control_panel_task_does_not() {
        assert_eq!(EntryKind::System.tier(), EntryKind::App.tier());
        assert!(EntryKind::SystemTask.tier() > EntryKind::App.tier());
        // Still above documents: a task is something you go to, a file is not.
        assert!(EntryKind::SystemTask.tier() < EntryKind::File.tier());
    }

    /// The handicap that stopped `dis` being a coin flip between Discord and the
    /// Display page. Applied after Frecency, so use can still move a page up.
    #[test]
    fn v0_3_system_entries_are_handicapped_against_applications() {
        assert_eq!(EntryKind::App.weight(), 1.0);
        assert!(EntryKind::System.weight() < EntryKind::App.weight());
        assert_eq!(EntryKind::System.weight(), EntryKind::SystemTask.weight());
    }

    /// Generalising Steam into [`GameLauncher`] must not move a single learned id:
    /// `steam:440` is what v0.2 wrote and what Frecency is keyed on.
    #[test]
    fn v0_3_a_game_id_is_its_launcher_slug_and_the_launchers_own_id() {
        let steam = EntryId::for_launch(&LaunchTarget::Game {
            launcher: GameLauncher::Steam,
            id: "440".into(),
        });
        assert_eq!(steam.as_str(), "steam:440");

        let epic = EntryId::for_launch(&LaunchTarget::Game {
            launcher: GameLauncher::Epic,
            id: "0a2d9f6403244d12969e11da6713137b".into(),
        });
        assert_eq!(epic.as_str(), "epic:0a2d9f6403244d12969e11da6713137b");
        assert_ne!(steam, epic);
    }

    /// No path, so the id is the AUMID — stable across updates by design, unlike
    /// the display name beside it.
    #[test]
    fn v0_2_uwp_and_steam_ids_are_namespaced_away_from_paths() {
        let uwp = EntryId::for_launch(&LaunchTarget::Aumid(
            "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".into(),
        ));
        let steam = EntryId::for_launch(&LaunchTarget::Game {
            launcher: GameLauncher::Steam,
            id: "440".into(),
        });
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
