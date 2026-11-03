//! Focus IPC: every command the Palette, the bar and Settings call, and the
//! one event Rust sends back (`takyon://focus`).
//!
//! The frontend sends local time of day for `until …` durations; Rust holds
//! no timezone logic (`duration.rs`).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use super::categories::{builtins, schema_error, Category, Edit, Item};
use super::duration::{self, DurationError};
use super::icons;
use super::members::{members, Known, Member};
use super::rules::Refusal;
use super::runtime::{now_ms, Signal};
use super::service::{FocusError, View};
use super::session::Strictness;
use super::settings::{themed_page, PageTheme};
use super::store::BlockList;
use super::Focus;
use crate::entry::LaunchTarget;
use crate::prefs::{self, Prefs};
use crate::sources::apps::{AppOrigin, AppSource};

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

/// What the start form lists as refused, before starting. Async: it walks
/// the process table and the registry, never on the main thread.
#[tauri::command(async)]
pub fn focus_preview(list: BlockList, focus: State<'_, Arc<Focus>>) -> Vec<Refusal> {
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
    // A session nothing enforces would read as blocking and block nothing.
    if cfg!(not(windows)) {
        return Err(FocusError::Unsupported);
    }
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
        builtins: focus.builtins(),
        custom: focus.store().custom_categories(),
    }
}

/// What one Focus category holds on this machine, for its Settings page.
#[tauri::command]
pub fn focus_members(
    id: String,
    focus: State<'_, Arc<Focus>>,
    apps: State<'_, Arc<AppSource>>,
) -> Vec<Member> {
    let Some(category) = focus
        .builtins()
        .into_iter()
        .chain(focus.store().custom_categories())
        .find(|c| c.id == id)
    else {
        return Vec::new();
    };
    members(&category, &known(&apps))
}

/// The apps Source's list, reduced for [`members`].
fn known(apps: &AppSource) -> Vec<Known> {
    apps.all()
        .into_iter()
        .map(|a| Known {
            id: a.id.0,
            title: a.title,
            exe: match &a.target {
                LaunchTarget::Exe { path, .. } => {
                    path.file_name().map(|n| n.to_string_lossy().to_lowercase())
                }
                _ => None,
            },
            aumid: match &a.target {
                LaunchTarget::Aumid(aumid) => Some(aumid.clone()),
                _ => None,
            },
            game: a.origin == AppOrigin::Game,
            icon: a.icon.map(|i| i.0),
        })
        .collect()
}

/// Fetch icons for sites the user just added (ADR-0037); cached ones cost
/// nothing. Async: one request per new host. Returns how many arrived.
#[tauri::command(async)]
pub fn focus_fetch_icons(keys: Vec<String>) -> usize {
    let hosts = icons::hosts_of(keys.iter().map(String::as_str));
    crate::identity::data_dir().map_or(0, |dir| icons::fetch_missing(&dir, &hosts))
}

/// Built-in sites' icons, once per machine, off every budget (ADR-0037). An
/// attempt that fetched nothing (offline) tries again next launch.
pub fn fetch_builtin_icons(prefs: &Prefs) {
    if prefs::flag(prefs, prefs::FOCUS_ICONS_FETCHED, false) {
        return;
    }
    let Some(dir) = crate::identity::data_dir() else {
        return;
    };
    let hosts = icons::hosts_of(
        builtins()
            .iter()
            .flat_map(|c| c.items.iter().map(|i| i.key.as_str())),
    );
    let missing = hosts
        .iter()
        .filter(|h| crate::search::favicon::cached(&dir, h).is_none())
        .count();
    if missing == 0 || icons::fetch_missing(&dir, &hosts) > 0 {
        let _ = prefs.set(prefs::FOCUS_ICONS_FETCHED, "1");
    }
}

/// Add to or take out of a built-in Focus category.
#[tauri::command]
pub fn focus_edit_builtin(
    category: String,
    item: Item,
    removed: bool,
    focus: State<'_, Arc<Focus>>,
) -> Result<(), String> {
    if !builtins().iter().any(|c| c.id == category) {
        return Err(format!("No built-in Focus category {category}"));
    }
    if let Some(problem) = custom_item_error(&item.key) {
        return Err(problem);
    }
    focus
        .store()
        .edit_builtin(&Edit {
            category,
            key: item.key,
            label: item.label,
            removed,
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn focus_reset_builtin(category: String, focus: State<'_, Arc<Focus>>) -> Result<(), String> {
    focus
        .store()
        .reset_builtin(&category)
        .map_err(|e| e.to_string())
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
    bar_place: String,
    bar_opacity: u32,
}

/// Default and range of `focus.bar_opacity`, percent.
const BAR_OPACITY: (u32, std::ops::RangeInclusive<u32>) = (90, 30..=100);

fn bar_opacity(value: Option<&str>) -> u32 {
    value
        .and_then(|v| v.parse().ok())
        .filter(|v| BAR_OPACITY.1.contains(v))
        .unwrap_or(BAR_OPACITY.0)
}

#[tauri::command]
pub fn focus_settings(prefs: State<'_, Arc<Prefs>>) -> FocusSettings {
    settings_of(&prefs)
}

fn settings_of(prefs: &Prefs) -> FocusSettings {
    let get = |k: &str| prefs.get(k);
    FocusSettings {
        snooze_minutes: super::settings::snooze_ms(get(prefs::FOCUS_SNOOZE).as_deref())
            .map_or(0, |ms| (ms / 60_000) as u32),
        restore_apps: prefs::flag(prefs, prefs::FOCUS_RESTORE_APPS, false),
        surface: get(prefs::FOCUS_SURFACE).unwrap_or_else(|| "bar".into()),
        bang: prefs::flag(prefs, prefs::FOCUS_BANG, false),
        hurdle_shutdown: get(prefs::FOCUS_HURDLE_SHUTDOWN).unwrap_or_else(|| "strict".into()),
        hurdle_idle: get(prefs::FOCUS_HURDLE_IDLE).unwrap_or_else(|| "strict".into()),
        bar_place: match super::bar::Place::parse(get(prefs::FOCUS_BAR_PLACE).as_deref()) {
            super::bar::Place::Top => "top".into(),
            super::bar::Place::Bottom => "bottom".into(),
        },
        bar_opacity: bar_opacity(get(prefs::FOCUS_BAR_OPACITY).as_deref()),
    }
}

/// The bar's pill measured `width` logical px: size its window to it.
#[tauri::command]
pub fn focus_bar_fit(width: f64, app: AppHandle) {
    super::bar::fit(&app, width);
}

/// Hover on the bar: open the `"up"` or `"down"` dock for its buttons, or
/// close both (None) once they have slid away.
#[tauri::command]
pub fn focus_bar_reveal(side: Option<String>, app: AppHandle) {
    use super::bar::Side;
    let side = match side.as_deref() {
        Some("up") => Some(Side::Up),
        Some("down") => Some(Side::Down),
        _ => None,
    };
    super::bar::reveal(&app, side);
}

/// Set one Focus preference. Only Focus keys, only known values.
#[tauri::command]
pub fn focus_set_setting(
    key: String,
    value: String,
    app: AppHandle,
    prefs: State<'_, Arc<Prefs>>,
    pipeline: State<'_, Arc<crate::query::Pipeline>>,
) -> Result<(), String> {
    let ok = match key.as_str() {
        prefs::FOCUS_SNOOZE => matches!(value.as_str(), "0" | "1" | "3" | "5" | "10"),
        prefs::FOCUS_RESTORE_APPS | prefs::FOCUS_BANG => matches!(value.as_str(), "0" | "1"),
        prefs::FOCUS_SURFACE => matches!(value.as_str(), "bar" | "tray"),
        prefs::FOCUS_HURDLE_SHUTDOWN | prefs::FOCUS_HURDLE_IDLE => {
            matches!(value.as_str(), "strict" | "always" | "never")
        }
        prefs::FOCUS_BAR_PLACE => matches!(value.as_str(), "top" | "bottom"),
        prefs::FOCUS_BAR_OPACITY => value
            .parse::<u32>()
            .is_ok_and(|v| BAR_OPACITY.1.contains(&v)),
        _ => false,
    };
    if !ok {
        return Err(format!("{key} cannot be {value:?}"));
    }
    prefs.set(&key, &value).map_err(|e| e.to_string())?;
    if key == prefs::FOCUS_BANG {
        // Cached on the keystroke path, like every Bang switch.
        pipeline.set_focus_bang(value == "1");
    }
    // A showing bar follows at once; the next one reads the prefs anyway.
    if key == prefs::FOCUS_BAR_PLACE {
        super::bar::replace(&app);
    }
    if key == prefs::FOCUS_BAR_OPACITY {
        emit(
            &app,
            Signal::Bar {
                opacity: bar_opacity(Some(&value)),
            },
        );
    }
    Ok(())
}

#[tauri::command]
pub fn focus_clear_history(focus: State<'_, Arc<Focus>>) -> Result<(), String> {
    focus.store().clear_history().map_err(|e| e.to_string())
}

/// Focus stats over the last `days` local days. `utc_offset_min`: minutes east
/// of UTC, as the frontend's clock says (`-getTimezoneOffset()`).
#[tauri::command]
pub fn focus_stats(
    days: u32,
    utc_offset_min: i32,
    focus: State<'_, Arc<Focus>>,
) -> super::stats::Stats {
    let now = now_ms();
    let since = now.saturating_sub(u64::from(days + 1) * 86_400_000);
    let store = focus.store();
    super::stats::compute(
        &store.history(since),
        &store.events(since),
        now,
        days,
        utc_offset_min,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keys as `interface FocusSettings` in `packages/shared/src/ipc.ts`
    /// declares them, copied by hand: the independent side of the contract.
    #[test]
    fn v0_16_focus_settings_match_ipc_ts() {
        let prefs = Prefs::open(None).unwrap();
        let v = serde_json::to_value(settings_of(&prefs)).unwrap();
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "bang",
                "barOpacity",
                "barPlace",
                "hurdleIdle",
                "hurdleShutdown",
                "restoreApps",
                "snoozeMinutes",
                "surface"
            ]
        );
        assert_eq!(
            v["snoozeMinutes"], 3,
            "default snooze is Raycast's 3 minutes"
        );
        assert_eq!(v["barPlace"], "bottom");
        assert_eq!(v["barOpacity"], 90);
    }

    #[test]
    fn v0_16_bar_opacity_out_of_range_reads_as_the_default() {
        assert_eq!(bar_opacity(Some("55")), 55);
        assert_eq!(bar_opacity(Some("100")), 100);
        assert_eq!(bar_opacity(Some("10")), 90);
        assert_eq!(bar_opacity(Some("x")), 90);
        assert_eq!(bar_opacity(None), 90);
    }
}
