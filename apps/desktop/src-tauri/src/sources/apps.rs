//! The application Source: five discovery paths, one in-memory list, one matcher.
//!
//! **No cache on disk**, deliberately — ADR-0012 has the reasoning, the measured
//! walk time, and the trigger that would change the decision.
//!
//! [`AppSource::is_indexing`] rides in the query response so the Palette can say
//! "Indexing applications…" during the walk. An empty list means "you have no such
//! app", which in the first second after login is exactly wrong.

pub mod appsfolder;
pub mod games;
pub mod lnk;
pub mod noise;
pub mod path;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;
use std::time::{Duration, Instant};

use crate::actions;
use crate::entry::{
    Action, Entry, EntryId, EntryKind, IconRef, LaunchTarget, Query, Source, SourceId,
    SOURCE_SHORTLIST,
};
use crate::icons::{IconSource, IconStore};
use crate::rank::{self, Haystack};

pub const SOURCE_ID: SourceId = SourceId("apps");

/// Where an application was discovered, which decides how it is grouped.
///
/// Four paths, four groups. This machine has 1,891 applications and most are
/// `PATH` executables like `a2ping` — real, launchable, and never what someone
/// is scrolling the Settings list for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AppOrigin {
    /// A Start Menu or Desktop shortcut. What "installed" means to a person.
    Installed,
    /// `shell:AppsFolder` — a packaged app, Store or otherwise.
    Store,
    /// A game, through its launcher.
    Game,
    /// A bare executable on `PATH`. The long tail.
    CommandLine,
}

/// One discovered application, with its matching form precomputed.
///
/// The [`Haystack`] is built once at discovery rather than once per keystroke.
/// Lowercasing and tokenising three hundred–odd titles on every keypress would
/// spend most of the 20 ms Source budget recomputing an answer that never changes.
#[derive(Clone, Debug)]
pub struct App {
    pub id: EntryId,
    pub title: String,
    /// Which discovery path found it (v0.7). Grouping only — it has never
    /// affected ranking and must not start.
    pub origin: AppOrigin,
    /// Where it came from, shown under the title — a path for a Win32 app, the
    /// store or Steam for the others. This is what disambiguates two apps with the
    /// same name, which is common enough (`Code` and `Code - Insiders`) to matter.
    pub subtitle: Option<String>,
    pub target: LaunchTarget,
    pub hay: Haystack,
    /// The file whose icon represents this app. The `.lnk` where there is one,
    /// because a shortcut may carry an icon its target executable does not.
    pub icon_source: Option<std::path::PathBuf>,
    /// The icon key, resolved **once at discovery** rather than per keystroke.
    ///
    /// Computing it stats the file for its mtime (§6). Lazily that was twelve
    /// `fs::metadata` calls per keypress — I/O on the span the 30 ms first-Entry
    /// budget measures — plus a linear scan of the app list per drawn row.
    pub icon: Option<IconRef>,
    /// Shown beside the title, and **only where two same-named executables
    /// disagree** — two Node installs, two R installs. Resolved once at
    /// discovery for the handful of colliding names; reading it for everything
    /// costs 13 s against a 450 ms walk.
    pub version: Option<String>,
}

impl App {
    /// Whether there is a file behind this application.
    ///
    /// A packaged app has none, so it is offered Open alone — no reveal, no copy
    /// path, no run-as-administrator. Public since v0.10, when
    /// `query::suggestion` began building Entries outside this module.
    pub fn has_path(&self) -> bool {
        matches!(self.target, LaunchTarget::Exe { .. })
    }
}

/// The application Source.
///
/// `RwLock`, not `Mutex`: written once by the discovery thread and read on every
/// keystroke, and from v0.7 the rayon fan-out means several reads are genuinely
/// in flight at once.
pub struct AppSource {
    apps: RwLock<Vec<App>>,
    indexing: AtomicBool,
}

impl Default for AppSource {
    fn default() -> Self {
        Self::new()
    }
}

impl AppSource {
    pub fn new() -> Self {
        AppSource {
            apps: RwLock::new(Vec::new()),
            // True from construction, not from the start of the walk. Between the
            // two there is a window where the list is empty and nothing has begun
            // filling it, and reporting "ready, no apps" there is the exact lie
            // this flag exists to prevent.
            indexing: AtomicBool::new(true),
        }
    }

    /// Is the first discovery pass still running?
    pub fn is_indexing(&self) -> bool {
        self.indexing.load(Ordering::Acquire)
    }

    pub fn len(&self) -> usize {
        self.apps.read().map(|a| a.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Run all five discovery paths and replace the list.
    ///
    /// Swapped in one write, so a keystroke mid-walk sees the previous complete
    /// list or the next one, never a half-built one. Takes the [`IconStore`]
    /// because icon keys are resolved here, once — see [`App::icon`].
    pub fn refresh(&self, icons: &IconStore) {
        let started = Instant::now();
        let apps = discover_all(icons);
        let count = apps.len();

        if let Ok(mut guard) = self.apps.write() {
            *guard = apps;
        }
        self.indexing.store(false, Ordering::Release);

        // The number the no-cache decision rests on. Printed rather than logged
        // because there is no log yet, and printed unconditionally because a
        // decision described as provisional needs its evidence to be visible
        // without a rebuild.
        eprintln!(
            "[takyon] discovered {count} applications in {} ms",
            started.elapsed().as_millis()
        );
    }

    /// Attach user aliases to the applications that already exist.
    ///
    /// In place rather than through a re-walk, so v0.6's editor can make a new
    /// alias live immediately. Discovery happens once at login; an alias that
    /// needed the next one would look broken for the rest of the session.
    pub fn apply_aliases(&self, aliases: &crate::aliases::AliasStore) {
        let by_target = aliases.by_target();
        let Ok(mut apps) = self.apps.write() else {
            return;
        };
        for app in apps.iter_mut() {
            app.hay.aliases = by_target.get(&app.id).cloned().unwrap_or_default();
        }
    }

    /// Every App that has an icon, paired with its key. Used by the measurement
    /// tests that compare what shares an icon on the real machine.
    pub fn icon_keys(&self) -> Vec<(EntryId, String)> {
        let Ok(apps) = self.apps.read() else {
            return Vec::new();
        };
        apps.iter()
            .filter_map(|a| a.icon.as_ref().map(|i| (a.id.clone(), i.0.clone())))
            .collect()
    }

    /// Every application, title-sorted, for the Settings list.
    ///
    /// The whole list, not a page: read once on mount, off every latency budget,
    /// and paging an in-memory `Vec` is machinery for a scroll position.
    pub fn all(&self) -> Vec<App> {
        let Ok(apps) = self.apps.read() else {
            return Vec::new();
        };
        let mut all = apps.clone();
        all.sort_by_key(|a| a.title.to_lowercase());
        all
    }

    /// Look up one App by its id, for launching and for the action menu.
    pub fn find(&self, id: &EntryId) -> Option<App> {
        self.apps.read().ok()?.iter().find(|a| &a.id == id).cloned()
    }

    /// Populate the list without running discovery.
    ///
    /// The seam `query.rs`'s tests need. The alternative is asserting against
    /// whatever happens to be installed on the machine running them, which is how
    /// a suite becomes a ritual nobody trusts.
    #[doc(hidden)]
    pub fn set_for_test(&self, apps: Vec<App>) {
        if let Ok(mut guard) = self.apps.write() {
            *guard = apps;
        }
        self.indexing.store(false, Ordering::Release);
    }
}

impl Source for AppSource {
    fn id(&self) -> SourceId {
        SOURCE_ID
    }

    fn query(&self, q: &Query, budget: Duration) -> Vec<Entry> {
        if q.is_empty() {
            return Vec::new();
        }
        let deadline = Instant::now() + budget;
        let Ok(apps) = self.apps.read() else {
            return Vec::new();
        };

        let mut out = Vec::new();
        let mut binary_only: Vec<bool> = Vec::new();
        for (i, app) in apps.iter().enumerate() {
            // Checked in blocks rather than per app: `Instant::now()` is a real
            // syscall on some Windows configurations, and calling it three hundred
            // times to protect a loop that takes under a millisecond would cost
            // more than the loop.
            if i % 64 == 0 && Instant::now() > deadline {
                break;
            }
            let Some(score) = rank::score(q, &app.hay) else {
                continue;
            };
            binary_only.push(rank::matched_only_by_binary(q, &app.hay));
            out.push(Entry {
                id: app.id.clone(),
                title: app.title.clone(),
                subtitle: app.subtitle.clone(),
                kind: EntryKind::App,
                // Already resolved at discovery, so this is a clone rather than a
                // stat. See `App::icon`.
                icon: app.icon.clone(),
                score,
                actions: actions::for_app(app.has_path()),
                version: app.version.clone(),
            });
        }

        // A binary name is a way in, not an answer. Where anything matched by its
        // *name*, the rows that matched only through an executable's filename are
        // a different product wearing a shared filename — `chrome` reaching a
        // Chromium fork. Where nothing did, they are the only way in and stay.
        if binary_only.iter().any(|only| !only) {
            let mut keep = binary_only.iter();
            out.retain(|_| !keep.next().copied().unwrap_or(false));
        }

        // Trimmed here as well as in `query.rs`. Without this a two-letter query
        // hands several hundred Entries to the merge step, and every one of them
        // is cloned across the fan-out for nothing. `SOURCE_SHORTLIST`, not
        // `MAX_ENTRIES`: Frecency is applied after the fan-out and needs room.
        rank::order(out, SOURCE_SHORTLIST)
    }

    fn actions(&self, entry: &Entry) -> Vec<Action> {
        actions::for_entry(entry)
    }
}

/// Run the five discovery paths and merge them.
///
/// Order matters, because [`rank::dedupe`] keeps the better-scoring Entry and
/// these paths produce descending quality of metadata: a Start Menu shortcut knows
/// the real display name, a bare `PATH` executable knows only its basename.
fn discover_all(icons: &IconStore) -> Vec<App> {
    #[cfg(windows)]
    let _com = crate::com::ComScope::new();

    let mut apps: Vec<App> = Vec::new();
    let mut seen: std::collections::HashSet<EntryId> = std::collections::HashSet::new();

    // A function rather than a closure: step 2 has to read the titles collected by
    // step 1, and a closure capturing `apps` mutably holds that borrow open.
    fn push(
        apps: &mut Vec<App>,
        seen: &mut std::collections::HashSet<EntryId>,
        icons: &IconStore,
        mut app: App,
    ) {
        if seen.insert(app.id.clone()) {
            app.icon = icons.register(icon_source_for(&app));
            apps.push(app);
        }
    }

    // 1. Start Menu shortcuts — the best metadata, so first.
    for sc in lnk::discover() {
        let target = LaunchTarget::Exe {
            path: sc.target.clone(),
            args: sc.args.clone(),
            working_dir: sc.working_dir.clone(),
        };
        let stem = sc
            .target
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        push(&mut apps, &mut seen, icons, App {
            id: EntryId::for_launch(&target),
            origin: AppOrigin::Installed,
            hay: Haystack::new(&sc.name, stem.as_deref()),
            title: sc.name,
            subtitle: Some(sc.target.to_string_lossy().to_string()),
            target,
            icon_source: Some(sc.link),
            icon: None,
            version: None,
        });
    }

    // 2. Packaged apps.
    //
    // Skipped by *title* where the Start Menu already produced one. `AppsFolder`
    // hands back an AUMID for Win32 apps that register one for taskbar pinning —
    // Visual Studio Code appeared twice, with two EntryIds, which from v0.3 would
    // split its Frecency. `SIGDN_FILESYSPATH` cannot tell them apart (an
    // AppsFolder item is virtual, so it fails for both), and the id sets are
    // disjoint by construction, so the name is the only handle. The Start Menu
    // copy wins because it has a path, and so supports reveal, elevate and copy.
    let win32_titles: std::collections::HashSet<String> =
        apps.iter().map(|a| a.title.to_lowercase()).collect();
    for app in appsfolder::discover() {
        if win32_titles.contains(&app.name.to_lowercase()) {
            continue;
        }
        let target = LaunchTarget::Aumid(app.aumid.clone());
        push(&mut apps, &mut seen, icons, App {
            id: EntryId::for_launch(&target),
            hay: Haystack::new(&app.name, None),
            title: app.name,
            origin: AppOrigin::Store,
            // Detected, not assumed: 74 of 112 AUMIDs here are Win32, and calling
            // File Explorer a Store app is the v0.2 defect this closes.
            subtitle: appsfolder::subtitle(&app.aumid),
            target,
            icon_source: None,
            icon: None,
            version: None,
        });
    }

    // 3. Games, each through its own launcher. Also pathless as far as identity
    // goes: the id is the launcher's, so a game that moves drive keeps its
    // Frecency. Adding GOG or EA touches `games.rs` and nothing here.
    for library in games::all() {
        for game in library.games() {
            let target = LaunchTarget::Game {
                launcher: game.launcher,
                id: game.id,
            };
            push(&mut apps, &mut seen, icons, App {
                id: EntryId::for_launch(&target),
                hay: Haystack::new(&game.name, None),
                title: game.name,
                origin: AppOrigin::Game,
                subtitle: Some(game.launcher.label().to_string()),
                target,
                icon_source: None,
                icon: None,
                version: None,
            });
        }
    }

    // 4. Bare executables on PATH — least metadata, so last. Anything already
    // found through a shortcut is dropped by the `seen` set, which is why this
    // ordering is not cosmetic: reversed, `code` would be titled "code" rather
    // than "Visual Studio Code".
    //
    // Skipped by title where an application of that name is already known. The
    // `WindowsApps` aliases are the case that needs it: `notepad.exe` there is a
    // 0-byte reparse point into the same packaged Notepad that `AppsFolder`
    // already listed. Matching the *whole* name keeps every CLI tool — `winget`,
    // `wt`, `python` and `bash` name no application, so none of them collides.
    let known_titles: std::collections::HashSet<String> =
        apps.iter().map(|a| a.title.to_lowercase()).collect();
    for exe in path::discover() {
        if known_titles.contains(&exe.stem.to_lowercase()) {
            continue;
        }
        let target = LaunchTarget::Exe {
            path: exe.path.clone(),
            args: None,
            working_dir: None,
        };
        push(&mut apps, &mut seen, icons, App {
            id: EntryId::for_launch(&target),
            // `for_executable`, not `new(stem, Some(stem))`. A bare PATH entry has
            // no display name, and pretending its basename is one lets `code`
            // match `code.cmd` at the exact-name rung and outrank Visual Studio
            // Code. See the constructor for the whole story.
            hay: Haystack::for_executable(&exe.stem),
            title: exe.stem,
            origin: AppOrigin::CommandLine,
            subtitle: Some(exe.path.to_string_lossy().to_string()),
            target,
            icon_source: Some(exe.path),
            icon: None,
            version: None,
        });
    }

    // 5. Desktop shortcuts, last and least. Almost every one duplicates something
    // an earlier path already found under a better title, so Desktop loses every
    // collision and the EntryId stays on the Start Menu copy. Nine shortcuts on
    // the dev machine, eight duplicates, one genuinely new.
    let known_titles: std::collections::HashSet<String> =
        apps.iter().map(|a| a.title.to_lowercase()).collect();
    for sc in lnk::discover_desktop() {
        if known_titles.contains(&sc.name.to_lowercase()) {
            continue;
        }
        let target = LaunchTarget::Exe {
            path: sc.target.clone(),
            args: sc.args.clone(),
            working_dir: sc.working_dir.clone(),
        };
        let stem = sc
            .target
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        push(&mut apps, &mut seen, icons, App {
            id: EntryId::for_launch(&target),
            origin: AppOrigin::Installed,
            hay: Haystack::new(&sc.name, stem.as_deref()),
            title: sc.name,
            subtitle: Some(sc.target.to_string_lossy().to_string()),
            target,
            icon_source: Some(sc.link),
            icon: None,
            version: None,
        });
    }

    attach_versions(&mut apps, crate::version::of);
    apps
}

/// Stamp a version on same-named executables that disagree about theirs.
///
/// **Only the colliding names are read.** Measured on the dev machine: 16 files
/// in 3 ms, against 13.3 seconds to read all 1233 — which is thirty times the
/// whole walk. The reader is a parameter so the rule is testable without files.
fn attach_versions(apps: &mut [App], read: impl FnMut(&Path) -> Option<String>) {
    let mut read = read;

    let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, app) in apps.iter().enumerate() {
        let LaunchTarget::Exe { path, .. } = &app.target else {
            continue;
        };
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        by_name.entry(name.to_lowercase()).or_default().push(i);
    }

    for (_, group) in by_name.into_iter().filter(|(_, g)| g.len() > 1) {
        // One read per distinct path: the same binary reached by two shortcuts is
        // not a collision, and reading it twice would say it agrees with itself.
        let mut versions: HashMap<PathBuf, Option<String>> = HashMap::new();
        for &i in &group {
            if let LaunchTarget::Exe { path, .. } = &apps[i].target {
                versions
                    .entry(path.clone())
                    .or_insert_with(|| read(path.as_path()));
            }
        }
        let distinct: Vec<Option<String>> = versions.values().cloned().collect();
        if !crate::version::tells_apart(&distinct) {
            continue;
        }
        for &i in &group {
            if let LaunchTarget::Exe { path, .. } = &apps[i].target {
                apps[i].version = versions.get(path).cloned().flatten();
            }
        }
    }
}

/// Where an App's icon comes from.
///
/// The `.lnk` in preference to its target, because a shortcut may carry an icon
/// the target executable does not — installers routinely point a shortcut at a
/// generic launcher stub and give the shortcut the real branding.
pub fn icon_source_for(app: &App) -> Option<IconSource> {
    match &app.target {
        LaunchTarget::Aumid(aumid) => Some(IconSource::Aumid(aumid.clone())),
        LaunchTarget::Exe { path, .. } => Some(IconSource::File(
            app.icon_source.clone().unwrap_or_else(|| path.clone()),
        )),
        // A Steam game's icon lives in the client's cache under a name derived
        // from the app id, and reading it is a v0.3 nicety rather than something
        // v0.2 owes. The row renders its placeholder until then.
        LaunchTarget::Game { .. } => None,
        // System entries never reach this path — they are their own Source, not
        // Apps. Present for exhaustiveness; they render a placeholder.
        LaunchTarget::ShellItem(_) | LaunchTarget::Uri(_) => None,
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn exe_app(title: &str, path: &str, exe_stem: Option<&str>) -> App {
        let target = LaunchTarget::Exe {
            path: PathBuf::from(path),
            args: None,
            working_dir: None,
        };
        App {
            id: EntryId::for_launch(&target),
            origin: crate::sources::apps::AppOrigin::Installed,
            hay: Haystack::new(title, exe_stem),
            title: title.to_string(),
            subtitle: Some(path.to_string()),
            target,
            icon_source: None,
            icon: None,
            version: None,
        }
    }

    fn source_with(apps: Vec<App>) -> AppSource {
        let s = AppSource::new();
        *s.apps.write().unwrap() = apps;
        s.indexing.store(false, Ordering::Release);
        s
    }

    /// Two installs of one tool are two applications, and the version is the only
    /// thing on the machine that separates them (measured: `node.exe` 24.14.1
    /// against 26.7).
    #[test]
    fn v0_3_same_named_executables_get_a_version_where_they_differ() {
        let mut apps = vec![
            exe_app("node", r"C:\nvm4w\nodejs\node.exe", Some("node")),
            exe_app("Node.js", r"C:\program files\nodejs\node.exe", Some("node")),
        ];
        attach_versions(&mut apps, |p| {
            Some(if p.to_string_lossy().contains("nvm4w") {
                "24.14.1".into()
            } else {
                "26.7".into()
            })
        });
        assert_eq!(apps[0].version.as_deref(), Some("24.14.1"));
        assert_eq!(apps[1].version.as_deref(), Some("26.7"));
    }

    /// One Windows binary shipped for two architectures carries one version, so
    /// stamping it on both rows adds width and no information (ADR-0016).
    #[test]
    fn v0_3_identical_versions_are_not_shown_at_all() {
        let mut apps = vec![
            exe_app("Windows PowerShell", r"C:\windows\system32\powershell.exe", None),
            exe_app("Windows PowerShell (x86)", r"C:\windows\syswow64\powershell.exe", None),
        ];
        attach_versions(&mut apps, |_| Some("6.2.26100.8875".into()));
        assert!(apps.iter().all(|a| a.version.is_none()));
    }

    /// An executable whose name is unique is never read at all. That is the whole
    /// cost control: 16 files instead of 1233.
    #[test]
    fn v0_3_a_unique_filename_is_never_read_for_its_version() {
        let mut apps = vec![exe_app("Notepad", r"C:\windows\notepad.exe", None)];
        let mut reads = 0;
        attach_versions(&mut apps, |_| {
            reads += 1;
            Some("1.0".into())
        });
        assert_eq!(reads, 0, "a unique name was read anyway");
        assert!(apps[0].version.is_none());
    }

    /// A product name must return that product, not a fork that kept upstream's
    /// binary name. Measured live: `chrome` returned Helium at the exe rung.
    #[test]
    fn v0_3_a_name_match_hides_rows_that_only_matched_a_binary_name() {
        let source = source_with(vec![
            exe_app("Google Chrome", r"C:\chrome\chrome.exe", Some("chrome")),
            exe_app("Helium", r"C:\imput\helium\application\chrome.exe", Some("chrome")),
        ]);
        let titles: Vec<String> = source
            .query(&Query::new("chrome"), Duration::from_millis(20))
            .into_iter()
            .map(|e| e.title)
            .collect();
        assert_eq!(titles, vec!["Google Chrome"], "Helium is a different product");

        // And it is still reachable by its own name.
        let by_name = source.query(&Query::new("helium"), Duration::from_millis(20));
        assert_eq!(by_name[0].title, "Helium");
    }

    /// With upstream absent, the binary name is the only way in and must work.
    #[test]
    fn v0_3_a_binary_name_still_finds_a_fork_when_nothing_matches_by_name() {
        let source = source_with(vec![exe_app(
            "Helium",
            r"C:\imput\helium\application\chrome.exe",
            Some("chrome"),
        )]);
        let found = source.query(&Query::new("chrome"), Duration::from_millis(20));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "Helium");
    }

    /// The three matching cases from the phase's manual verification script, run
    /// against the Source rather than the ranker, so a wiring mistake between the
    /// two is caught here rather than by hand.
    #[test]
    fn v0_2_the_manual_verification_queries_find_their_apps() {
        let source = source_with(vec![
            exe_app("Adobe Photoshop", r"C:\ps\Photoshop.exe", Some("Photoshop")),
            exe_app("Visual Studio Code", r"C:\vsc\Code.exe", Some("Code")),
            exe_app("Notepad", r"C:\Windows\notepad.exe", Some("notepad")),
        ]);

        let top = |needle: &str| {
            source
                .query(&Query::new(needle), Duration::from_millis(20))
                .first()
                .map(|e| e.title.clone())
        };

        assert_eq!(top("phot").as_deref(), Some("Adobe Photoshop"));
        assert_eq!(top("vsc").as_deref(), Some("Visual Studio Code"));
        assert_eq!(top("code").as_deref(), Some("Visual Studio Code"));
    }

    /// Verification step 2 of the phase: `ps` reaches Photoshop.
    ///
    /// The rung matters as much as the hit — an alias must beat a name starting
    /// with the same letters, or `ps` still finds "PS Remote Play" first and the
    /// feature looks broken rather than outranked.
    #[test]
    fn v0_3_an_alias_outranks_a_name_that_starts_with_the_same_letters() {
        let photoshop = exe_app("Adobe Photoshop", r"C:\ps\Photoshop.exe", Some("Photoshop"));
        let source = source_with(vec![
            photoshop.clone(),
            exe_app("PS Remote Play", r"C:\sony\RemotePlay.exe", Some("RemotePlay")),
        ]);

        let store = crate::aliases::AliasStore::open(None).unwrap();
        store.set("ps", &photoshop.id).unwrap();
        source.apply_aliases(&store);

        let entries = source.query(&Query::new("ps"), Duration::from_millis(20));
        assert_eq!(entries[0].title, "Adobe Photoshop");
    }

    /// Removing an alias takes effect without a re-walk, the same way adding one
    /// does. Otherwise a deleted alias keeps answering until the next login.
    #[test]
    fn v0_3_clearing_an_alias_takes_effect_in_place() {
        let photoshop = exe_app("Adobe Photoshop", r"C:\ps\Photoshop.exe", Some("Photoshop"));
        let source = source_with(vec![photoshop.clone()]);
        let store = crate::aliases::AliasStore::open(None).unwrap();

        store.set("zz", &photoshop.id).unwrap();
        source.apply_aliases(&store);
        assert_eq!(source.query(&Query::new("zz"), Duration::from_millis(20)).len(), 1);

        store.remove("zz").unwrap();
        source.apply_aliases(&store);
        assert!(source.query(&Query::new("zz"), Duration::from_millis(20)).is_empty());
    }

    #[test]
    fn v0_2_an_empty_query_returns_no_entries() {
        let source = source_with(vec![exe_app("Notepad", r"C:\n.exe", Some("n"))]);
        assert!(source.query(&Query::new(""), Duration::from_millis(20)).is_empty());
    }

    /// A UWP Entry must not offer actions that need a file. Checked here because
    /// this is where the two halves meet — the Source decides `has_path`, and
    /// `actions.rs` decides what that permits.
    #[test]
    fn v0_2_a_packaged_app_entry_offers_only_open() {
        let target = LaunchTarget::Aumid("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".into());
        let source = source_with(vec![App {
            id: EntryId::for_launch(&target),
            origin: crate::sources::apps::AppOrigin::Installed,
            hay: Haystack::new("Calculator", None),
            title: "Calculator".into(),
            subtitle: Some("Store app".into()),
            target,
            icon_source: None,
            icon: None,
            version: None,
        }]);
        let entries = source.query(&Query::new("calc"), Duration::from_millis(20));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].actions, vec![actions::OPEN]);
    }

    /// Until the first walk finishes, the Palette must be able to say so. An empty
    /// list and "no such app" are the same picture, and one of them is a lie.
    #[test]
    fn v0_2_a_fresh_source_reports_that_it_is_still_indexing() {
        let fresh = AppSource::new();
        assert!(fresh.is_indexing());
        assert!(fresh.is_empty());

        let ready = source_with(vec![]);
        assert!(!ready.is_indexing());
    }

    /// §3: a Source that misses its deadline contributes nothing for that
    /// keystroke. A zero budget is the degenerate case of that rule.
    #[test]
    fn v0_2_a_source_past_its_deadline_returns_nothing() {
        let apps: Vec<App> = (0..500)
            .map(|i| exe_app(&format!("App {i}"), &format!(r"C:\a{i}.exe"), None))
            .collect();
        let source = source_with(apps);
        let entries = source.query(&Query::new("app"), Duration::ZERO);
        assert!(entries.len() < 500);
    }

    /// Amended at v0.3: a Source stops at the **shortlist**, not at what the
    /// Palette shows. Frecency is applied after the fan-out, so cutting to twelve
    /// here would discard a much-used Entry one step before its lift. `query.rs`
    /// enforces `MAX_ENTRIES` on the way out.
    #[test]
    fn v0_3_the_source_stops_at_the_shortlist_not_the_entry_limit() {
        let apps: Vec<App> = (0..200)
            .map(|i| exe_app(&format!("Photo {i}"), &format!(r"C:\p{i}.exe"), None))
            .collect();
        let source = source_with(apps);
        let entries = source.query(&Query::new("photo"), Duration::from_millis(50));
        assert_eq!(entries.len(), SOURCE_SHORTLIST);
    }

    /// Run the real discovery walk on this machine and report what it found.
        ///
        /// `#[ignore]`d: depends on what is installed, so it can never assert. It is
        /// the measurement ADR-0012 rests on, kept beside the code so re-checking is
        /// one command. Debug build, so treat the number as an upper bound.
    #[test]
    #[ignore = "measures the host machine; run explicitly with --ignored"]
    fn v0_2_measure_the_real_walk() {
        let source = AppSource::new();
        let icons = IconStore::new(None);
        let started = std::time::Instant::now();
        source.refresh(&icons);
        let elapsed = started.elapsed();

        let apps = source.apps.read().unwrap();
        let count = |f: fn(&App) -> bool| apps.iter().filter(|a| f(a)).count();
        println!("  total          {:>6} ms", elapsed.as_millis());
        println!("  applications   {:>6}", apps.len());
        println!(
            "  executables    {:>6}",
            count(|a| matches!(a.target, LaunchTarget::Exe { .. }))
        );
        println!(
            "  packaged       {:>6}",
            count(|a| matches!(a.target, LaunchTarget::Aumid(_)))
        );
        println!(
            "  games          {:>6}",
            count(|a| matches!(a.target, LaunchTarget::Game { .. }))
        );

        // v0.3 task 0: applications that exist only because arguments joined the
        // id. Fifteen were being dropped by `seen.insert` before it (tbd §9).
        let with_args: Vec<&App> = apps.iter().filter(|a| a.id.as_str().contains('|')).collect();
        println!("  argument ids   {:>6}", with_args.len());
        for host in ["cmd.exe", "javacpl.exe", "powershell.exe"] {
            let sharing: Vec<&str> = with_args
                .iter()
                .filter(|a| a.id.as_str().contains(host))
                .map(|a| a.title.as_str())
                .collect();
            println!("    {host:<16} {} {sharing:?}", sharing.len());
        }

        // ADR-0016: how often the second line has anything to disambiguate.
        let mut by_title: std::collections::HashMap<String, Vec<&str>> = Default::default();
        for a in apps.iter() {
            by_title
                .entry(a.title.to_lowercase())
                .or_default()
                .push(a.subtitle.as_deref().unwrap_or("-"));
        }
        let mut collisions: Vec<(&String, &Vec<&str>)> =
            by_title.iter().filter(|(_, v)| v.len() > 1).collect();
        collisions.sort();
        println!("  colliding titles {:>4}", collisions.len());
        for (title, subs) in collisions.iter().take(12) {
            println!("    {title:<28} {subs:?}");
        }

        assert!(!source.is_indexing(), "a completed walk is not still indexing");
    }

    #[test]
    fn v0_2_find_returns_the_app_behind_an_entry_id() {
        let source = source_with(vec![exe_app("Notepad", r"C:\Windows\notepad.exe", None)]);
        let entries = source.query(&Query::new("note"), Duration::from_millis(20));
        let found = source.find(&entries[0].id).expect("the id came from this Source");
        assert_eq!(found.title, "Notepad");
        assert!(source.find(&EntryId("nothing".into())).is_none());
    }
}
