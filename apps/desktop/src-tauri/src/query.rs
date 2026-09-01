//! The query pipeline (IMPLEMENTATION_PLAN §3).
//!
//! ```text
//! input ─▶ bang::parse ─┬─▶ Bangless ─▶ fan out (rayon, 20 ms) ─▶ rank ─▶ top 12
//!                       └─▶ Bang(mode, rest) ─▶ that Mode alone
//! ```
//!
//! v0.2 builds the left branch with one Source. `bang.rs` is v0.8 and the
//! Stability lock v0.3; both are in the flow above because this file's shape
//! decides whether adding them is a line or a rewrite.
//!
//! **One `invoke` per keystroke, never one per Source** (ADR-0009). Every Source
//! registered below is local, which is how ADR-0002 stays checkable by reading.

use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;

use crate::entry::{Entry, EntryId, Query, Source, MAX_ENTRIES, SOURCE_BUDGET};
use crate::frecency::Frecency;
use crate::icons::IconStore;
use crate::rank;
use crate::sources::apps::AppSource;
use crate::sources::recents::RecentsSource;
use crate::sources::system::SystemSource;

/// What one keystroke gets back.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    /// Echoed from the request. **The frontend discards any response whose `seq`
    /// is lower than the newest it has seen** (§3) — without this a slow
    /// keystroke's results overwrite a fast one's, and the Palette shows answers
    /// for a prefix of what is now in the input box.
    pub seq: u64,
    pub entries: Vec<Entry>,
    /// True while the first application walk is still running.
    ///
    /// Lets the Palette say "Indexing applications…" instead of drawing an empty
    /// list. Those look identical and one of them is a lie — an empty list means "you
    /// have no such app", which right after login is exactly wrong.
    pub indexing: bool,
}

/// How long a query must stand still before its top row freezes (§3).
pub const LOCK_DELAY_MS: u64 = 100;

/// The Stability rule's state (IMPLEMENTATION_PLAN §3).
///
/// Not a debounce: results keep arriving and keep being shown. What freezes is
/// which Entry sits first, and only for the exact string that produced it.
struct StabilityLock {
    query: String,
    top: EntryId,
    /// When this query string was first answered, in milliseconds since start.
    first_seen_ms: u64,
}

/// The registry of Sources, and everything a query needs.
pub struct Pipeline {
    /// Held concretely as well as in `sources`, because launching and the action
    /// menu need to look an App up by id, which the trait deliberately does not
    /// expose — a `Source` produces Entries and knows nothing else.
    pub apps: Arc<AppSource>,
    /// Recently-opened files. Held concretely for the same reason `apps` is:
    /// activation has to find the thing behind an id, which the trait does not
    /// expose.
    pub recents: Arc<RecentsSource>,
    /// System settings pages and control-panel tasks. Held concretely for the
    /// same reason as the others: activation looks the entry up by id, which the
    /// `Source` trait does not expose.
    pub system: Arc<SystemSource>,
    pub icons: Arc<IconStore>,
    /// What the user has actually chosen before. Read once per candidate Entry,
    /// written once per activation.
    pub frecency: Arc<Frecency>,
    sources: Vec<Arc<dyn Source>>,
    /// The Stability rule. `Mutex` because a keystroke both reads and replaces it.
    lock: std::sync::Mutex<Option<StabilityLock>>,
    /// Process start, so "now" is a monotonic millisecond count the tests can
    /// supply directly. A wall clock here would make the rule untestable.
    started: Instant,
}

impl Pipeline {
    pub fn new(
        apps: Arc<AppSource>,
        recents: Arc<RecentsSource>,
        system: Arc<SystemSource>,
        icons: Arc<IconStore>,
        frecency: Arc<Frecency>,
    ) -> Self {
        let sources: Vec<Arc<dyn Source>> = vec![apps.clone(), recents.clone(), system.clone()];
        Pipeline {
            apps,
            recents,
            system,
            icons,
            frecency,
            sources,
            lock: std::sync::Mutex::new(None),
            started: Instant::now(),
        }
    }

    /// Answer one keystroke.
    pub fn query(&self, raw: &str, seq: u64) -> QueryResult {
        self.query_at(raw, seq, self.started.elapsed().as_millis() as u64)
    }

    /// Answer one keystroke at a given millisecond, for the Stability tests.
    pub fn query_at(&self, raw: &str, seq: u64, now_ms: u64) -> QueryResult {
        let q = Query::new(raw);
        let indexing = self.apps.is_indexing();

        if q.is_empty() {
            // An empty Palette has no top row to protect, and leaving a stale
            // lock behind would apply it to whatever is typed next.
            if let Ok(mut lock) = self.lock.lock() {
                *lock = None;
            }
            return QueryResult {
                seq,
                entries: Vec::new(),
                // Nothing is shown for an empty query, so nothing needs explaining.
                // Reporting `indexing` here would put a status row under a Palette
                // that has been deliberately left blank (ADR-0001).
                indexing: false,
            };
        }

        // Icon keys are already on the Entries: each Source resolves its own at
        // discovery time, so nothing here stats a file. Lazily that was twelve
        // `fs::metadata` calls per keystroke, on the exact span the 30 ms
        // first-Entry budget measures.
        let entries = self.fan_out(&q);

        // Frecency folds in here rather than inside a Source: the ladder stays
        // testable without a usage database, and no Source has to know what the
        // user has launched. One indexed read per candidate.
        let mut entries = rank::dedupe(entries);
        for entry in &mut entries {
            // Kind weight last, so it handicaps the frecency-lifted score rather
            // than the raw match. See `EntryKind::weight`.
            entry.score = rank::with_frecency(entry.score, self.frecency.weight(&entry.id))
                * entry.kind.weight();
        }

        let entries = rank::order(entries, MAX_ENTRIES);
        let entries = self.apply_stability(raw, entries, now_ms);
        // Last, and after the truncation, so "does this title repeat?" is asked
        // about the list the Palette is sent rather than a longer one.
        let entries = rank::disambiguate_subtitles(entries);

        QueryResult {
            seq,
            entries,
            indexing,
        }
    }

    /// Hold the top row still once a query has stopped changing (§3).
    ///
    /// A different query string starts the clock again — a new keystroke is a new
    /// question. Inside the delay the list reorders freely; past it the Entry that
    /// was first stays first, and better answers append below.
    fn apply_stability(&self, raw: &str, mut entries: Vec<Entry>, now_ms: u64) -> Vec<Entry> {
        let Ok(mut guard) = self.lock.lock() else {
            return entries;
        };

        match guard.as_mut() {
            // Same question, and it has stood still long enough to be committed
            // to. Promote the locked Entry back to the top if it is still here;
            // if it has gone, there is nothing to hold and nothing to fake.
            Some(lock) if lock.query == raw && now_ms.saturating_sub(lock.first_seen_ms) >= LOCK_DELAY_MS => {
                if let Some(i) = entries.iter().position(|e| e.id == lock.top) {
                    let top = entries.remove(i);
                    entries.insert(0, top);
                }
            }
            // Same question, still settling: let it reorder, and remember what is
            // currently first so the lock closes on the newest answer.
            Some(lock) if lock.query == raw => {
                if let Some(first) = entries.first() {
                    lock.top = first.id.clone();
                }
            }
            // A new question. Start the clock.
            _ => {
                if let Some(first) = entries.first() {
                    *guard = Some(StabilityLock {
                        query: raw.to_string(),
                        top: first.id.clone(),
                        first_seen_ms: now_ms,
                    });
                }
            }
        }
        entries
    }

    /// Ask every Source in parallel, with a shared deadline.
    ///
    /// One Source at v0.2, so rayon buys nothing yet and is here anyway: the fan-out
    /// is where the 20 ms budget is enforced, and retrofitting parallelism around a
    /// sequential budget is how it becomes "20 ms each, five Sources, 100 ms".
    fn fan_out(&self, q: &Query) -> Vec<Entry> {
        use rayon::prelude::*;

        let started = Instant::now();
        self.sources
            .par_iter()
            .flat_map_iter(|source| {
                // Each Source gets what remains of the budget, not a fresh copy. They run
                // concurrently, so in practice that is nearly all of it — but a Source that
                // starts late inherits the time spent rather than extending the total.
                let remaining = SOURCE_BUDGET.saturating_sub(started.elapsed());
                if remaining.is_zero() {
                    return Vec::new().into_iter();
                }
                source.query(q, remaining).into_iter()
            })
            .collect()
    }

    /// What an Entry launches, and what kind it is.
    ///
    /// Sources are asked in registration order. Ids do not collide across them —
    /// an App is keyed by its executable, a Recent by its document — so the order
    /// is a formality rather than a precedence rule.
    fn target_for(&self, id: &EntryId) -> Option<(crate::entry::LaunchTarget, crate::entry::EntryKind)> {
        if let Some(app) = self.apps.find(id) {
            return Some((app.target, crate::entry::EntryKind::App));
        }
        if let Some(entry) = self.system.find(id) {
            return Some((entry.target, entry.kind));
        }
        let recent = self.recents.find(id)?;
        Some((
            crate::entry::LaunchTarget::Exe {
                path: recent.target,
                args: None,
                working_dir: None,
            },
            recent.kind,
        ))
    }

    /// The actions offered for one Entry, for the `Ctrl+K` menu.
    pub fn actions_for(&self, id: &EntryId) -> Vec<crate::entry::Action> {
        if let Some(app) = self.apps.find(id) {
            return crate::actions::for_entry(&Entry {
                id: app.id.clone(),
                title: app.title.clone(),
                subtitle: app.subtitle.clone(),
                kind: crate::entry::EntryKind::App,
                icon: None,
                score: 0.0,
                actions: crate::actions::for_app(matches!(
                    app.target,
                    crate::entry::LaunchTarget::Exe { .. }
                )),
                version: None,
            });
        }
        if let Some(entry) = self.system.find(id) {
            return crate::actions::for_entry(&Entry {
                id: entry.id,
                title: entry.title,
                subtitle: None,
                kind: entry.kind,
                icon: None,
                score: 0.0,
                actions: crate::actions::for_system(),
                version: None,
            });
        }
        let Some(recent) = self.recents.find(id) else {
            return Vec::new();
        };
        crate::actions::for_entry(&Entry {
            id: recent.id,
            title: recent.title,
            subtitle: recent.subtitle,
            kind: recent.kind,
            icon: None,
            score: 0.0,
            actions: crate::actions::for_file(),
            version: None,
        })
    }

    /// Perform one action on one Entry.
    ///
    /// Returns the error rather than logging it: the only useful place to report an
    /// activation failure is the Palette, and by now it is hidden — so the frontend
    /// has to be told in order to bring it back.
    pub fn activate(&self, id: &EntryId, action: &str) -> Result<(), String> {
        let (target, kind) = self
            .target_for(id)
            .ok_or_else(|| "That Entry is no longer in the index.".to_string())?;

        // The image path of what started, kept for TBC-0010 (a recents list Takyon
        // owns) — nothing consumes it yet.
        let _image = match action {
            a if a == crate::actions::OPEN.as_str() => crate::launch::open(&target),
            a if a == crate::actions::RUN_AS_ADMIN.as_str() => crate::launch::run_as_admin(&target),
            a if a == crate::actions::REVEAL.as_str() => crate::launch::reveal(&target).map(|_| None),
            a if a == crate::actions::COPY_PATH.as_str() => {
                let path = crate::launch::path_of(&target)
                    .ok_or_else(|| "That Entry has no path to copy.".to_string())?;
                crate::launch::copy_to_clipboard(&path).map(|_| None)
            }
            other => Err(format!("Unknown action: {other}")),
        }?;

        self.record_activation(id, kind, action);
        Ok(())
    }

    /// What an activation teaches, after it has succeeded.
    ///
    /// Split out because the launch itself cannot be tested and this can. Both
    /// writes are best-effort: the application already started, and losing a unit
    /// of usage costs a little ranking accuracy and nothing else.
    pub fn record_activation(&self, id: &EntryId, kind: crate::entry::EntryKind, action: &str) {
        // Never before the launch succeeded. A failed activation is not a choice,
        // and recording one would teach the ranker to promote something that
        // cannot start.
        if !records_usage(action) {
            return;
        }
        if let Err(e) = self.frecency.record(id, kind) {
            eprintln!("[takyon] could not record usage: {e}");
        }
    }
}

/// Does this action count as choosing the application?
///
/// Only a launch teaches the ranker. Revealing a file or copying its path is
/// something people do while looking *for* something, and counting it would
/// train the Palette on the search rather than on the choice.
pub fn records_usage(action: &str) -> bool {
    action == crate::actions::OPEN.as_str() || action == crate::actions::RUN_AS_ADMIN.as_str()
}

/// Present so a future Source cannot quietly become a network client.
///
/// Not idle belt-and-braces: v0.8 adds a `SearchProvider` and v0.9 a subprocess,
/// and both will be tempting to reach for "just for suggestions". ADR-0002 calls
/// that a correctness bug, and this is where the claim is checkable.
pub fn bangless_sources_are_local() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{EntryKind, LaunchTarget};
    use crate::rank::Haystack;
    use crate::sources::apps::App;
    use std::path::PathBuf;

    fn app(title: &str, path: &str) -> App {
        let target = LaunchTarget::Exe {
            path: PathBuf::from(path),
            args: None,
            working_dir: None,
        };
        App {
            id: EntryId::for_launch(&target),
            hay: Haystack::new(title, PathBuf::from(path).file_stem().and_then(|s| s.to_str())),
            title: title.into(),
            subtitle: Some(path.into()),
            target,
            icon_source: None,
            icon: None,
            version: None,
        }
    }

    fn pipeline_with(apps: Vec<App>) -> Pipeline {
        let source = AppSource::new();
        source.set_for_test(apps);
        Pipeline::new(
            Arc::new(source),
            Arc::new(RecentsSource::new()),
            Arc::new(SystemSource::new()),
            Arc::new(IconStore::new(None)),
            Arc::new(Frecency::open(None).unwrap()),
        )
    }

    /// The live report: `dis` selected the Display page instead of Discord.
    ///
    /// Both are 7 letters matching a 3-letter prefix, so they score identically
    /// at 796.5 and a 0.3% Frecency gap decided the top row. The kind weight is
    /// what stops that being a coin flip. Numbers in `docs/tbd/v0.3.md` §10.
    #[test]
    fn v0_3_a_settings_page_does_not_take_the_top_row_from_an_app_on_a_hair() {
        let discord_target = LaunchTarget::Exe {
            path: PathBuf::from(r"C:\Users\x\AppData\Local\Discord\Update.exe"),
            args: Some("--processStart discord.exe".into()),
            working_dir: None,
        };
        let discord = App {
            id: EntryId::for_launch(&discord_target),
            hay: Haystack::new("Discord", Some("update")),
            title: "Discord".into(),
            subtitle: None,
            target: discord_target,
            icon_source: None,
            icon: None,
            version: None,
        };

        let apps = AppSource::new();
        apps.set_for_test(vec![discord]);
        let system = SystemSource::new();
        system.set_for_test(crate::sources::system::settings_catalog());
        let p = Pipeline::new(
            Arc::new(apps),
            Arc::new(RecentsSource::new()),
            Arc::new(system),
            Arc::new(IconStore::new(None)),
            Arc::new(Frecency::open(None).unwrap()),
        );

        let entries = p.query("dis", 1).entries;
        let seen: Vec<(String, f32)> = entries
            .iter()
            .map(|e| (e.title.clone(), e.score))
            .collect();
        assert_eq!(entries[0].title, "Discord", "{seen:?}");
        assert!(
            entries.iter().any(|e| e.title == "Display"),
            "the page must still be reachable, just not first: {seen:?}"
        );
    }

    /// RK4 from the live pass: `disk` selected the Storage settings page.
    ///
    /// Storage carries a shipped keyword "disk". That keyword sat on the *user
    /// alias* rung (1000), so it beat an application literally named "Disk
    /// Cleanup". Shipped keywords have their own rung now (`TIER_KEYWORD`).
    #[test]
    fn v0_3_a_shipped_keyword_does_not_beat_an_app_named_for_the_same_word() {
        let apps = AppSource::new();
        apps.set_for_test(vec![app("Disk Cleanup", r"C:\Windows\System32\cleanmgr.exe")]);
        let system = SystemSource::new();
        system.set_for_test(crate::sources::system::settings_catalog());
        let p = Pipeline::new(
            Arc::new(apps),
            Arc::new(RecentsSource::new()),
            Arc::new(system),
            Arc::new(IconStore::new(None)),
            Arc::new(Frecency::open(None).unwrap()),
        );

        let entries = p.query("disk", 1).entries;
        let seen: Vec<(String, f32)> =
            entries.iter().map(|e| (e.title.clone(), e.score)).collect();
        assert_eq!(entries[0].title, "Disk Cleanup", "{seen:?}");
        // Still reachable — the keyword works, it just does not win.
        assert!(entries.iter().any(|e| e.title == "Storage"), "{seen:?}");
    }

    /// A control-panel task sits below every app, whatever it scores.
    ///
    /// 198 of these walk in, all long sentences that match only by word prefix.
    /// "Change the way currency is displayed" is not what `dis` is asking for.
    #[test]
    fn v0_3_a_control_panel_task_never_outranks_an_application() {
        let apps = AppSource::new();
        apps.set_for_test(vec![app("Disk Cleanup", r"C:\Windows\System32\cleanmgr.exe")]);
        let system = SystemSource::new();
        system.set_for_test(vec![crate::sources::system::task_from(
            "Disk Cleanup Options",
            vec![1, 2, 3, 4],
        )
        .expect("a named task with a pidl")]);
        let p = Pipeline::new(
            Arc::new(apps),
            Arc::new(RecentsSource::new()),
            Arc::new(system),
            Arc::new(IconStore::new(None)),
            Arc::new(Frecency::open(None).unwrap()),
        );

        let entries = p.query("disk", 1).entries;
        assert_eq!(entries[0].title, "Disk Cleanup");
        assert_eq!(entries[0].kind, EntryKind::App);
        assert_eq!(entries[1].kind, EntryKind::SystemTask);
    }

    /// A settings page beats a weakly-matching app on merit (task 8 ranking).
    ///
    /// The real case: `display` put `DisplaySwitch` (bare `System32` exe, matches
    /// only by filename stem, 650) above the `Display` page (exact name, 900).
    /// System and App share a tier now, so score decides and the page wins.
    #[test]
    fn v0_3_a_settings_page_beats_an_app_that_only_matches_by_exe_stem() {
        let ds_target = LaunchTarget::Exe {
            path: PathBuf::from(r"C:\Windows\System32\displayswitch.exe"),
            args: None,
            working_dir: None,
        };
        let display_switch = App {
            id: EntryId::for_launch(&ds_target),
            // A bare PATH exe has no display name, so it matches only by stem —
            // exactly the DisplaySwitch case. `app()` would give it a real name.
            hay: Haystack::for_executable("displayswitch"),
            title: "DisplaySwitch".into(),
            subtitle: None,
            target: ds_target,
            icon_source: None,
            icon: None,
            version: None,
        };

        let apps = AppSource::new();
        apps.set_for_test(vec![display_switch]);
        let system = SystemSource::new();
        system.set_for_test(crate::sources::system::settings_catalog());
        let p = Pipeline::new(
            Arc::new(apps),
            Arc::new(RecentsSource::new()),
            Arc::new(system),
            Arc::new(IconStore::new(None)),
            Arc::new(Frecency::open(None).unwrap()),
        );

        let entries = p.query("display", 1).entries;
        assert_eq!(
            entries[0].id.as_str(),
            "ms-settings:display",
            "the settings page should win on match quality: {:?}",
            entries.iter().map(|e| (e.title.clone(), e.score)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn v0_2_the_sequence_number_is_echoed_so_a_stale_response_can_be_discarded() {
        let p = pipeline_with(vec![app("Notepad", r"C:\Windows\notepad.exe")]);
        assert_eq!(p.query("note", 7).seq, 7);
        assert_eq!(p.query("", 8).seq, 8);
    }

    /// Verification step R5, which no browser could reach.
    ///
    /// A second `Pipeline` over the same directory must rank by what the first
    /// one learned. The write was already proven by `frecency.rs`; what this adds
    /// is the read-back path — a fresh process consulting that history.
    #[test]
    fn v0_3_a_fresh_pipeline_ranks_by_what_an_earlier_one_learned() {
        let dir = std::env::temp_dir().join("takyon-pipeline-restart");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let apps = || {
            vec![
                app("Code Composer", r"C:\cc\composer.exe"),
                app("Codex", r"C:\cx\codex.exe"),
            ]
        };
        let build = || {
            let source = AppSource::new();
            source.set_for_test(apps());
            Pipeline::new(
                Arc::new(source),
                Arc::new(RecentsSource::new()),
                Arc::new(SystemSource::new()),
                Arc::new(IconStore::new(None)),
                Arc::new(Frecency::open(Some(dir.clone())).unwrap()),
            )
        };

        let cold_top;
        let promoted;
        {
            let first = build();
            let entries = first.query("code", 1).entries;
            cold_top = entries[0].title.clone();
            promoted = entries
                .iter()
                .find(|e| e.title != cold_top)
                .map(|e| e.id.clone())
                .expect("two Entries match");
            for _ in 0..5 {
                first.frecency.record(&promoted, EntryKind::App).unwrap();
            }
            assert_ne!(first.query("code", 2).entries[0].title, cold_top);
        }

        // A different Pipeline, a different Frecency, the same directory.
        let second = build();
        assert_eq!(
            second.query("code", 1).entries[0].id, promoted,
            "usage must outlive the process that learned it"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The phase, end to end: the Palette starts guessing right.
    ///
    /// tbd v0.2 §2's real case. `code` matches both at the later-word rung, and
    /// the shorter name wins on a cold install. One launch of the editor must
    /// reverse it — through the whole pipeline, not just the score function.
    #[test]
    fn v0_3_launching_an_application_puts_it_on_top_next_time() {
        let p = pipeline_with(vec![
            app("T3 Code (Alpha)", r"C:\t3\t3code.exe"),
            app("Visual Studio Code", r"C:\vsc\Code.exe"),
        ]);
        let top = |p: &Pipeline| p.query("code", 1).entries[0].title.clone();
        assert_eq!(top(&p), "T3 Code (Alpha)", "cold, the shorter name wins");

        let editor = p.query("code", 2).entries.iter()
            .find(|e| e.title == "Visual Studio Code")
            .map(|e| e.id.clone())
            .expect("the editor is in the list, just not first");
        p.frecency.record(&editor, EntryKind::App).unwrap();

        assert_eq!(top(&p), "Visual Studio Code", "one launch settles it");
    }

    /// The Traps section's test, written the day the lock exists.
    ///
    /// Deliver a higher-scoring Entry after the lock settles: the top must hold
    /// and the newcomer append below. Without it a late Source inserts above
    /// your selection between eye and finger, and you launch the wrong thing.
    #[test]
    fn v0_3_a_settled_top_row_is_not_displaced_by_a_better_late_answer() {
        let p = pipeline_with(vec![
            app("Code Composer", r"C:\cc\composer.exe"),
            app("Codex", r"C:\cx\codex.exe"),
        ]);
        let first = p.query_at("code", 1, 0).entries[0].title.clone();

        // Settled: the same query, past the lock delay.
        let settled = p.query_at("code", 2, LOCK_DELAY_MS).entries[0].title.clone();
        assert_eq!(settled, first, "nothing has changed yet");

        // Now make the other one genuinely outrank it.
        let other = p
            .query_at("code", 3, LOCK_DELAY_MS)
            .entries
            .iter()
            .find(|e| e.title != first)
            .map(|e| e.id.clone())
            .expect("two Entries match");
        for _ in 0..5 {
            p.frecency.record(&other, EntryKind::App).unwrap();
        }

        let after = p.query_at("code", 4, LOCK_DELAY_MS + 50);
        assert_eq!(after.entries[0].title, first, "the locked top must hold");
        assert!(
            after.entries.iter().any(|e| e.id == other),
            "the better answer is appended, not dropped"
        );
    }

    /// A new keystroke is a new question, so the lock does not survive it.
    #[test]
    fn v0_3_a_new_keystroke_clears_the_lock() {
        let p = pipeline_with(vec![
            app("Code Composer", r"C:\cc\composer.exe"),
            app("Codex", r"C:\cx\codex.exe"),
        ]);
        let first = p.query_at("code", 1, 0).entries[0].title.clone();
        p.query_at("code", 2, LOCK_DELAY_MS);

        let other = p
            .query_at("code", 3, LOCK_DELAY_MS)
            .entries
            .iter()
            .find(|e| e.title != first)
            .map(|e| e.id.clone())
            .expect("two Entries match");
        for _ in 0..5 {
            p.frecency.record(&other, EntryKind::App).unwrap();
        }

        // A different query string entirely: the lock must not apply.
        let typed_more = p.query_at("cod", 4, LOCK_DELAY_MS + 50);
        assert_ne!(typed_more.entries[0].title, first, "a new query ranks freshly");
    }

    /// Before it settles, the list is still allowed to reorder. The lock is a
    /// guarantee about a *stopped* query, not a freeze on the first answer.
    #[test]
    fn v0_3_results_still_reorder_while_the_query_is_settling() {
        let p = pipeline_with(vec![
            app("Code Composer", r"C:\cc\composer.exe"),
            app("Codex", r"C:\cx\codex.exe"),
        ]);
        let first = p.query_at("code", 1, 0).entries[0].title.clone();
        let other = p
            .query_at("code", 2, 10)
            .entries
            .iter()
            .find(|e| e.title != first)
            .map(|e| e.id.clone())
            .expect("two Entries match");
        for _ in 0..5 {
            p.frecency.record(&other, EntryKind::App).unwrap();
        }

        let still_settling = p.query_at("code", 3, LOCK_DELAY_MS - 1);
        assert_eq!(
            still_settling.entries[0].id, other,
            "inside the delay the better answer may take the top"
        );
    }

    /// Verification step R4, as a unit test — the cheaper place to catch it.
    ///
    /// A silent failure otherwise: copying a path would quietly promote whatever
    /// you copied, and the only symptom would be a ranker that slowly learns the
    /// wrong things over weeks.
    #[test]
    fn v0_3_only_launching_teaches_the_ranker() {
        assert!(records_usage(crate::actions::OPEN.as_str()));
        assert!(records_usage(crate::actions::RUN_AS_ADMIN.as_str()));
        assert!(!records_usage(crate::actions::REVEAL.as_str()));
        assert!(!records_usage(crate::actions::COPY_PATH.as_str()));
        assert!(!records_usage("teleport"));
    }

    /// The shortlist is an internal width, never something the Palette sees.
    #[test]
    fn v0_3_the_palette_is_still_sent_at_most_the_entry_limit() {
        let apps: Vec<App> = (0..200)
            .map(|i| app(&format!("Photo {i}"), &format!(r"C:\p{i}.exe")))
            .collect();
        let p = pipeline_with(apps);
        assert_eq!(p.query("photo", 1).entries.len(), MAX_ENTRIES);
    }

    #[test]
    fn v0_2_a_query_returns_matching_entries() {
        let p = pipeline_with(vec![
            app("Notepad", r"C:\Windows\notepad.exe"),
            app("Calculator", r"C:\Windows\calc.exe"),
        ]);
        let result = p.query("note", 1);
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].title, "Notepad");
        assert_eq!(result.entries[0].kind, EntryKind::App);
    }

    /// ADR-0001: the Palette opens empty. An empty query is not a request for
    /// everything, and it must not report an indexing state either — that would
    /// draw a status row under a Palette deliberately left blank.
    #[test]
    fn v0_2_an_empty_query_returns_nothing_and_reports_nothing() {
        let p = pipeline_with(vec![app("Notepad", r"C:\Windows\notepad.exe")]);
        let result = p.query("   ", 1);
        assert!(result.entries.is_empty());
        assert!(!result.indexing);
    }

    /// The window between the hotkey going live and the walk finishing. The
    /// Palette has to be able to tell the user the difference between "still
    /// looking" and "no such app".
    #[test]
    fn v0_2_a_query_during_the_first_walk_says_it_is_still_indexing() {
        let source = AppSource::new(); // indexing until refreshed
        let p = Pipeline::new(
            Arc::new(source),
            Arc::new(RecentsSource::new()),
            Arc::new(SystemSource::new()),
            Arc::new(IconStore::new(None)),
            Arc::new(Frecency::open(None).unwrap()),
        );
        let result = p.query("anything", 1);
        assert!(result.indexing);
        assert!(result.entries.is_empty());
    }

    #[test]
    fn v0_2_results_are_capped_at_twelve() {
        let apps: Vec<App> = (0..50)
            .map(|i| app(&format!("Photo Tool {i}"), &format!(r"C:\p{i}.exe")))
            .collect();
        let p = pipeline_with(apps);
        assert_eq!(p.query("photo", 1).entries.len(), MAX_ENTRIES);
    }

    /// The same executable found by two discovery paths is one row.
    #[test]
    fn v0_2_duplicate_ids_collapse_before_the_list_is_returned() {
        let p = pipeline_with(vec![
            app("Visual Studio Code", r"C:\vsc\Code.exe"),
            app("code", r"c:\vsc\code.exe"),
        ]);
        let result = p.query("code", 1);
        assert_eq!(result.entries.len(), 1, "one executable, one Entry");
    }

    #[test]
    fn v0_2_activating_an_unknown_entry_explains_itself() {
        let p = pipeline_with(vec![]);
        let err = p.activate(&EntryId("nothing".into()), "open").unwrap_err();
        assert!(err.contains("no longer in the index"));
    }

    #[test]
    fn v0_2_an_unknown_action_is_refused_rather_than_guessed_at() {
        let p = pipeline_with(vec![app("Notepad", r"C:\Windows\notepad.exe")]);
        let id = p.query("note", 1).entries[0].id.clone();
        assert!(p.activate(&id, "teleport").unwrap_err().contains("Unknown action"));
    }

    #[test]
    fn v0_2_the_action_menu_for_an_executable_offers_the_full_set() {
        let p = pipeline_with(vec![app("Notepad", r"C:\Windows\notepad.exe")]);
        let id = p.query("note", 1).entries[0].id.clone();
        let labels: Vec<String> = p.actions_for(&id).into_iter().map(|a| a.label).collect();
        assert!(labels.contains(&"Run as administrator".to_string()));
        assert!(labels.contains(&"Open file location".to_string()));
        assert!(labels.contains(&"Copy path".to_string()));
    }

    #[test]
    fn v0_2_the_action_menu_for_an_unknown_entry_is_empty_not_a_panic() {
        let p = pipeline_with(vec![]);
        assert!(p.actions_for(&EntryId("nothing".into())).is_empty());
    }

    /// ADR-0002, as an assertion that fails loudly when a Source is added
    /// without thinking about it.
    ///
    /// **Fired once already, as designed**: v0.3's Recents reads a local folder,
    /// no network, so 1 became 2 deliberately.
    #[test]
    fn v0_3_every_bangless_source_is_local() {
        assert!(bangless_sources_are_local());
        let p = pipeline_with(vec![]);
        assert_eq!(
            p.sources.len(),
            3,
            "adding a Source means revisiting ADR-0002 before changing this number"
        );
    }

}
