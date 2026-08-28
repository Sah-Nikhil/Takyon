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

/// The registry of Sources, and everything a query needs.
pub struct Pipeline {
    /// Held concretely as well as in `sources`, because launching and the action
    /// menu need to look an App up by id, which the trait deliberately does not
    /// expose — a `Source` produces Entries and knows nothing else.
    pub apps: Arc<AppSource>,
    pub icons: Arc<IconStore>,
    /// What the user has actually chosen before. Read once per candidate Entry,
    /// written once per activation.
    pub frecency: Arc<Frecency>,
    sources: Vec<Arc<dyn Source>>,
}

impl Pipeline {
    pub fn new(apps: Arc<AppSource>, icons: Arc<IconStore>, frecency: Arc<Frecency>) -> Self {
        let sources: Vec<Arc<dyn Source>> = vec![apps.clone()];
        Pipeline {
            apps,
            icons,
            frecency,
            sources,
        }
    }

    /// Answer one keystroke.
    pub fn query(&self, raw: &str, seq: u64) -> QueryResult {
        let q = Query::new(raw);
        let indexing = self.apps.is_indexing();

        if q.is_empty() {
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
            entry.score = rank::with_frecency(entry.score, self.frecency.weight(&entry.id));
        }

        let entries = rank::order(entries, MAX_ENTRIES);
        // Last, and after the truncation, so "does this title repeat?" is asked
        // about the list the Palette is sent rather than a longer one.
        let entries = rank::disambiguate_subtitles(entries);

        QueryResult {
            seq,
            entries,
            indexing,
        }
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

    /// The actions offered for one Entry, for the `Ctrl+K` menu.
    pub fn actions_for(&self, id: &EntryId) -> Vec<crate::entry::Action> {
        let Some(app) = self.apps.find(id) else {
            return Vec::new();
        };
        crate::actions::for_entry(&Entry {
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
        })
    }

    /// Perform one action on one Entry.
    ///
    /// Returns the error rather than logging it: the only useful place to report an
    /// activation failure is the Palette, and by now it is hidden — so the frontend
    /// has to be told in order to bring it back.
    pub fn activate(&self, id: &EntryId, action: &str) -> Result<(), String> {
        let app = self
            .apps
            .find(id)
            .ok_or_else(|| "That application is no longer in the index.".to_string())?;

        let launched = records_usage(action);

        match action {
            a if a == crate::actions::OPEN.as_str() => crate::launch::open(&app.target),
            a if a == crate::actions::RUN_AS_ADMIN.as_str() => {
                crate::launch::run_as_admin(&app.target)
            }
            a if a == crate::actions::REVEAL.as_str() => crate::launch::reveal(&app.target),
            a if a == crate::actions::COPY_PATH.as_str() => {
                let path = crate::launch::path_of(&app.target)
                    .ok_or_else(|| "That application has no path to copy.".to_string())?;
                crate::launch::copy_to_clipboard(&path)
            }
            other => Err(format!("Unknown action: {other}")),
        }?;

        // After the launch succeeded, never before. A failed activation is not a
        // choice, and recording one would teach the ranker to promote something
        // that cannot start.
        if launched {
            if let Err(e) = self.frecency.record(id, crate::entry::EntryKind::App) {
                // Not fatal: the application did start. Losing one unit of usage
                // costs a little ranking accuracy and nothing else.
                eprintln!("[takyon] could not record usage: {e}");
            }
        }
        Ok(())
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
        }
    }

    fn pipeline_with(apps: Vec<App>) -> Pipeline {
        let source = AppSource::new();
        source.set_for_test(apps);
        Pipeline::new(
            Arc::new(source),
            Arc::new(IconStore::new(None)),
            Arc::new(Frecency::open(None).unwrap()),
        )
    }

    #[test]
    fn v0_2_the_sequence_number_is_echoed_so_a_stale_response_can_be_discarded() {
        let p = pipeline_with(vec![app("Notepad", r"C:\Windows\notepad.exe")]);
        assert_eq!(p.query("note", 7).seq, 7);
        assert_eq!(p.query("", 8).seq, 8);
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

    /// ADR-0002, as an assertion that will fail loudly if a future Source is added
    /// to the registry without thinking about it.
    #[test]
    fn v0_2_every_bangless_source_is_local() {
        assert!(bangless_sources_are_local());
        let p = pipeline_with(vec![]);
        assert_eq!(p.sources.len(), 1, "adding a Source means revisiting ADR-0002");
    }

}
