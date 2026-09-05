//! The Agent half of the IPC contract (ADR-0009).
//!
//! Every command here is `async`, which on Tauri means "off the main thread".
//! All of them either spawn a process or wait on one, and the main thread is the
//! one the Palette is painted from.

use std::sync::Arc;

use serde::Serialize;

use super::{scratch, turn::Turns, AgentKind, Snapshot, TurnRequest};
use crate::prefs::{self, Prefs};

/// The Agent preferences a window reads on mount.
///
/// One response rather than one call per control, for the reason
/// `settings_snapshot` gives: Settings mounts every card at once.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSettings {
    /// The preference order, first to last. Every Agent appears once, switched
    /// off ones included — Settings ranks all of them.
    pub order: Vec<AgentKind>,
    /// Which Agents are switched on. `!c` walks the order and skips the rest.
    pub enabled: std::collections::HashMap<String, bool>,
    /// Empty means the Scratch directory. Never the process cwd, which for a
    /// launcher is wherever Windows started it.
    pub cwd: String,
    /// The Scratch path, shown as the placeholder so the default is visible.
    pub scratch: String,
    /// The locked-in model per Agent, keyed by its stored spelling. Absent means
    /// the Agent's own default, which is what a fresh install has.
    pub models: std::collections::HashMap<String, String>,
    /// The locked-in effort level per Agent, same keying and same rule.
    pub efforts: std::collections::HashMap<String, String>,
}

/// Probe every Agent. Three process spawns, so never on the login path.
#[tauri::command(async)]
pub fn agent_snapshots() -> Vec<Snapshot> {
    super::snapshots()
}

#[tauri::command(async)]
pub fn agent_settings(prefs: tauri::State<'_, Arc<Prefs>>) -> AgentSettings {
    let models = AgentKind::ALL
        .iter()
        .filter_map(|kind| {
            prefs
                .get(&prefs::ask_model_key(*kind))
                .filter(|model| !model.trim().is_empty())
                .map(|model| (kind.as_str().to_string(), model))
        })
        .collect();
    let efforts = AgentKind::ALL
        .iter()
        .filter_map(|kind| {
            prefs
                .get(&prefs::ask_effort_key(*kind))
                .filter(|effort| !effort.trim().is_empty())
                .map(|effort| (kind.as_str().to_string(), effort))
        })
        .collect();
    let enabled = AgentKind::ALL
        .iter()
        .map(|kind| {
            let on = prefs::flag(&prefs, &prefs::ask_enabled_key(*kind), true);
            (kind.as_str().to_string(), on)
        })
        .collect();
    AgentSettings {
        order: super::parse_order(prefs.get(prefs::ASK_ORDER).as_deref()),
        enabled,
        cwd: prefs.get(prefs::ASK_CWD).unwrap_or_default(),
        scratch: scratch::dir().to_string_lossy().to_string(),
        models,
        efforts,
    }
}

/// The models one Agent offers, for the Settings picker.
///
/// A spawn per call, so Settings asks for one Agent at a time and `!c` never
/// asks at all — it reads the locked-in choice, not the list.
#[tauri::command(async)]
pub fn agent_models(agent: AgentKind) -> Vec<String> {
    super::models_for(agent)
}

/// Rank the Agents `!c` tries. Writes the preference and the cached copy.
///
/// Normalised rather than trusted: a list short one Agent is a list `!c` cannot
/// fall back through, so what is stored is always all of them, once each.
#[tauri::command(async)]
pub fn set_ask_order(
    order: Vec<AgentKind>,
    prefs: tauri::State<'_, Arc<Prefs>>,
    pipeline: tauri::State<'_, Arc<crate::query::Pipeline>>,
) -> Result<(), String> {
    let order = super::normalise_order(order);
    prefs
        .set(prefs::ASK_ORDER, &super::order_to_json(&order))
        .map_err(|e| e.to_string())?;
    // Recomputed from what was just stored rather than from `order`, so the
    // cached list and the switches can never disagree.
    pipeline.set_ask_order(super::route(&prefs));
    Ok(())
}

/// Switch one Agent on or off. Off is skipped by `!c` without being probed.
///
/// The switch, not Sign-in state, is what makes the Palette instant: knowing
/// whether an Agent is signed in costs a process, and this costs a lookup.
#[tauri::command(async)]
pub fn set_ask_enabled(
    agent: AgentKind,
    enabled: bool,
    prefs: tauri::State<'_, Arc<Prefs>>,
    pipeline: tauri::State<'_, Arc<crate::query::Pipeline>>,
) -> Result<(), String> {
    prefs
        .set(&prefs::ask_enabled_key(agent), if enabled { "1" } else { "0" })
        .map_err(|e| e.to_string())?;
    pipeline.set_ask_order(super::route(&prefs));
    Ok(())
}

/// Where a Turn runs. Blank restores the Scratch directory.
#[tauri::command(async)]
pub fn set_ask_cwd(path: String, prefs: tauri::State<'_, Arc<Prefs>>) -> Result<(), String> {
    let path = path.trim();
    if !path.is_empty() && !std::path::Path::new(path).is_dir() {
        return Err(format!("{path} is not a folder."));
    }
    prefs.set(prefs::ASK_CWD, path).map_err(|e| e.to_string())
}

/// Lock in the model this Agent must use. Blank restores the Agent's default.
#[tauri::command(async)]
pub fn set_ask_model(
    agent: AgentKind,
    model: String,
    prefs: tauri::State<'_, Arc<Prefs>>,
) -> Result<(), String> {
    prefs
        .set(&prefs::ask_model_key(agent), model.trim())
        .map_err(|e| e.to_string())
}

/// Lock in the effort level. Refused unless the Agent accepts that word — each
/// spells effort differently, and a wrong one fails the Turn, not the setting.
#[tauri::command(async)]
pub fn set_ask_effort(
    agent: AgentKind,
    effort: String,
    prefs: tauri::State<'_, Arc<Prefs>>,
) -> Result<(), String> {
    let effort = effort.trim();
    let accepted = super::driver_for(agent).is_some_and(|driver| {
        effort.is_empty() || driver.efforts().contains(&effort)
    });
    if !accepted {
        return Err(format!("{effort} is not an effort level that agent accepts."));
    }
    prefs
        .set(&prefs::ask_effort_key(agent), effort)
        .map_err(|e| e.to_string())
}

/// Start a Turn. Returns its id immediately; the answer arrives as events.
///
/// `tools` is false for the first Turn and true for every follow-up: the answer
/// you get by reflex, one keystroke from the global hotkey, has no tools; asking
/// again is an explicit act (ADR-0017).
#[tauri::command(async)]
pub fn agent_ask(
    app: tauri::AppHandle,
    agent: AgentKind,
    prompt: String,
    session: Option<String>,
    tools: bool,
    turns: tauri::State<'_, Arc<Turns>>,
    prefs: tauri::State<'_, Arc<Prefs>>,
) -> Result<u64, String> {
    let driver = super::driver_for(agent).ok_or("This build does not ship that Agent.")?;
    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        return Err("Nothing to ask.".into());
    }
    let request = TurnRequest {
        prompt,
        cwd: scratch::resolve(prefs.get(prefs::ASK_CWD).as_deref()),
        session,
        // Read from `settings.db` here, never taken from the caller: the model
        // and effort are locked in Settings and are the only pair a Turn can
        // use, so the frontend has no say in them.
        model: prefs
            .get(&prefs::ask_model_key(agent))
            .filter(|model| !model.trim().is_empty()),
        effort: prefs
            .get(&prefs::ask_effort_key(agent))
            .filter(|effort| !effort.trim().is_empty()),
        tools,
    };
    let turn_id = next_turn_id();
    turns.inner().clone().start(app, turn_id, driver, request);
    Ok(turn_id)
}

/// Stop a Turn. Only this does — never the Palette's Escape.
#[tauri::command(async)]
pub fn agent_cancel(turn_id: u64, turns: tauri::State<'_, Arc<Turns>>) {
    turns.cancel(turn_id);
}

/// Turn ids are minted here, not by the frontend: two windows asking at once
/// would otherwise collide on the one event channel. `!s` mints from the same
/// counter, so a search's answer and an `!c` answer can never share an id.
pub fn next_turn_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ids are unique and monotonic, or one Turn's events render into another's
    /// conversation.
    #[test]
    fn v0_8_turn_ids_never_repeat() {
        let first = next_turn_id();
        let second = next_turn_id();
        assert!(second > first);
    }
}
