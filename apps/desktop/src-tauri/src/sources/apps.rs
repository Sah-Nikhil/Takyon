//! The application Source: four discovery paths, one in-memory list, one matcher.
//!
//! **No cache on disk**, deliberately — ADR-0012 has the reasoning, the measured
//! walk time, and the trigger that would change the decision.
//!
//! [`AppSource::is_indexing`] rides in the query response so the Palette can say
//! "Indexing applications…" during the walk. An empty list means "you have no such
//! app", which in the first second after login is exactly wrong.

pub mod appsfolder;
pub mod lnk;
pub mod path;
pub mod steam;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;
use std::time::{Duration, Instant};

use crate::actions;
use crate::entry::{
    Action, Entry, EntryId, EntryKind, IconRef, LaunchTarget, Query, Source, SourceId, MAX_ENTRIES,
};
use crate::icons::{IconSource, IconStore};
use crate::rank::{self, Haystack};

pub const SOURCE_ID: SourceId = SourceId("apps");

/// One discovered application, with its matching form precomputed.
///
/// The [`Haystack`] is built once at discovery rather than once per keystroke.
/// Lowercasing and tokenising three hundred–odd titles on every keypress would
/// spend most of the 20 ms Source budget recomputing an answer that never changes.
#[derive(Clone, Debug)]
pub struct App {
    pub id: EntryId,
    pub title: String,
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
}

impl App {
    fn has_path(&self) -> bool {
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

    /// Run all four discovery paths and replace the list.
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
            });
        }

        // Trimmed here as well as in `query.rs`. Without this a two-letter query
        // hands several hundred Entries to the merge step, and every one of them
        // is cloned across the fan-out for nothing.
        rank::order(out, MAX_ENTRIES)
    }

    fn actions(&self, entry: &Entry) -> Vec<Action> {
        actions::for_entry(entry)
    }
}

/// Run the four discovery paths and merge them.
///
/// Order matters, because [`rank::dedupe`] keeps the better-scoring Entry and
/// these paths produce descending quality of metadata: a Start Menu shortcut knows
/// the real display name, a bare `PATH` executable knows only its basename.
fn discover_all(icons: &IconStore) -> Vec<App> {
    #[cfg(windows)]
    let _com = ComScope::new();

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
            hay: Haystack::new(&sc.name, stem.as_deref()),
            title: sc.name,
            subtitle: Some(sc.target.to_string_lossy().to_string()),
            target,
            icon_source: Some(sc.link),
            icon: None,
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
            subtitle: Some("Store app".to_string()),
            target,
            icon_source: None,
            icon: None,
        });
    }

    // 3. Steam. Also pathless as far as identity goes.
    if let Some(steam) = steam::steam_path() {
        for game in steam::discover(&steam) {
            let target = LaunchTarget::SteamGame(game.app_id);
            push(&mut apps, &mut seen, icons, App {
                id: EntryId::for_launch(&target),
                hay: Haystack::new(&game.name, None),
                title: game.name,
                subtitle: Some("Steam".to_string()),
                target,
                icon_source: None,
                icon: None,
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
            subtitle: Some(exe.path.to_string_lossy().to_string()),
            target,
            icon_source: Some(exe.path),
            icon: None,
        });
    }

    apps
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
        LaunchTarget::SteamGame(_) => None,
    }
}

/// COM initialised for the lifetime of one discovery pass.
///
/// Once per walk, not per shortcut. **Apartment-threaded**, because `AppsFolder`
/// is a shell namespace extension and several are known to deadlock when
/// enumerated from an MTA.
#[cfg(windows)]
struct ComScope {
    initialised: bool,
}

#[cfg(windows)]
impl ComScope {
    fn new() -> Self {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        // `RPC_E_CHANGED_MODE` means someone already initialised this thread into
        // the other apartment. That is a working COM thread, so carry on — but do
        // not uninitialise it on the way out, because we did not initialise it.
        ComScope {
            initialised: hr.is_ok(),
        }
    }
}

#[cfg(windows)]
impl Drop for ComScope {
    fn drop(&mut self) {
        if self.initialised {
            unsafe { windows::Win32::System::Com::CoUninitialize() };
        }
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
            hay: Haystack::new(title, exe_stem),
            title: title.to_string(),
            subtitle: Some(path.to_string()),
            target,
            icon_source: None,
            icon: None,
        }
    }

    fn source_with(apps: Vec<App>) -> AppSource {
        let s = AppSource::new();
        *s.apps.write().unwrap() = apps;
        s.indexing.store(false, Ordering::Release);
        s
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
            hay: Haystack::new("Calculator", None),
            title: "Calculator".into(),
            subtitle: Some("Store app".into()),
            target,
            icon_source: None,
            icon: None,
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

    #[test]
    fn v0_2_the_source_never_returns_more_than_the_entry_limit() {
        let apps: Vec<App> = (0..200)
            .map(|i| exe_app(&format!("Photo {i}"), &format!(r"C:\p{i}.exe"), None))
            .collect();
        let source = source_with(apps);
        let entries = source.query(&Query::new("photo"), Duration::from_millis(50));
        assert_eq!(entries.len(), MAX_ENTRIES);
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
            "  steam          {:>6}",
            count(|a| matches!(a.target, LaunchTarget::SteamGame(_)))
        );
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
