//! Focus IPC: every command the Palette, the bar and Settings call, and the
//! one event Rust sends back (`takyon://focus`).
//!
//! The frontend sends local time of day for `until …` durations; Rust holds
//! no timezone logic (`duration.rs`).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use super::categories::{builtins, schema_error, Category};
use super::duration::{self, DurationError};
use super::runtime::{now_ms, Signal};
use super::service::{FocusError, View};
use super::session::Strictness;
use super::settings::{themed_page, PageTheme};
use super::store::BlockList;
use super::Focus;
use crate::prefs::{self, Prefs};

pub const EVENT_FOCUS: &str = "takyon://focus";

/// The themed block page for the running session, shared with the runtime.
pub type PagePath = Arc<Mutex<Option<PathBuf>>>;

pub fn emit(app: &AppHandle, signal: Signal) {
    let _ = app.emit(EVENT_FOCUS, signal);
}

/// Write the block page into the data directory, painted in `theme`.
pub fn write_page(theme: Option<&PageTheme>) -> Option<PathBuf> {
    let dir = crate::identity::data_dir()?.join("focus");
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("blocked.html");
    std::fs::write(&path, themed_page(theme)).ok()?;
    Some(path)
}

fn duration_reason(e: DurationError) -> String {
    match e {
        DurationError::Unrecognised => "Not a length or a time".into(),
        DurationError::Zero => "Too short".into(),
        DurationError::OverDay => "Longer than a day".into(),
    }
}

/// Live validation for the form's Duration field: ms, or why not.
#[tauri::command]
pub fn focus_parse_duration(text: String, local_ms_of_day: u32) -> Result<u64, String> {
    duration::parse(&text, local_ms_of_day).map_err(duration_reason)
}

/// `!f` line split into goal and duration.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BangLine {
    goal: String,
    length_ms: Option<u64>,
    error: Option<String>,
}

#[tauri::command]
pub fn focus_parse_bang(line: String, local_ms_of_day: u32) -> BangLine {
    let tail = duration::split_tail(&line, local_ms_of_day);
    BangLine {
        goal: tail.goal.to_string(),
        length_ms: tail
            .duration
            .as_ref()
            .and_then(|d| d.as_ref().ok().copied()),
        error: tail.duration.and_then(|d| d.err()).map(duration_reason),
    }
}

#[tauri::command]
pub fn focus_view(focus: State<'_, Arc<Focus>>) -> View {
    focus.view(now_ms())
}

/// Keys the start form lists as refused, before starting.
#[tauri::command]
pub fn focus_preview(list: BlockList, focus: State<'_, Arc<Focus>>) -> Vec<String> {
    focus.preview(&list)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRequest {
    goal: String,
    length_ms: u64,
    strict: bool,
    list: BlockList,
    theme: Option<PageTheme>,
}

#[tauri::command]
pub fn focus_start(
    request: StartRequest,
    app: AppHandle,
    focus: State<'_, Arc<Focus>>,
    prefs: State<'_, Arc<Prefs>>,
    page: State<'_, PagePath>,
) -> Result<View, FocusError> {
    let strictness = if request.strict {
        Strictness::Strict
    } else {
        Strictness::Soft
    };
    let goal = request.goal.trim();
    let goal = if goal.is_empty() { "Focus" } else { goal };
    let view = focus.start(goal, request.length_ms, strictness, &request.list, now_ms())?;
    if let Ok(mut p) = page.lock() {
        *p = write_page(request.theme.as_ref());
    }
    if let Ok(json) = serde_json::to_string(&request.list) {
        let _ = prefs.set(prefs::FOCUS_LAST_LIST, &json);
    }
    emit(&app, Signal::View { view: view.clone() });
    Ok(view)
}

/// One result shape for every control, so the frontend redraws from it.
fn after(app: &AppHandle, result: Result<View, FocusError>) -> Result<View, FocusError> {
    if let Ok(view) = &result {
        emit(app, Signal::View { view: view.clone() });
    }
    result
}

#[tauri::command]
pub fn focus_pause(
    break_minutes: Option<u32>,
    app: AppHandle,
    focus: State<'_, Arc<Focus>>,
) -> Result<View, FocusError> {
    after(
        &app,
        focus.pause(break_minutes.map(|m| u64::from(m) * 60_000), now_ms()),
    )
}

#[tauri::command]
pub fn focus_resume(app: AppHandle, focus: State<'_, Arc<Focus>>) -> Result<View, FocusError> {
    after(&app, focus.resume(now_ms()))
}

#[tauri::command]
pub fn focus_extend(
    minutes: u32,
    app: AppHandle,
    focus: State<'_, Arc<Focus>>,
) -> Result<View, FocusError> {
    after(&app, focus.extend(u64::from(minutes) * 60_000, now_ms()))
}

#[tauri::command]
pub fn focus_complete(app: AppHandle, focus: State<'_, Arc<Focus>>) -> Result<View, FocusError> {
    let result = after(&app, focus.complete(now_ms()));
    if result.is_ok() {
        emit(
            &app,
            Signal::Ended {
                outcome: super::store::Outcome::Completed,
            },
        );
    }
    result
}

#[tauri::command]
pub fn focus_snooze(
    key: String,
    focus: State<'_, Arc<Focus>>,
    prefs: State<'_, Arc<Prefs>>,
) -> Result<(), FocusError> {
    let ms = super::settings::snooze_ms(prefs.get(prefs::FOCUS_SNOOZE).as_deref())
        .ok_or(FocusError::NotAllowed)?;
    focus.snooze(&key, ms, now_ms())
}

#[tauri::command]
pub fn focus_type_goal(
    text: String,
    app: AppHandle,
    focus: State<'_, Arc<Focus>>,
) -> Result<bool, FocusError> {
    let ok = focus.type_goal(&text, now_ms())?;
    emit(
        &app,
        Signal::View {
            view: focus.view(now_ms()),
        },
    );
    Ok(ok)
}

#[tauri::command]
pub fn focus_reset_friction(focus: State<'_, Arc<Focus>>) {
    focus.reset_friction();
}

/// Built-in and custom Focus categories together.
#[derive(Serialize)]
pub struct Categories {
    builtins: Vec<Category>,
    custom: Vec<Category>,
}

#[tauri::command]
pub fn focus_categories(focus: State<'_, Arc<Focus>>) -> Categories {
    Categories {
        builtins: builtins().to_vec(),
        custom: focus.store().custom_categories(),
    }
}

#[tauri::command]
pub fn focus_save_category(
    category: Category,
    focus: State<'_, Arc<Focus>>,
) -> Result<Category, String> {
    if category.name.trim().is_empty() {
        return Err("A Focus category needs a name".into());
    }
    if let Some(problem) = category
        .items
        .iter()
        .find_map(|i| custom_item_error(&i.key))
    {
        return Err(problem);
    }
    focus
        .store()
        .save_category(&category)
        .map_err(|e| e.to_string())
}

/// Custom items may also be `entry:` picks from the app list.
fn custom_item_error(key: &str) -> Option<String> {
    if key.starts_with("entry:") && key.len() > "entry:".len() {
        return None;
    }
    schema_error(key)
}

#[tauri::command]
pub fn focus_delete_category(id: String, focus: State<'_, Arc<Focus>>) -> Result<(), String> {
    focus
        .store()
        .delete_category(&id)
        .map_err(|e| e.to_string())
}

/// Export custom categories as the import format: `{"categories": [...]}`.
#[tauri::command]
pub fn focus_export_categories(ids: Vec<String>, focus: State<'_, Arc<Focus>>) -> String {
    let chosen: Vec<Category> = focus
        .store()
        .custom_categories()
        .into_iter()
        .filter(|c| ids.is_empty() || ids.contains(&c.id))
        .map(|c| Category {
            id: String::new(),
            ..c
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({ "version": 1, "categories": chosen }))
        .unwrap_or_default()
}

/// Import categories from an export. A clashing name gets a suffix, never a merge.
#[tauri::command]
pub fn focus_import_categories(
    json: String,
    focus: State<'_, Arc<Focus>>,
) -> Result<Vec<Category>, String> {
    #[derive(Deserialize)]
    struct File {
        categories: Vec<Category>,
    }
    let file: File =
        serde_json::from_str(&json).map_err(|e| format!("Not a Focus category file: {e}"))?;
    let mut names: Vec<String> = focus
        .store()
        .custom_categories()
        .into_iter()
        .map(|c| c.name)
        .collect();
    let mut saved = Vec::new();
    for mut category in file.categories {
        category.id = String::new();
        category.dynamic = None;
        category
            .items
            .retain(|i| custom_item_error(&i.key).is_none());
        let base = category.name.trim().to_string();
        let mut name = base.clone();
        let mut n = 2;
        while names.contains(&name) {
            name = format!("{base} ({n})");
            n += 1;
        }
        category.name = name.clone();
        names.push(name);
        saved.push(
            focus
                .store()
                .save_category(&category)
                .map_err(|e| e.to_string())?,
        );
    }
    Ok(saved)
}

#[tauri::command]
pub fn focus_last_list(prefs: State<'_, Arc<Prefs>>) -> BlockList {
    prefs
        .get(prefs::FOCUS_LAST_LIST)
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default()
}

/// Focus preferences, typed for the Settings page.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusSettings {
    snooze_minutes: u32,
    restore_apps: bool,
    surface: String,
    bang: bool,
    hurdle_shutdown: String,
    hurdle_idle: String,
}

#[tauri::command]
pub fn focus_settings(prefs: State<'_, Arc<Prefs>>) -> FocusSettings {
    let get = |k: &str| prefs.get(k);
    FocusSettings {
        snooze_minutes: super::settings::snooze_ms(get(prefs::FOCUS_SNOOZE).as_deref())
            .map_or(0, |ms| (ms / 60_000) as u32),
        restore_apps: prefs::flag(&prefs, prefs::FOCUS_RESTORE_APPS, false),
        surface: get(prefs::FOCUS_SURFACE).unwrap_or_else(|| "bar".into()),
        bang: prefs::flag(&prefs, prefs::FOCUS_BANG, false),
        hurdle_shutdown: get(prefs::FOCUS_HURDLE_SHUTDOWN).unwrap_or_else(|| "strict".into()),
        hurdle_idle: get(prefs::FOCUS_HURDLE_IDLE).unwrap_or_else(|| "strict".into()),
    }
}

/// Set one Focus preference. Only Focus keys, only known values.
#[tauri::command]
pub fn focus_set_setting(
    key: String,
    value: String,
    prefs: State<'_, Arc<Prefs>>,
) -> Result<(), String> {
    let ok = match key.as_str() {
        prefs::FOCUS_SNOOZE => matches!(value.as_str(), "0" | "1" | "3" | "5" | "10"),
        prefs::FOCUS_RESTORE_APPS | prefs::FOCUS_BANG => matches!(value.as_str(), "0" | "1"),
        prefs::FOCUS_SURFACE => matches!(value.as_str(), "bar" | "tray"),
        prefs::FOCUS_HURDLE_SHUTDOWN | prefs::FOCUS_HURDLE_IDLE => {
            matches!(value.as_str(), "strict" | "always" | "never")
        }
        _ => false,
    };
    if !ok {
        return Err(format!("{key} cannot be {value:?}"));
    }
    prefs.set(&key, &value).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn focus_clear_history(focus: State<'_, Arc<Focus>>) -> Result<(), String> {
    focus.store().clear_history().map_err(|e| e.to_string())
}
