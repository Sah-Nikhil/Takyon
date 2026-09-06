//! The `!s` half of the IPC contract (ADR-0009).
//!
//! One command starts a whole search and returns immediately. Progress arrives
//! on `takyon://search`; the answer itself streams on `takyon://turn`, because
//! it *is* a Turn — reusing that channel means buffering, cancellation and the
//! Agent's own failures are not written twice.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::{key, synth, Hit, SearchError};
use crate::agents::{turn::Turns, TurnRequest};
use crate::prefs::Prefs;

/// The event every search streams over. One channel, `searchId` discriminates.
pub const EVENT_SEARCH: &str = "takyon://search";

/// One thing that happened during a search.
/// `rename_all_fields` is load-bearing: `rename_all` renames the variants only,
/// so without it `turnId` reaches the frontend as `turn_id` and reads undefined.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "kind")]
pub enum SearchEvent {
    /// The query has left the machine. The first thing the Palette can say
    /// truthfully, and what the outbound state is drawn from (task 7).
    Searching { provider: &'static str },
    /// Hits are known and their pages are being read. Sent before the answer so
    /// the sources are on screen while it is still being written.
    Reading { sources: Vec<Hit> },
    /// A Turn is answering. Its text arrives on `takyon://turn`.
    Answering { turn_id: u64, agent: String },
    /// Nothing came back. `message` is shown as written.
    Failed { message: String },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    search_id: u64,
    #[serde(flatten)]
    event: SearchEvent,
}

/// Searches that have not finished, so `web_cancel` has something to stop.
#[derive(Default)]
pub struct Searches {
    running: Mutex<std::collections::HashMap<u64, Arc<AtomicBool>>>,
}

impl Searches {
    fn register(&self, id: u64) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        self.running
            .lock()
            .expect("searches mutex")
            .insert(id, flag.clone());
        flag
    }

    fn finish(&self, id: u64) {
        self.running.lock().expect("searches mutex").remove(&id);
    }

    /// Stop a search between steps. A fetch already in flight runs to its own
    /// timeout — WinHTTP has no cancel that is worth the handle bookkeeping.
    pub fn cancel(&self, id: u64) {
        if let Some(flag) = self.running.lock().expect("searches mutex").remove(&id) {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

/// What Settings shows for web search. The key itself never crosses IPC.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSettings {
    /// The keyed provider, used when a key is stored.
    pub provider: &'static str,
    /// The provider that answers with no key at all, and whenever the keyed one
    /// fails (ADR-0021). Named so Settings can say `!s` works without a key.
    pub keyless_provider: &'static str,
    pub has_key: bool,
    /// Last four characters of the stored key, so a wrong paste is visible.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Where to get a key. In the response rather than the component, so the
    /// provider and its sign-up page move together.
    pub signup_url: &'static str,
}

/// Where a key comes from. Exa's dashboard (ADR-0021).
const SIGNUP_URL: &str = super::exa::SIGNUP_URL;

#[tauri::command(async)]
pub fn web_settings(app: AppHandle) -> WebSettings {
    let dir = data_dir(&app);
    WebSettings {
        provider: super::exa::LABEL,
        keyless_provider: super::ddg::LABEL,
        has_key: dir.as_deref().map(key::present).unwrap_or(false),
        hint: dir.as_deref().and_then(key::hint),
        signup_url: SIGNUP_URL,
    }
}

/// Store the key, or clear it with a blank string.
///
/// The cached flag in `Pipeline` is updated from what was stored rather than
/// from the argument, so the Palette row and the file cannot disagree.
#[tauri::command(async)]
pub fn set_web_key(
    app: AppHandle,
    key_value: String,
    pipeline: tauri::State<'_, Arc<crate::query::Pipeline>>,
) -> Result<(), String> {
    let dir = data_dir(&app).ok_or("Takyon has no data directory to store a key in.")?;
    key::store(&dir, &key_value).map_err(|e| e.to_string())?;
    pipeline.set_web_key_present(key::present(&dir));
    Ok(())
}

/// Run one search. Returns its id immediately; everything else is an event.
#[tauri::command(async)]
pub fn web_search(
    app: AppHandle,
    query: String,
    searches: tauri::State<'_, Arc<Searches>>,
    turns: tauri::State<'_, Arc<Turns>>,
    prefs: tauri::State<'_, Arc<Prefs>>,
) -> Result<u64, String> {
    let query = query.trim().to_string();
    if query.is_empty() {
        return Err("Nothing to search for.".into());
    }
    let id = next_search_id();
    let cancelled = searches.register(id);
    let searches = searches.inner().clone();
    let turns = turns.inner().clone();
    let prefs = prefs.inner().clone();

    std::thread::spawn(move || {
        let outcome = run(&app, id, &query, &cancelled, &turns, &prefs);
        searches.finish(id);
        if let Err(error) = outcome {
            emit(
                &app,
                id,
                SearchEvent::Failed {
                    message: error.message(),
                },
            );
        }
    });
    Ok(id)
}

/// Open one source in the default browser. http(s) only — these URLs come from
/// a remote provider, and the shell would happily run anything else.
#[tauri::command(async)]
pub fn open_url(url: String) -> Result<(), String> {
    super::browser::open_url(&url)
}

/// Enter on `!s`: the query, in the default browser's own search engine.
#[tauri::command(async)]
pub fn open_web_query(query: String) -> Result<(), String> {
    super::browser::open_query(&query)
}

/// Stop a search. The Turn it started is stopped separately, by `agent_cancel`.
#[tauri::command(async)]
pub fn web_cancel(search_id: u64, searches: tauri::State<'_, Arc<Searches>>) {
    searches.cancel(search_id);
}

/// Search, read, then ask. Every step checks `cancelled` first: a dismissed
/// Palette must not still be spending an Agent's tokens two seconds later.
fn run(
    app: &AppHandle,
    id: u64,
    query: &str,
    cancelled: &AtomicBool,
    turns: &Arc<Turns>,
    prefs: &Prefs,
) -> Result<(), SearchError> {
    // No key is not an error since ADR-0021: it selects DuckDuckGo.
    let stored = data_dir(app).and_then(|dir| key::load(&dir));

    let answered = super::search(
        super::keyed().as_ref(),
        super::keyless().as_ref(),
        query,
        stored.as_deref(),
        // Once per provider actually contacted, so a fallback repaints the
        // outbound header rather than leaving it naming a service that did not
        // answer. The Palette treats a second `searching` as a correction.
        |provider| emit(app, id, SearchEvent::Searching { provider }),
    )?;
    let hits = answered.hits;
    if hits.is_empty() {
        return Err(SearchError::Failed(format!(
            "{} found nothing for that.",
            answered.provider
        )));
    }
    if cancelled.load(Ordering::Relaxed) {
        return Ok(());
    }

    emit(
        app,
        id,
        SearchEvent::Reading {
            sources: hits.clone(),
        },
    );
    let urls: Vec<String> = hits.iter().map(|hit| hit.url.clone()).collect();
    let pages = super::fetch::pages(&urls);
    /*
      Icons, from the pages just read and the hosts already contacted (ADR-0022).
      Before the Turn rather than after: the Agent takes seconds, so this costs
      nothing visible, and the source list is drawn with its icons already there
      instead of swapping them in under the reader.
     */
    if let Some(dir) = data_dir(app) {
        super::favicon::cache_all(&dir, &urls, &pages);
    }
    let citations = synth::citations(hits, pages);
    if cancelled.load(Ordering::Relaxed) {
        return Ok(());
    }

    let agent = synth::agent(prefs).ok_or_else(|| {
        SearchError::Failed(
            "Every Agent is switched off, so there is nothing to write the answer.".into(),
        )
    })?;
    let driver = crate::agents::driver_for(agent)
        .ok_or_else(|| SearchError::Failed("This build does not ship that Agent.".into()))?;

    let turn_id = crate::agents::ipc::next_turn_id();
    emit(
        app,
        id,
        SearchEvent::Answering {
            turn_id,
            agent: driver.label().to_string(),
        },
    );
    turns.clone().start(
        app.clone(),
        turn_id,
        driver,
        TurnRequest {
            prompt: synth::prompt(query, &citations),
            // Scratch and tools off, exactly as `!c`'s first Turn: summarising
            // pages is a read, and nothing about it needs to touch a disk.
            cwd: crate::agents::scratch::dir(),
            session: None,
            model: prefs
                .get(&crate::prefs::ask_model_key(agent))
                .filter(|m| !m.trim().is_empty()),
            effort: prefs
                .get(&crate::prefs::ask_effort_key(agent))
                .filter(|e| !e.trim().is_empty()),
            tools: false,
        },
    );
    Ok(())
}

fn emit(app: &AppHandle, search_id: u64, event: SearchEvent) {
    let _ = app.emit(EVENT_SEARCH, Envelope { search_id, event });
}

fn data_dir(app: &AppHandle) -> Option<std::path::PathBuf> {
    let _ = app;
    crate::identity::data_dir()
}

fn next_search_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire shape the frontend switches on. A rename here is an IPC break.
    #[test]
    fn v0_9_search_events_serialise_with_a_kind_tag() {
        let json = serde_json::to_string(&SearchEvent::Searching {
            provider: "Brave Search",
        })
        .unwrap();
        assert_eq!(json, r#"{"kind":"searching","provider":"Brave Search"}"#);
    }

    /// The envelope flattens, so the frontend reads one object rather than two,
    /// and carries the Turn id that links a search to its streaming answer.
    #[test]
    fn v0_9_an_envelope_carries_the_search_id_and_the_turn_it_started() {
        let json = serde_json::to_string(&Envelope {
            search_id: 3,
            event: SearchEvent::Answering {
                turn_id: 9,
                agent: "Claude Code".into(),
            },
        })
        .unwrap();
        assert!(json.contains(r#""searchId":3"#));
        assert!(json.contains(r#""turnId":9"#));
    }

    /// Cancelling a search that never ran must be silent — the Palette can
    /// dismiss between the command returning and the thread registering.
    #[test]
    fn v0_9_cancelling_an_unknown_search_does_nothing() {
        Searches::default().cancel(404);
    }

    /// A cancelled search is forgotten, so its id cannot be cancelled twice and
    /// the map cannot grow for the life of the process.
    #[test]
    fn v0_9_a_cancelled_search_is_dropped_from_the_registry() {
        let searches = Searches::default();
        let flag = searches.register(1);
        searches.cancel(1);
        assert!(flag.load(Ordering::Relaxed));
        assert!(searches.running.lock().unwrap().is_empty());
    }
}
