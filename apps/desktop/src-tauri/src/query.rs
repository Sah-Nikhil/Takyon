//! The query pipeline (IMPLEMENTATION_PLAN §3).
//!
//! ```text
//! input ─▶ bang::parse ─┬─▶ Bangless ─▶ fan out (rayon, 20 ms) ─▶ rank ─▶ top 12
//!                       └─▶ Bang(mode, rest) ─▶ that Mode alone
//! ```
//!
//! v0.2 builds the left branch with one Source. `bang.rs` is v0.9 and the
//! Stability lock v0.3; both are in the flow above because this file's shape
//! decides whether adding them is a line or a rewrite.
//!
//! **One `invoke` per keystroke, never one per Source** (ADR-0009). Every Source
//! registered below is local, which is how ADR-0002 stays checkable by reading.

use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;

use crate::bang::{self, Route};
use crate::clips::{Clip, ClipStore};
use crate::entry::{Entry, EntryId, Query, Source, MAX_ENTRIES, SOURCE_BUDGET};
use crate::index::FileIndex;
use crate::frecency::Frecency;
use crate::icons::IconStore;
use crate::rank;
use crate::sources::apps::AppSource;
use crate::sources::calc::CalcSource;
use crate::sources::commands::{CommandId, CommandSource};
use crate::sources::files::{self, FileSource};
use crate::sources::recents::RecentsSource;
use crate::sources::system::SystemSource;

/// What one keystroke gets back.
///
/// `Default` is the empty answer, which the Mode branches spread over: they
/// differ in one or two fields and listing the rest is how one gets forgotten.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    /// Echoed from the request. **The frontend discards any response whose `seq`
    /// is lower than the newest it has seen** (§3) — without this a slow
    /// keystroke's results overwrite a fast one's, and the Palette shows answers
    /// for a prefix of what is now in the input box.
    pub seq: u64,
    pub entries: Vec<Entry>,
    /// Reserve one row in the window for a status line.
    ///
    /// **Not "the application walk is running"**, which it also meant until
    /// v0.9: `!s` inherited that and drew two rows in a window sized for one.
    /// The walk reports in Settings and the tray (`docs/tbd/v0.9.md` §10).
    pub status_row: bool,
    /// Present exactly when the line is `!c` (v0.8). The Ask Mode has no Entries
    /// to rank — the answer streams over `takyon://turn` — so this carries the
    /// question and which Agent would answer it, and `entries` stays empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask: Option<Ask>,
    /// Present exactly when the line is `!s` (v0.9). Same shape as `ask` and for
    /// the same reason: the answer streams, so there is nothing to rank.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web: Option<Web>,
}

/// The `!c` Mode's state for one keystroke.
///
/// Data, not rendered copy: whether to show a Sign-in line or a streaming answer
/// is the frontend's call, the way `file_index_status` splits at v0.7.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ask {
    /// The question, trimmed. Empty means the Bang alone was typed.
    pub query: String,
    /// Which Agent answers. `None` when every Agent is switched off, which is
    /// the one state `!c` has nothing to offer.
    pub agent: Option<crate::agents::AgentKind>,
    /// The switched-on Agents in preference order (`agents.order`). Preference
    /// only, and no Sign-in state: reading that costs three process spawns, and
    /// this is built on the keystroke path. The Palette refines the choice once
    /// its own probe lands.
    pub order: Vec<crate::agents::AgentKind>,
}


/// The `!s` Mode's state for one keystroke.
///
/// `has_key` is read from a cached flag, never from disk: this is built on the
/// keystroke path. No request is made here either — `!s` reaches the network on
/// Enter, not while being typed (ADR-0002 and the phase's own debounce trap).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Web {
    /// The question, trimmed. Empty means the Bang alone was typed.
    pub query: String,
    /// The keyed service, named so the row can say it.
    pub provider: &'static str,
    /// The service that answers with no key (ADR-0021). A missing key is not a
    /// dead end, so the row names this one instead of asking for a key.
    pub keyless_provider: &'static str,
    /// Whether a key is stored. Read from a cached flag, never from disk.
    pub has_key: bool,
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
    /// Inline arithmetic. Held concretely so Settings can flip its Policy; it has
    /// no index, so unlike the others there is nothing to look an id up in.
    pub calc: Arc<CalcSource>,
    pub icons: Arc<IconStore>,
    /// Built-in commands: "Clipboard History". Held concretely because
    /// activation looks one up by id, which the `Source` trait does not expose.
    pub commands: Arc<CommandSource>,
    /// Files and folders: behind `!e` always, and Bangless when the setting is
    /// on (task 11). `None` only where no index could be built at all.
    pub files: Option<Arc<FileSource>>,
    /// Clipboard history, reached through the command above and, when
    /// `clips.bang` is on, through `!v`.
    ///
    /// **Not in `sources`, and never will be** (ADR-0006): a Source feeds
    /// Bangless by definition, so the guarantee is that the store is not one.
    pub clips: Option<Arc<ClipStore>>,
    /// What the user has actually chosen before. Read once per candidate Entry,
    /// written once per activation.
    pub frecency: Arc<Frecency>,
    /// Whether `!v` routes to clipboard history (`clips.bang`). Read every
    /// keystroke, so atomic rather than behind the Stability mutex.
    clips_bang: std::sync::atomic::AtomicBool,
    recents_on: std::sync::atomic::AtomicBool,
    /// The switched-on Agents in preference order. Cached from `settings.db` for
    /// the same reason `clips_bang` is: it is read on the keystroke path, which
    /// must not touch SQLite.
    ask_order: std::sync::Mutex<Vec<crate::agents::AgentKind>>,
    /// Whether a Brave key is stored, cached for the keystroke path. Written at
    /// startup and on every Settings write, exactly as `ask_order` is.
    web_key: std::sync::atomic::AtomicBool,
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
        // Built here rather than injected like the others, because there is
        // nothing to inject: the calculator holds no index, reads no disk and
        // needs no seeding, so a test has nothing it would want to hand it.
        let calc = Arc::new(CalcSource::new());
        // Commands join the fan-out like any Source. A Command carries no clip
        // content, so ADR-0006 is untouched: what is excluded from Bangless is
        // the clipboard's *contents*, not a row that opens the history.
        let commands = crate::sources::commands::shared();
        let sources: Vec<Arc<dyn Source>> = vec![
            apps.clone(),
            recents.clone(),
            system.clone(),
            calc.clone(),
            commands.clone(),
        ];
        Pipeline {
            apps,
            recents,
            system,
            calc,
            commands,
            icons,
            files: None,
            clips: None,
            clips_bang: std::sync::atomic::AtomicBool::new(true),
            recents_on: std::sync::atomic::AtomicBool::new(true),
            ask_order: std::sync::Mutex::new(crate::agents::AgentKind::ALL.to_vec()),
            web_key: std::sync::atomic::AtomicBool::new(false),
            frecency,
            sources,
            lock: std::sync::Mutex::new(None),
            started: Instant::now(),
        }
    }

    /// Whether `!v` routes to clipboard history (`clips.bang`).
    ///
    /// Atomic rather than behind the Shape mutex: it is read on every keystroke
    /// and written when Settings changes, so a lock here would put contention on
    /// the 30 ms path for a boolean.
    pub fn set_bang_enabled(&self, on: bool) {
        self.clips_bang
            .store(on, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn bang_enabled(&self) -> bool {
        self.clips_bang.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// The Agents `!c` walks. Written by Settings, read every keystroke.
    pub fn set_ask_order(&self, order: Vec<crate::agents::AgentKind>) {
        if let Ok(mut held) = self.ask_order.lock() {
            *held = order;
        }
    }

    pub fn ask_order(&self) -> Vec<crate::agents::AgentKind> {
        self.ask_order
            .lock()
            .map(|held| held.clone())
            .unwrap_or_else(|_| crate::agents::AgentKind::ALL.to_vec())
    }

    /// Whether the Recents Source contributes Entries (v0.6's Launcher page).
    ///
    /// Filtered after the fan-out, not by dropping the Source: it answers from an
    /// in-memory snapshot, and rebuilding `sources` would need a lock per keystroke.
    /// Record whether a Brave key is stored. Settings and startup call this.
    pub fn set_web_key_present(&self, present: bool) {
        self.web_key
            .store(present, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn web_key_present(&self) -> bool {
        self.web_key.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn set_recents_enabled(&self, on: bool) {
        self.recents_on
            .store(on, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn recents_enabled(&self) -> bool {
        self.recents_on.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Attach the clipboard history, enabling `!v`.
    ///
    /// Separate from [`Pipeline::new`] because most of what a Pipeline does has
    /// nothing to do with clips, and every existing test would otherwise have to
    /// construct a database to ask about applications.
    pub fn with_clips(mut self, clips: Arc<ClipStore>) -> Self {
        self.clips = Some(clips);
        self
    }

    /// Attach the file index, enabling `!e`.
    ///
    /// Separate from [`Pipeline::new`] for the same reason as the clips: a test
    /// asking about applications should not have to walk a disk first. It joins
    /// `sources` too, where its own Bangless toggle decides whether it answers.
    pub fn with_files(mut self, files: Arc<FileSource>) -> Self {
        self.sources.push(files.clone());
        self.files = Some(files);
        self
    }

    /// Answer one keystroke.
    pub fn query(&self, raw: &str, seq: u64) -> QueryResult {
        self.query_at(raw, seq, self.started.elapsed().as_millis() as u64)
    }

    /// Answer one keystroke at a given millisecond, for the Stability tests.
    pub fn query_at(&self, raw: &str, seq: u64, now_ms: u64) -> QueryResult {
        // Routed before anything else (§9). A Bang consumes the whole line, so a
        // Mode's query never reaches a Source and a Source's query never reaches
        // a Mode — which is what makes ADR-0002 checkable by reading this file.
        let line = match bang::parse(raw) {
            // Off, and `!v` is text like any other: it falls through to Bangless
            // and matches nothing, rather than erroring. The command is still
            // there for anyone who types "clipboard".
            Route::Clips(needle) if self.bang_enabled() => {
                return self.clips_result(needle, seq)
            }
            Route::Clips(_) => raw,
            // No toggle: `!e` is the door to file search, and task 11's setting
            // governs Bangless Entries rather than the Bang.
            Route::Files(needle) => return self.files_result(needle, seq),
            // No toggle either: `!c` is the only door to an Agent, and which
            // Agent answers is the setting (`docs/plans/bang-registry.md`).
            Route::Ask(question) => return self.ask_result(question, seq, self.ask_order()),
            Route::Web(question) => return self.web_result(question, seq),
            Route::Bangless(line) => line,
        };

        let q = Query::new(line);

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
                // Reporting the walk here would put a status row under a Palette
                // that has been deliberately left blank (ADR-0001).
                status_row: false,
                ask: None,
            web: None,
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
            // The walk no longer draws a row, so a Bangless line reserves none.
            status_row: false,
            ask: None,
            web: None,
        }
    }

    /// The `!c` Mode (v0.8 task 7).
    ///
    /// No Sources, no Frecency, no Stability lock: there is nothing to rank. The
    /// frontend takes it from here and calls `agent_ask`.
    fn ask_result(
        &self,
        question: &str,
        seq: u64,
        order: Vec<crate::agents::AgentKind>,
    ) -> QueryResult {
        let agent = order.first().copied();
        QueryResult {
            seq,
            ask: Some(Ask {
                query: question.to_string(),
                agent,
                order,
            }),
            web: None,
            // Reuses the flag that reserves a status row in the *native* window,
            // exactly as `files_result` does: `!c` has no Entries, and without a
            // reserved row its one line falls outside the window.
            status_row: true,
            ..QueryResult::default()
        }
    }

    /// The `!s` Mode (v0.9 task 5).
    ///
    /// Nothing leaves the machine here. Building this is a struct literal; the
    /// request happens on Enter, in `search::ipc`.
    fn web_result(&self, question: &str, seq: u64) -> QueryResult {
        QueryResult {
            seq,
            web: Some(Web {
                query: question.to_string(),
                provider: crate::search::exa::LABEL,
                keyless_provider: crate::search::ddg::LABEL,
                has_key: self.web_key_present(),
            }),
            // Reserves the status row in the native window, exactly as `!c` does.
            status_row: true,
            ..QueryResult::default()
        }
    }

    /// The `!v` Mode: clipboard history, its own view (v0.5 task 7).
    ///
    /// No Frecency and no Stability lock. Order is recency, which is what anyone
    /// reaching for clipboard history is actually asking for, and a lift from a
    /// usage database would only fight it.
    fn clips_result(&self, needle: &str, seq: u64) -> QueryResult {
        let entries = match &self.clips {
            Some(store) => store
                .search(needle, MAX_ENTRIES)
                .into_iter()
                .map(clip_entry)
                .collect(),
            None => Vec::new(),
        };
        QueryResult {
            seq,
            // ADR-0016 applies here too: the source application is shown only
            // where two clips would otherwise read identically.
            entries: rank::disambiguate_subtitles(entries),
            status_row: false,
            ask: None,
            web: None,
        }
    }

    /// The `!e` Mode (§5 task 10).
    ///
    /// No Frecency and no Stability lock, exactly as `!v`: the index answers in
    /// microseconds, so there is no late arrival for the lock to protect against,
    /// and a usage lift would fight a name the user typed in full.
    fn files_result(&self, needle: &str, seq: u64) -> QueryResult {
        let Some(files) = &self.files else {
            return QueryResult {
                seq,
                ..QueryResult::default()
            };
        };
        let entries = if needle.is_empty() {
            // An empty Bang is its own state, not nothing: it shows what Takyon
            // has opened (TBC-0010). Our table, never the shell's.
            FileSource::recent_entries(&self.frecency, files::MODE_LIMIT)
        } else {
            files.mode_entries(needle, files::MODE_LIMIT)
        };
        QueryResult {
            seq,
            // ADR-0016: two files with the same name are what the second line is
            // for, and under `!e` that is common rather than rare.
            entries: rank::disambiguate_subtitles(entries),
            // Reuses the flag that reserves a status row in the *native* window
            // (TBC-0006 sizes it from the row count, which the webview cannot
            // change). Which words go in that row is the frontend's business —
            // `file_index_status` tells it Building from Stale.
            status_row: !matches!(
                files.index().status(),
                crate::index::IndexStatus::Ready
            ),
            ask: None,
            web: None,
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
            .filter(|e| self.recents_enabled() || e.kind != crate::entry::EntryKind::Recent)
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
        if let Some(recent) = self.recents.find(id) {
            return Some((
                crate::entry::LaunchTarget::Exe {
                    path: recent.target,
                    args: None,
                    working_dir: None,
                },
                recent.kind,
            ));
        }

        // A file Entry's id **is** its path, lowercased (§2), and Windows paths
        // are case-insensitive — so the id opens the file without a second index
        // to look it up in. Checked for existence rather than trusted: an id can
        // outlive the file when the Palette is left open.
        let path = std::path::PathBuf::from(id.as_str());
        if !path.exists() {
            return None;
        }
        let kind = if path.is_dir() {
            crate::entry::EntryKind::Folder
        } else {
            crate::entry::EntryKind::File
        };
        Some((
            crate::entry::LaunchTarget::Exe {
                path,
                args: None,
                working_dir: None,
            },
            kind,
        ))
    }

    /// The actions offered for one Entry, for the `Ctrl+K` menu.
    pub fn actions_for(&self, id: &EntryId) -> Vec<crate::entry::Action> {
        // Before every index lookup: a Clip is in none of them, and its id could
        // not collide with a path anyway (`clip:` is not a legal drive letter).
        if CommandId::from_entry(id).is_some() {
            return crate::actions::for_command()
                .iter()
                .filter_map(crate::actions::describe)
                .collect();
        }
        if clip_id(id).is_some() {
            return crate::actions::for_clip()
                .iter()
                .filter_map(crate::actions::describe)
                .collect();
        }
        if CalcSource::answer_of(id).is_some() {
            return crate::actions::for_calc().iter().filter_map(crate::actions::describe).collect();
        }
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
        // A calculation is answered before anything is looked up, because there
        // is nothing to look up: the Entry was computed for one keystroke and
        // belongs to no index. The answer rides in the id (`sources/calc`).
        if let Some(answer) = CalcSource::answer_of(id) {
            if action != crate::actions::COPY_ANSWER.as_str() {
                return Err(format!("A calculation cannot be {action}ed."));
            }
            return crate::launch::copy_to_clipboard(answer);
            // No `record_activation`: see `records_usage`.
        }

        // A command opens a surface inside the Palette, so it never reaches
        // `launch.rs`. `lib.rs` owns the window half; this only validates.
        if let Some(command) = CommandId::from_entry(id) {
            if action != crate::actions::OPEN_COMMAND.as_str() {
                return Err(format!("A command cannot be {action}ed."));
            }
            self.record_activation(id, crate::entry::EntryKind::Command, action);
            return match command {
                CommandId::ClipboardHistory => Ok(()),
            };
        }
        if let Some(row) = clip_id(id) {
            return self.activate_clip(row, action);
        }

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

    /// Paste, copy or delete one Clip.
    ///
    /// Never records usage: Frecency would reorder history away from recency, and
    /// the Palette has no other way to reach a Clip anyway.
    fn activate_clip(&self, row: i64, action: &str) -> Result<(), String> {
        use crate::clips::paste::{self, Paste};

        let store = self
            .clips
            .as_ref()
            .ok_or_else(|| "Clipboard history is not available.".to_string())?;

        if action == crate::actions::DELETE_CLIP.as_str() {
            return match store.delete(row) {
                0 => Err("That clip is already gone.".to_string()),
                _ => Ok(()),
            };
        }

        let text = store
            .content(row)
            .ok_or_else(|| "That clip is no longer in the history.".to_string())?;
        let clip = Paste {
            kind: crate::clips::ClipKind::Text,
            text: &text,
        };
        match action {
            a if a == crate::actions::PASTE.as_str() => paste::paste_back(&clip),
            a if a == crate::actions::COPY_CLIP.as_str() => paste::to_clipboard(&clip),
            other => Err(format!("A clip cannot be {other}ed.")),
        }
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
        self.record_opened(id, kind);
    }

    /// Also remember it in the Recents list Takyon owns (TBC-0010).
    ///
    /// Only what has a path to reopen: a Calc has none and a Clip is excluded by
    /// ADR-0006. The id is the path for a file and the target for an app, which
    /// is what makes one statement enough here.
    fn record_opened(&self, id: &EntryId, kind: crate::entry::EntryKind) {
        use crate::entry::EntryKind;
        if !matches!(kind, EntryKind::App | EntryKind::File | EntryKind::Folder) {
            return;
        }
        let Some((target, _)) = self.target_for(id) else {
            return;
        };
        let Some(path) = crate::launch::path_of(&target) else {
            return;
        };
        if let Err(e) = self.frecency.record_opened(id, &path, kind) {
            eprintln!("[takyon] could not record what was opened: {e}");
        }
    }
}

/// The `EntryId` namespace clips live in.
///
/// A path can never start with it, so a Clip is distinguishable from every other
/// Entry by its id alone — which is what lets `activate` route one before it
/// touches an index.
pub const CLIP_PREFIX: &str = "clip:";

/// The row behind a Clip's `EntryId`, or `None` if this is not one.
pub fn clip_id(id: &EntryId) -> Option<i64> {
    id.as_str().strip_prefix(CLIP_PREFIX)?.parse().ok()
}

/// One stored clip as the Palette draws it.
///
/// The preview is the title, and the full content never travels: a search
/// response would otherwise ship every matching secret into the webview.
fn clip_entry(clip: Clip) -> Entry {
    Entry {
        id: EntryId(format!("{CLIP_PREFIX}{}", clip.id)),
        title: clip.preview,
        subtitle: clip.source_exe.as_deref().and_then(source_label),
        kind: crate::entry::EntryKind::Clip,
        icon: None,
        // Order is recency, applied by the store's query. A score here would
        // invite something to re-sort by it.
        score: 0.0,
        actions: crate::actions::for_clip(),
        version: None,
    }
}

/// "Bitwarden" from `C:\Program Files\Bitwarden\Bitwarden.exe`.
fn source_label(exe: &str) -> Option<String> {
    let stem = std::path::Path::new(exe).file_stem()?.to_string_lossy();
    (!stem.is_empty()).then(|| stem.to_string())
}

/// Does this action dismiss the Palette before it runs?
///
/// Everything does except deleting a clip: every other action ends the session,
/// and that one edits the list you are still reading.
pub fn hides_palette(action: &str) -> bool {
    action != crate::actions::DELETE_CLIP.as_str()
        // Opening a command navigates *into* a surface in the same window. Hiding
        // would dismiss the thing the user just asked to see.
        && action != crate::actions::OPEN_COMMAND.as_str()
}

/// Does this action count as choosing the application?
///
/// Only a launch teaches the ranker. Revealing a file or copying its path is
/// something people do while looking *for* something, and counting it would
/// train the Palette on the search rather than on the choice.
pub fn records_usage(action: &str) -> bool {
    action == crate::actions::OPEN.as_str()
        || action == crate::actions::RUN_AS_ADMIN.as_str()
        // Opening a command is a choice like launching an app: it is what the
        // user meant by those keystrokes, so it should rank like one.
        || action == crate::actions::OPEN_COMMAND.as_str()
}

/// Present so a future Source cannot quietly become a network client.
///
/// Not idle belt-and-braces: v0.9 adds a `SearchProvider` and v0.8 a subprocess,
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
            origin: crate::sources::apps::AppOrigin::Installed,
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

    /// A pipeline that knows one application and holds three clips.
    fn pipeline_with_clips(clips: &[&str]) -> Pipeline {
        let store = ClipStore::open(None).expect("in-memory clips");
        for (i, text) in clips.iter().enumerate() {
            store
                .insert_at(
                    crate::clips::ClipKind::Text,
                    Some(r"C:\Program Files\Bitwarden\Bitwarden.exe"),
                    text,
                    1_000 + i as i64,
                )
                .expect("insert");
        }
        pipeline_with(vec![app("Notepad", r"C:\Windows
otepad.exe")])
            .with_clips(Arc::new(store))
    }

    /// ADR-0006, as the assertion the whole phase turns on: a Clip is unreachable
    /// from a Bangless query, whatever it contains and however well it matches.
    #[test]
    fn v0_5_a_bangless_query_can_never_return_a_clip() {
        let p = pipeline_with_clips(&["notepad", "hunter2", "notepad is a clip too"]);
        for q in ["notepad", "hunter2", "n", "clip"] {
            let entries = p.query(q, 1).entries;
            assert!(
                entries.iter().all(|e| e.kind != EntryKind::Clip),
                "{q} returned a Clip from a Bangless query"
            );
        }
    }

    /// A pipeline over a small real tree, for the `!e` route.
    fn pipeline_with_files(label: &str) -> (std::path::PathBuf, Pipeline) {
        use crate::index::live::WalkIndex;
        use crate::index::roots::Roots;

        let base = std::env::temp_dir()
            .join("takyon-pipeline-files")
            .join(format!("{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let tree = base.join("tree");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::write(tree.join("ledger.md"), "x").unwrap();

        let index = Arc::new(WalkIndex::load(
            base.join("index"),
            Roots {
                include: vec![tree.clone()],
                exclude: Vec::new(),
            },
        ));
        index.rebuild().unwrap();
        let files = Arc::new(FileSource::new(index));
        (base, pipeline_with(Vec::new()).with_files(files))
    }

    /// `!e` searches files and nothing else — a Bang consumes the whole line, so
    /// no Source sees it (paragraph 9, ADR-0002).
    #[test]
    fn v0_7_the_file_bang_searches_files_only() {
        let (base, p) = pipeline_with_files("search");

        let hits = p.query("!e ledger", 1).entries;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "ledger.md");
        assert_eq!(hits[0].kind, EntryKind::File);
        assert!(p.query("!e nothingatall", 2).entries.is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The Bang works whatever the Bangless setting says: task 11 governs
    /// ordinary results, never the Bang itself.
    #[test]
    fn v0_7_the_file_bang_ignores_the_bangless_setting() {
        let (base, p) = pipeline_with_files("toggle");
        let files = p.files.clone().expect("files attached");

        files.set_bangless(false);
        assert_eq!(p.query("!e ledger", 1).entries.len(), 1);
        assert!(p.query("ledger", 2).entries.is_empty());

        files.set_bangless(true);
        assert_eq!(p.query("ledger", 3).entries.len(), 1);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// An activated file is remembered in the list Takyon owns, and `!e` with no
    /// query shows it back (TBC-0010).
    #[test]
    fn v0_7_an_empty_file_bang_shows_what_takyon_opened() {
        let (base, p) = pipeline_with_files("recents");
        assert!(p.query("!e", 1).entries.is_empty(), "nothing opened yet");

        let entry = p.query("!e ledger", 2).entries.remove(0);
        p.record_activation(&entry.id, EntryKind::File, crate::actions::OPEN.as_str());

        let recents = p.query("!e", 3).entries;
        assert_eq!(recents.len(), 1);
        assert_eq!(recents[0].title, "ledger.md");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// `!v` is the only way in, and it is a view rather than a search: empty
    /// lists the history instead of nothing.
    #[test]
    fn v0_5_the_clip_bang_lists_history_and_searches_it() {
        let p = pipeline_with_clips(&["first", "second", "third"]);

        let all = p.query("!v", 1).entries;
        assert_eq!(all.len(), 3);
        assert!(all.iter().all(|e| e.kind == EntryKind::Clip));
        assert_eq!(all[0].title, "third", "history lists newest first");

        let hit = p.query("!v seco", 2).entries;
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].title, "second");
        assert!(p.query("!v nothing", 3).entries.is_empty());
    }

    /// A Clip's actions never touch the filesystem, and its id round-trips.
    #[test]
    fn v0_5_a_clip_entry_carries_only_clip_actions() {
        let p = pipeline_with_clips(&["token"]);
        let entry = p.query("!v", 1).entries.remove(0);
        assert_eq!(clip_id(&entry.id), Some(1));
        assert_eq!(
            p.actions_for(&entry.id)
                .iter()
                .map(|a| a.id.as_str())
                .collect::<Vec<_>>(),
            ["paste", "copy_clip", "delete_clip"]
        );
    }

    /// Deleting is the one action that leaves the Palette up, and it destroys the
    /// row rather than hiding it.
    #[test]
    fn v0_5_deleting_a_clip_removes_it_and_keeps_the_palette_open() {
        let p = pipeline_with_clips(&["gone soon"]);
        let entry = p.query("!v", 1).entries.remove(0);
        assert!(!hides_palette(crate::actions::DELETE_CLIP.as_str()));
        assert!(hides_palette(crate::actions::PASTE.as_str()));
        p.activate(&entry.id, crate::actions::DELETE_CLIP.as_str())
            .expect("delete");
        assert!(p.query("!v", 2).entries.is_empty());
    }

    /// A pipeline with no clipboard history answers `!v` with an empty list, not
    /// with a Bangless search for the letters after the Bang.
    #[test]
    fn v0_5_the_clip_bang_without_a_store_returns_nothing() {
        let p = pipeline_with(vec![app("Notepad", r"C:\Windows
otepad.exe")]);
        assert!(p.query("!v notepad", 1).entries.is_empty());
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
            origin: crate::sources::apps::AppOrigin::Installed,
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

    /// `!s` is its own Mode with no Entries: the answer streams, exactly as `!c`.
    #[test]
    fn v0_9_the_web_bang_is_a_mode_with_no_entries() {
        let p = pipeline_with(vec![app("Steam", r"C:\steam.exe")]);
        let result = p.query("!s ferrari in f1", 1);
        let web = result.web.expect("!s yields a web payload");
        assert_eq!(web.query, "ferrari in f1");
        assert!(result.entries.is_empty(), "!s ranks nothing");
    }

    /// The Bang alone is a state of its own, and it says whether a key is held —
    /// `!s` with no key must explain itself rather than failing at Enter.
    #[test]
    fn v0_9_the_web_bang_alone_reports_whether_a_key_is_held() {
        let p = pipeline_with(vec![]);
        p.set_web_key_present(false);
        let web = p.query("!s", 1).web.expect("a payload");
        assert_eq!(web.query, "");
        assert!(!web.has_key);

        p.set_web_key_present(true);
        assert!(p.query("!s ferrari", 2).web.expect("a payload").has_key);
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
            origin: crate::sources::apps::AppOrigin::CommandLine,
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
        assert!(!result.status_row);
    }

    /// The window between the hotkey going live and the walk finishing.
    ///
    /// **Amended at v0.9**: the walk reports in Settings and the tray, not the
    /// Palette, so a Bangless line reserves no row while it runs. The Palette
    /// stays a list (`docs/tbd/v0.9.md` §10).
    #[test]
    fn v0_9_a_query_during_the_first_walk_reserves_no_row_in_the_palette() {
        let source = AppSource::new(); // indexing until refreshed
        let p = Pipeline::new(
            Arc::new(source),
            Arc::new(RecentsSource::new()),
            Arc::new(SystemSource::new()),
            Arc::new(IconStore::new(None)),
            Arc::new(Frecency::open(None).unwrap()),
        );
        let result = p.query("anything", 1);
        assert!(!result.status_row);
        assert!(result.entries.is_empty());
        // The walk itself is still reported, just not through the Palette.
        assert!(p.apps.is_indexing());
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

    /// ADR-0002, as an assertion that fails when a Source is added carelessly.
    ///
    /// **Fired three times.** v0.3's Recents reads a local folder; v0.4's
    /// calculator is a parse over static tables (currency, which would have gone
    /// to the network, was left out); v0.5's commands are a `const` table.
    #[test]
    fn v0_3_every_bangless_source_is_local() {
        assert!(bangless_sources_are_local());
        let p = pipeline_with(vec![]);
        assert_eq!(
            p.sources.len(),
            5,
            "adding a Source means revisiting ADR-0002 before changing this number"
        );
    }

    /// ADR-0006's line, drawn where v0.5 moved it.
    ///
    /// A **command** that opens clipboard history is Bangless-reachable; a
    /// **clip** never is. The row carries no content, so a list glanced at over
    /// a shoulder still cannot hold a secret.
    #[test]
    fn v0_5_the_command_is_bangless_but_its_clips_are_not() {
        let p = pipeline_with_clips(&["hunter2 clipboard secret"]);

        let entries = p.query("clipboard", 1).entries;
        assert!(
            entries.iter().any(|e| e.kind == EntryKind::Command),
            "the command must be findable without knowing a Bang exists"
        );
        assert!(entries.iter().all(|e| e.kind != EntryKind::Clip));

        // The clip's own words still reach nothing Bangless.
        assert!(p
            .query("hunter2", 2)
            .entries
            .iter()
            .all(|e| e.kind != EntryKind::Clip));
    }

    /// The Bang is a shortcut over the command, and can be turned off.
    ///
    /// Off, `!v` is text: it falls through to Bangless and matches nothing,
    /// rather than erroring. The command is how you get there either way.
    #[test]
    fn v0_5_turning_off_the_bang_leaves_the_command_working() {
        let p = pipeline_with_clips(&["first", "second"]);
        assert_eq!(p.query("!v", 1).entries.len(), 2);

        p.set_bang_enabled(false);
        assert!(p.query("!v", 2).entries.is_empty());
        assert!(p
            .query("clipboard", 3)
            .entries
            .iter()
            .any(|e| e.kind == EntryKind::Command));

        p.set_bang_enabled(true);
        assert_eq!(p.query("!v", 4).entries.len(), 2);
    }

    /// v0.6's Launcher page can switch Recents off.
    ///
    /// A Source the user has turned off must contribute nothing, and everything
    /// else must be untouched — the switch is about one Source, not about search.
    #[test]
    fn v0_6_turning_recents_off_removes_only_recent_entries() {
        let recents = Arc::new(RecentsSource::new());
        recents.set_for_test(vec![crate::sources::recents::Recent {
            id: EntryId("recent:report".into()),
            title: "report.docx".into(),
            subtitle: None,
            target: PathBuf::from(r"C:\Users\t\Documents\report.docx"),
            kind: EntryKind::Recent,
            hay: Haystack::new("report.docx", Some("report")),
        }]);
        let apps = AppSource::new();
        apps.set_for_test(vec![app("Reporting Studio", r"C:\Apps\reporting.exe")]);
        let p = Pipeline::new(
            Arc::new(apps),
            recents,
            Arc::new(SystemSource::new()),
            Arc::new(IconStore::new(None)),
            Arc::new(Frecency::open(None).unwrap()),
        );

        let on = p.query("report", 1).entries;
        assert!(on.iter().any(|e| e.kind == EntryKind::Recent));
        let apps_on = on.iter().filter(|e| e.kind == EntryKind::App).count();

        p.set_recents_enabled(false);
        let off = p.query("report", 2).entries;
        assert!(!off.iter().any(|e| e.kind == EntryKind::Recent));
        assert_eq!(
            off.iter().filter(|e| e.kind == EntryKind::App).count(),
            apps_on,
            "turning Recents off changed which applications matched"
        );

        p.set_recents_enabled(true);
        assert!(p.query("report", 3).entries.iter().any(|e| e.kind == EntryKind::Recent));
    }

    /// Opening a command navigates rather than launching: the window stays, and
    /// the choice is learned like any other.
    #[test]
    fn v0_5_opening_a_command_keeps_the_palette_and_records_usage() {
        let p = pipeline_with_clips(&["x"]);
        let id = crate::sources::commands::CommandId::ClipboardHistory.entry_id();

        assert!(!hides_palette(crate::actions::OPEN_COMMAND.as_str()));
        assert!(records_usage(crate::actions::OPEN_COMMAND.as_str()));
        p.activate(&id, crate::actions::OPEN_COMMAND.as_str())
            .expect("opening a command");
        assert!(p.frecency.weight(&id) > 0.0);

        // And nothing else is reachable on it.
        assert!(p.activate(&id, crate::actions::OPEN.as_str()).is_err());
    }

    /// v0.4: the calculator answers, and it beats an application outright.
    ///
    /// Not a tie broken by score — `EntryKind::Calc` wins the tier, so no amount
    /// of Frecency on an app can displace an unambiguous expression.
    #[test]
    fn v0_4_an_expression_takes_the_top_row_from_every_application() {
        let p = pipeline_with(vec![app("Code", r"C:\code\Code.exe")]);
        let entries = p.query("12*1.18", 1).entries;
        assert_eq!(entries[0].title, "14.16");
        assert_eq!(entries[0].kind, EntryKind::Calc);
    }

    /// ADR-0016 amended at v0.4: a calculation keeps its expression.
    ///
    /// The subtitle rule strips a second line from any row whose title is unique,
    /// which silently swallowed the expression on its way out of the pipeline —
    /// found by the IPC contract test, invisible to the mocked visual layer.
    #[test]
    fn v0_4_a_calculation_keeps_its_expression_through_the_whole_pipeline() {
        let p = pipeline_with(vec![]);
        let entries = p.query("12*1.18", 1).entries;
        assert_eq!(entries[0].subtitle.as_deref(), Some("12*1.18"));
    }

    /// The `1password` trap, through the whole pipeline rather than the parser
    /// alone: an app whose name starts with a digit keeps its top row.
    #[test]
    fn v0_4_an_app_named_like_a_number_is_not_displaced_by_a_calculation() {
        let p = pipeline_with(vec![app("1Password", r"C:\1p\1Password.exe")]);
        let entries = p.query("1password", 1).entries;
        assert_eq!(entries[0].title, "1Password");
        assert!(entries.iter().all(|e| e.kind != EntryKind::Calc));
    }

    /// Copying an answer is not choosing an application, so it must never reach
    /// the usage database. A `calc:` row in `frecency.db` would also be
    /// permanent junk: the id is the answer, so every distinct sum makes one.
    #[test]
    fn v0_4_copying_an_answer_teaches_the_ranker_nothing() {
        assert!(!records_usage(crate::actions::COPY_ANSWER.as_str()));

        let p = pipeline_with(vec![]);
        let id = EntryId("calc:14.16".into());
        p.record_activation(&id, EntryKind::Calc, crate::actions::COPY_ANSWER.as_str());
        assert_eq!(p.frecency.weight(&id), 0.0);
    }

    /// The `Ctrl+K` menu for a calculation, which has no index to be found in.
    /// Before v0.4 an id no Source claimed produced an empty menu, so this is the
    /// difference between working and silently doing nothing.
    #[test]
    fn v0_4_a_calculation_has_an_action_menu() {
        let p = pipeline_with(vec![]);
        let menu = p.actions_for(&EntryId("calc:14.16".into()));
        assert_eq!(menu.len(), 1);
        assert_eq!(menu[0].id, crate::actions::COPY_ANSWER);
    }

    /// Activating a calculation with anything but its own action is refused
    /// rather than reaching a launch arm. `run_as_admin` on a number would
    /// otherwise raise a UAC prompt for a string.
    #[test]
    fn v0_4_a_calculation_refuses_actions_that_are_not_copying() {
        let p = pipeline_with(vec![]);
        let id = EntryId("calc:14.16".into());
        assert!(p.activate(&id, crate::actions::RUN_AS_ADMIN.as_str()).is_err());
        assert!(p.activate(&id, crate::actions::REVEAL.as_str()).is_err());
    }

}
