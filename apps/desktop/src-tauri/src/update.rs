//! In-app update through `tauri-plugin-updater` (ADR-0040).
//!
//! Network from three places only: About's button, Enter on the Palette's update
//! row, and one background check a day (on by default, switch in About). None of
//! them is on the query path (ADR-0002). The Palette reads [`notice`], a value in
//! memory, so a summon never waits on disk or network.
//!
//! A build whose config has no pubkey or feed (every local and bench build;
//! `release.yml` adds both) answers `Unconfigured` and never schedules a check.
//! The plugin verifies the installer's minisign signature before running it.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::prefs::{flag, Prefs};

/// Settings key for the daily background check: `"1"`/`"0"`, default on.
pub const AUTO_CHECK: &str = "update.auto_check";
/// Epoch ms of the last check that reached the feed.
const LAST_CHECK: &str = "update.last_check_ms";
/// Newest version a check found, kept so the Palette row survives a restart.
const AVAILABLE: &str = "update.available";
/// Version that ran last; a newer one at launch means an update just landed.
const LAST_RUN: &str = "update.last_run";
/// Version whose "See what's new" row has not been opened yet.
const WHATS_NEW: &str = "update.whats_new";

/// One feed request a day at most, however often Takyon starts.
pub const CHECK_EVERY_MS: u64 = 24 * 60 * 60 * 1000;
/// Clear of the 500 ms login budget and of everything else deferred init starts.
const FIRST_DELAY: Duration = Duration::from_secs(90);
/// How often the scheduler wakes to ask whether a day has passed.
const POLL: Duration = Duration::from_secs(60 * 60);

/// What a check found. `interface UpdateCheck` in `packages/shared/src/ipc.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum UpdateCheck {
    /// This build carries no feed or key, so it cannot update itself.
    Unconfigured,
    Current {
        version: String,
    },
    Available {
        version: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        notes: Option<String>,
    },
}

/// Install progress, sent on the command's channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
pub enum UpdateProgress {
    Downloading {
        downloaded: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        total: Option<u64>,
    },
    /// Signature checked, installer starting. Windows exits right after.
    Installing,
}

/// The Palette's update row. `type UpdateNotice` in `ipc.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Notice {
    /// A newer version is on the feed; Enter installs it.
    Available { version: String },
    /// This version was just installed; Enter opens its release page.
    WhatsNew { version: String, url: String },
}

/// The update the last check found, so Install never makes a second request.
#[derive(Default)]
pub struct Pending(Mutex<Option<Update>>);

impl Pending {
    fn put(&self, update: Option<Update>) {
        *self.0.lock().unwrap_or_else(|p| p.into_inner()) = update;
    }

    fn take(&self) -> Option<Update> {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).take()
    }

    fn is_empty(&self) -> bool {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).is_none()
    }
}

/// In memory, not `settings.db`: `window::show` reads it on every summon.
static NOTICE: Mutex<Option<Notice>> = Mutex::new(None);

/// The Palette's update row, if any. Never touches disk or network.
pub fn notice() -> Option<Notice> {
    NOTICE.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// Whether `plugins.updater` names a key and at least one feed.
pub fn configured(updater: Option<&Value>) -> bool {
    let Some(updater) = updater else {
        return false;
    };
    let key = updater
        .get("pubkey")
        .and_then(Value::as_str)
        .is_some_and(|k| !k.trim().is_empty());
    let feed = updater
        .get("endpoints")
        .and_then(Value::as_array)
        .is_some_and(|e| !e.is_empty());
    key && feed
}

/// Whether a day has passed since `last`. A clock set backwards counts as due.
pub fn due(now_ms: u64, last: Option<u64>) -> bool {
    last.map_or(true, |last| {
        now_ms < last || now_ms - last >= CHECK_EVERY_MS
    })
}

/// SemVer `candidate > current`. Unparseable is never newer.
pub fn newer(candidate: &str, current: &str) -> bool {
    match (
        semver::Version::parse(candidate),
        semver::Version::parse(current),
    ) {
        (Ok(c), Ok(cur)) => c > cur,
        _ => false,
    }
}

/// The version to announce at launch: after an upgrade only, never on a first
/// install, which has no `last_run` to compare against.
pub fn whats_new_after(current: &str, last_run: Option<&str>) -> Option<String> {
    last_run
        .filter(|last| newer(current, last))
        .map(|_| current.to_string())
}

/// `https://github.com/o/r/releases/latest/download/latest.json` -> the tag page.
pub fn release_page(feed: &str, version: &str) -> Option<String> {
    let base = feed.strip_suffix("/releases/latest/download/latest.json")?;
    base.starts_with("https://github.com/")
        .then(|| format!("{base}/releases/tag/v{version}"))
}

/// Which row the Palette shows. An install waiting beats notes already read.
pub fn pick_notice(
    current: &str,
    available: Option<&str>,
    whats_new: Option<&str>,
    feed: Option<&str>,
) -> Option<Notice> {
    if let Some(version) = available.filter(|v| newer(v, current)) {
        return Some(Notice::Available {
            version: version.to_string(),
        });
    }
    let version = whats_new.filter(|v| *v == current)?;
    Some(Notice::WhatsNew {
        version: version.to_string(),
        url: release_page(feed?, version)?,
    })
}

/// Plugin errors in words for the About page. The raw error goes to the log.
pub fn describe(error: &tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error as E;
    eprintln!("[takyon] update: {error}");
    match error {
        E::Reqwest(_) | E::Network(_) | E::ReleaseNotFound => {
            "Couldn't reach the update feed. Check the connection and try again.".into()
        }
        E::Minisign(_) | E::Base64(_) | E::SignatureUtf8(_) => {
            "The download failed its signature check. Nothing was installed.".into()
        }
        E::TargetNotFound(_) | E::TargetsNotFound(_) => {
            "The latest release has no installer for this system.".into()
        }
        other => format!("Update failed: {other}"),
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn feed<R: Runtime>(app: &AppHandle<R>) -> Option<String> {
    let updater = app.config().plugins.0.get("updater")?;
    let first = updater.get("endpoints")?.as_array()?.first()?;
    first.as_str().map(str::to_string)
}

fn prefs<R: Runtime>(app: &AppHandle<R>) -> Option<Arc<Prefs>> {
    app.try_state::<Arc<Prefs>>().map(|p| p.inner().clone())
}

/// Recompute the Palette row from what `settings.db` holds.
fn refresh_notice<R: Runtime>(app: &AppHandle<R>) {
    let Some(prefs) = prefs(app) else { return };
    let next = pick_notice(
        &app.package_info().version.to_string(),
        prefs.get(AVAILABLE).as_deref(),
        prefs.get(WHATS_NEW).as_deref(),
        feed(app).as_deref(),
    );
    *NOTICE.lock().unwrap_or_else(|p| p.into_inner()) = next;
}

/// Store what a check found, whoever asked, and update the Palette row.
fn record<R: Runtime>(app: &AppHandle<R>, found: Option<Update>) {
    if let Some(prefs) = prefs(app) {
        let _ = prefs.set(LAST_CHECK, &now_ms().to_string());
        let _ = match &found {
            Some(update) => prefs.set(AVAILABLE, &update.version),
            None => prefs.remove(AVAILABLE),
        };
    }
    if let Some(pending) = app.try_state::<Pending>() {
        pending.put(found);
    }
    refresh_notice(app);
}

/// Launch bookkeeping, then the daily check, on a thread of its own.
///
/// Off the startup path: two `settings.db` reads, then a sleep. The check runs
/// on this thread, never rayon's pool (the `search::fetch::pool` rule).
pub fn start<R: Runtime>(app: AppHandle<R>) {
    std::thread::spawn(move || {
        let Some(prefs) = prefs(&app) else { return };
        let current = app.package_info().version.to_string();
        let last_run = prefs.get(LAST_RUN);
        if let Some(version) = whats_new_after(&current, last_run.as_deref()) {
            let _ = prefs.set(WHATS_NEW, &version);
        }
        if last_run.as_deref() != Some(current.as_str()) {
            let _ = prefs.set(LAST_RUN, &current);
        }
        refresh_notice(&app);

        if !configured(app.config().plugins.0.get("updater")) {
            return;
        }
        std::thread::sleep(FIRST_DELAY);
        loop {
            let last = prefs.get(LAST_CHECK).and_then(|v| v.parse().ok());
            if flag(&prefs, AUTO_CHECK, true) && due(now_ms(), last) {
                let checked =
                    tauri::async_runtime::block_on(async { app.updater()?.check().await });
                match checked {
                    Ok(found) => record(&app, found),
                    Err(e) => {
                        describe(&e);
                    }
                }
            }
            std::thread::sleep(POLL);
        }
    });
}

async fn check_now<R: Runtime>(
    app: &AppHandle<R>,
    pending: &Pending,
) -> Result<UpdateCheck, String> {
    pending.put(None);
    if !configured(app.config().plugins.0.get("updater")) {
        return Ok(UpdateCheck::Unconfigured);
    }
    let found = app
        .updater()
        .map_err(|e| describe(&e))?
        .check()
        .await
        .map_err(|e| describe(&e))?;
    let answer = match &found {
        None => UpdateCheck::Current {
            version: app.package_info().version.to_string(),
        },
        Some(update) => UpdateCheck::Available {
            version: update.version.clone(),
            notes: update.body.clone().filter(|n| !n.trim().is_empty()),
        },
    };
    record(app, found);
    Ok(answer)
}

async fn install_pending<R: Runtime>(
    app: &AppHandle<R>,
    pending: &Pending,
    on_progress: Channel<UpdateProgress>,
) -> Result<(), String> {
    let update = pending
        .take()
        .ok_or_else(|| "Check for updates first.".to_string())?;
    let mut downloaded = 0u64;
    let result = update
        .download_and_install(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = on_progress.send(UpdateProgress::Downloading { downloaded, total });
            },
            || {
                let _ = on_progress.send(UpdateProgress::Installing);
            },
        )
        .await;
    match result {
        Ok(()) => app.restart(),
        Err(e) => {
            // Back in place so Install can retry without another check.
            pending.put(Some(update));
            Err(describe(&e))
        }
    }
}

#[tauri::command]
pub async fn update_check<R: Runtime>(
    app: AppHandle<R>,
    pending: State<'_, Pending>,
) -> Result<UpdateCheck, String> {
    check_now(&app, &pending).await
}

/// Download, verify, install. On Windows the plugin runs the installer and
/// exits the process, so success never returns there.
#[tauri::command]
pub async fn update_install<R: Runtime>(
    app: AppHandle<R>,
    pending: State<'_, Pending>,
    on_progress: Channel<UpdateProgress>,
) -> Result<(), String> {
    install_pending(&app, &pending, on_progress).await
}

/// Enter on the Palette's update row. A row left by an earlier run has no
/// `Update` in memory, so this asks the feed again first.
#[tauri::command]
pub async fn update_install_now<R: Runtime>(
    app: AppHandle<R>,
    pending: State<'_, Pending>,
    on_progress: Channel<UpdateProgress>,
) -> Result<(), String> {
    if pending.is_empty() {
        match check_now(&app, &pending).await? {
            UpdateCheck::Available { .. } => {}
            UpdateCheck::Unconfigured => return Err("This build can't update itself.".into()),
            UpdateCheck::Current { .. } => return Err("Takyon is already up to date.".into()),
        }
    }
    install_pending(&app, &pending, on_progress).await
}

/// The Palette's update row, for a mount that missed the show event.
#[tauri::command]
pub fn update_notice() -> Option<Notice> {
    notice()
}

/// "See what's new" was opened; the row goes until the next update.
#[tauri::command]
pub fn update_notice_dismiss<R: Runtime>(app: AppHandle<R>) {
    if let Some(prefs) = prefs(&app) {
        let _ = prefs.remove(WHATS_NEW);
    }
    refresh_notice(&app);
}

#[tauri::command]
pub fn update_auto_check<R: Runtime>(app: AppHandle<R>) -> bool {
    prefs(&app).map_or(true, |p| flag(&p, AUTO_CHECK, true))
}

#[tauri::command]
pub fn set_update_auto_check<R: Runtime>(app: AppHandle<R>, on: bool) -> Result<(), String> {
    let prefs = prefs(&app).ok_or("Settings are not available.")?;
    prefs
        .set(AUTO_CHECK, if on { "1" } else { "0" })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// What `release.yml`'s overlay adds. Anything less is a build that cannot update.
    #[test]
    fn updates_need_both_a_key_and_a_feed() {
        let feed = "https://github.com/acme/takyon/releases/latest/download/latest.json";
        assert!(configured(Some(
            &json!({ "pubkey": "RWQ...", "endpoints": [feed] })
        )));
        assert!(!configured(None));
        // The committed tauri.conf.json: the plugin needs the block to exist.
        assert!(!configured(Some(&json!({ "pubkey": "", "endpoints": [] }))));
        assert!(!configured(Some(
            &json!({ "pubkey": "  ", "endpoints": [feed] })
        )));
        assert!(!configured(Some(
            &json!({ "pubkey": "RWQ...", "endpoints": [] })
        )));
    }

    /// Field names and tags as `ipc.ts` spells them; optionals absent, not null.
    #[test]
    fn a_check_serialises_as_ipc_ts_declares() {
        let v = |c: UpdateCheck| serde_json::to_value(c).unwrap();
        assert_eq!(
            v(UpdateCheck::Unconfigured),
            json!({ "state": "unconfigured" })
        );
        assert_eq!(
            v(UpdateCheck::Current {
                version: "1.2.3".into()
            }),
            json!({ "state": "current", "version": "1.2.3" })
        );
        assert_eq!(
            v(UpdateCheck::Available {
                version: "1.3.0".into(),
                notes: Some("Fixed a thing.".into())
            }),
            json!({ "state": "available", "version": "1.3.0", "notes": "Fixed a thing." })
        );
    }

    #[test]
    fn progress_serialises_as_ipc_ts_declares() {
        let v = |p: UpdateProgress| serde_json::to_value(p).unwrap();
        assert_eq!(
            v(UpdateProgress::Downloading {
                downloaded: 10,
                total: Some(40)
            }),
            json!({ "phase": "downloading", "downloaded": 10, "total": 40 })
        );
        assert_eq!(
            v(UpdateProgress::Downloading {
                downloaded: 10,
                total: None
            }),
            json!({ "phase": "downloading", "downloaded": 10 })
        );
        assert_eq!(
            v(UpdateProgress::Installing),
            json!({ "phase": "installing" })
        );
    }

    /// A bad signature must say nothing ran, not read like a network blip.
    #[test]
    fn a_signature_failure_says_nothing_was_installed() {
        use tauri_plugin_updater::Error as E;
        assert!(describe(&E::SignatureUtf8("x".into())).contains("Nothing was installed"));
        assert!(describe(&E::ReleaseNotFound).contains("Couldn't reach"));
        assert!(describe(&E::Network("404".into())).contains("Couldn't reach"));
    }

    /// One request a day; a clock moved backwards is no reason to wait.
    #[test]
    fn a_check_is_due_once_a_day() {
        let day = CHECK_EVERY_MS;
        assert!(due(1_000, None));
        assert!(!due(10 * day, Some(10 * day - 1)));
        assert!(!due(11 * day - 1, Some(10 * day)));
        assert!(due(11 * day, Some(10 * day)));
        assert!(due(5, Some(10 * day)));
    }

    #[test]
    fn newer_compares_semver_not_strings() {
        assert!(newer("0.12.10", "0.12.9"));
        assert!(!newer("0.12.9", "0.12.10"));
        assert!(!newer("0.12.4", "0.12.4"));
        assert!(!newer("nonsense", "0.12.4"));
    }

    /// An upgrade announces itself; a first install has nothing to compare.
    #[test]
    fn whats_new_follows_an_upgrade_only() {
        assert_eq!(
            whats_new_after("0.12.5", Some("0.12.4")),
            Some("0.12.5".into())
        );
        assert_eq!(whats_new_after("0.12.5", None), None);
        assert_eq!(whats_new_after("0.12.5", Some("0.12.5")), None);
        assert_eq!(whats_new_after("0.12.4", Some("0.12.5")), None);
    }

    #[test]
    fn the_release_page_comes_from_the_feed() {
        let feed = "https://github.com/acme/takyon/releases/latest/download/latest.json";
        assert_eq!(
            release_page(feed, "1.2.3").as_deref(),
            Some("https://github.com/acme/takyon/releases/tag/v1.2.3")
        );
        assert_eq!(
            release_page("https://example.com/latest.json", "1.2.3"),
            None
        );
        assert_eq!(
            release_page(
                "http://github.com/a/b/releases/latest/download/latest.json",
                "1"
            ),
            None
        );
    }

    #[test]
    fn an_install_waiting_wins_over_notes_already_installed() {
        let feed = Some("https://github.com/acme/takyon/releases/latest/download/latest.json");
        assert_eq!(
            pick_notice("1.0.0", Some("1.1.0"), Some("1.0.0"), feed),
            Some(Notice::Available {
                version: "1.1.0".into()
            })
        );
        assert_eq!(
            pick_notice("1.0.0", None, Some("1.0.0"), feed),
            Some(Notice::WhatsNew {
                version: "1.0.0".into(),
                url: "https://github.com/acme/takyon/releases/tag/v1.0.0".into()
            })
        );
        // Installed since the check, by hand or from the row: nothing to offer.
        assert_eq!(pick_notice("1.1.0", Some("1.1.0"), None, feed), None);
        // Notes for another version, or no feed to link to: no row.
        assert_eq!(pick_notice("1.0.0", None, Some("0.9.0"), feed), None);
        assert_eq!(pick_notice("1.0.0", None, Some("1.0.0"), None), None);
    }

    #[test]
    fn a_notice_serialises_as_ipc_ts_declares() {
        let v = |n: Notice| serde_json::to_value(n).unwrap();
        assert_eq!(
            v(Notice::Available {
                version: "1.1.0".into()
            }),
            json!({ "kind": "available", "version": "1.1.0" })
        );
        assert_eq!(
            v(Notice::WhatsNew {
                version: "1.0.0".into(),
                url: "https://x".into()
            }),
            json!({ "kind": "whatsNew", "version": "1.0.0", "url": "https://x" })
        );
    }
}
