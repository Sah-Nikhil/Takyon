//! The `settings` table in `settings.db` (IMPLEMENTATION_PLAN §4).
//!
//! Opened at v0.5 for one key, `clips.retention`, and it could not wait for
//! v0.6: the retention sweep runs at startup, before any window exists to push a
//! preference in. A default held only in the frontend would mean every launch
//! sweeping with the default and deleting history the user chose to keep.
//!
//! Values are strings, typed by the caller. JSON arrives with v0.6's settings
//! window, which is where a value stops being one word.

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension, Result};

/// How long clipboard history is kept. `store::Retention` spells the values.
pub const CLIPS_RETENTION: &str = "clips.retention";

/// Whether `!v` reaches clipboard history. `"1"` or `"0"`.
///
/// The Bang is a shortcut, not the door: the **Clipboard History** command is
/// always in the Bangless list, so turning this off hides an accelerator rather
/// than a feature. Default on.
pub const CLIPS_BANG: &str = "clips.bang";

/// Whether our own animations are off. `"1"` or `"0"`, default off.
///
/// Independent of Windows' reduce-motion setting, which `styles.css` obeys
/// through a media query either way. Migrated out of `localStorage` at v0.6.
pub const UI_REDUCE_MOTION: &str = "ui.reduce-motion";

/// When the calculator may answer. `sources::calc::Policy` spells the values.
///
/// Read at startup as well as stored: a keystroke can arrive before any window
/// has mounted to push it, and the Bangless path must not go to SQLite.
pub const CALC_POLICY: &str = "calc.policy";

/// The global hotkey, as accelerator text. `hotkey::CHOICES` spells the values.
///
/// Read at startup, before any window exists — the hotkey is registered first and
/// everything else is deferred behind it.
pub const HOTKEY: &str = "hotkey.accelerator";

/// Whether the Recents Source contributes Entries. `"1"` or `"0"`, default on.
pub const RECENTS: &str = "launcher.recents";

/// Whether file Entries join Bangless results. `"1"` or `"0"`, **default off**.
///
/// `!e` is unaffected: the Bang is the door, this is the setting (v0.7 task 11).
pub const FILES_BANGLESS: &str = "files.bangless";

/// Whether Windows Search answers for locations outside the roots. `"1"` or
/// `"0"`, **default off**.
///
/// Off because its coverage cannot be relied on and its queries cost 10–72 ms
/// against a 20 ms budget — TBC-0005's amendment carries the measurement.
pub const FILES_FALLBACK: &str = "files.fallback";

/// Index roots, as a JSON array of paths. Absent means the probed defaults.
pub const FILES_ROOTS: &str = "files.roots";

/// Index exclusions, as a JSON array of names. Absent means the defaults.
pub const FILES_EXCLUDES: &str = "files.excludes";

/// Whether the tray icon is drawn. `"1"` or `"0"`, default on.
pub const TRAY: &str = "launcher.tray";

/// The order `!c` tries Agents in, as a JSON array: `["claude","codex",…]`.
///
/// `!c` asks the first Agent on it that is installed and signed in, so the list
/// is a preference rather than a choice. Read on the keystroke path through a
/// cached copy in `Pipeline`, never SQLite — no room in the 30 ms budget.
pub const ASK_ORDER: &str = "agents.order";

/// The single chosen Agent, before `ASK_ORDER` replaced it.
///
/// Read once at startup to seed the order so an existing install keeps its
/// choice first. Never written.
pub const ASK_AGENT: &str = "agents.default";

/// Where a Turn runs. Absent means the Scratch directory (ADR-0017).
pub const ASK_CWD: &str = "agents.cwd";

/// Whether one Agent is switched on: `agents.enabled.claude`. Default on.
///
/// `!c` skips a switched-off Agent without probing it, which is what lets the
/// Palette name an Agent on the first keystroke rather than after three spawns.
/// Sign-in state cannot do that job: reading it costs a process.
pub fn ask_enabled_key(kind: crate::agents::AgentKind) -> String {
    format!("agents.enabled.{}", kind.as_str())
}

/// The model one Agent must use, keyed by kind: `agents.model.claude`.
///
/// Chosen in Settings and used for **every** Turn — there is no per-query
/// override anywhere. Absent means the Agent's own default, which is the right
/// answer for anyone who has not chosen.
pub fn ask_model_key(kind: crate::agents::AgentKind) -> String {
    format!("agents.model.{}", kind.as_str())
}

/// The effort level one Agent must use: `agents.effort.claude`.
///
/// Locked the same way and for the same reason as the model. Each Agent spells
/// effort differently; the values come from that Agent's own vocabulary.
pub fn ask_effort_key(kind: crate::agents::AgentKind) -> String {
    format!("agents.effort.{}", kind.as_str())
}

/// Where the Palette opens: `"cursor"` (default) or `"primary"`.
pub const PLACEMENT: &str = "launcher.placement";

/// Appearance: `"system"` (default), `"light"` or `"dark"`.
///
/// Was `ui.theme` through v0.9; "theme" means a palette family since ADR-0023,
/// so this key took the name it always described. [`LEGACY_THEME`] is carried
/// across once, and the values are unchanged.
pub const APPEARANCE: &str = "ui.appearance";

/// The pre-v0.10 spelling of [`APPEARANCE`]. Read once by `settings::migrate`.
///
/// A rename without this is not a rename, it is a silent reset: every existing
/// install would find no value under the new key and fall back to `system`,
/// undoing an override the user chose deliberately.
pub const LEGACY_THEME: &str = "ui.theme";

/// Which palette family paints each half (v0.10). Ids from `theme/themes.ts`.
///
/// Both stored regardless of which is live: picking a dark theme in daylight has
/// to work. Rust never interprets the value — the registry is TypeScript's, and
/// an unknown id falls back in the renderer.
pub const THEME_DARK: &str = "ui.theme.dark";
pub const THEME_LIGHT: &str = "ui.theme.light";

/// The family a fresh install gets. Must match `DEFAULT_THEME` in `themes.ts`.
pub const DEFAULT_THEME: &str = "graphite";

/// The Palette's shape: `"compact"` (default) or `"expanded"`.
pub const WINDOW_MODE: &str = "ui.window-mode";

/// Whether tapping the Windows key opens the Palette (v0.10). Default off.
///
/// Not an accelerator: the Windows key is a modifier, so this arms a
/// `WH_KEYBOARD_LL` hook instead (`superkey.rs`). Stored rather than derived
/// because the hook has to be re-armed at every start.
pub const SUPER_HOTKEY: &str = "hotkey.super";

/// Interface size: `"small"`, `"default"` or `"large"`.
///
/// Applied as a root `zoom` in CSS, so every fixed pixel scales together — and
/// Rust scales the Palette's window height by the same factor or the two
/// disagree by exactly the zoom.
pub const UI_SIZE: &str = "ui.size";

/// The zoom a stored interface size means, as a percentage.
///
/// Integer percent rather than a float: it crosses into window arithmetic, where
/// a rounding difference between the two sides shows as a clipped last row.
pub fn ui_scale_percent(value: Option<&str>) -> u32 {
    match value {
        Some("small") => 90,
        Some("large") => 115,
        _ => 100,
    }
}

/// The `settings` table.
pub struct Prefs {
    conn: Mutex<Connection>,
}

impl Prefs {
    /// Open `settings.db` in `dir`, or in memory when there is nowhere to write.
    pub fn open(dir: Option<PathBuf>) -> Result<Self> {
        let conn = match dir {
            Some(dir) => {
                std::fs::create_dir_all(&dir).ok();
                Connection::open(dir.join("settings.db"))?
            }
            None => Connection::open_in_memory()?,
        };
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                 key   TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             )",
            [],
        )?;
        Ok(Prefs {
            conn: Mutex::new(conn),
        })
    }

    /// The stored value, or `None` if nothing has been written.
    pub fn get(&self, key: &str) -> Option<String> {
        let conn = self.conn.lock().ok()?;
        conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get::<_, String>(0)
        })
        .optional()
        .ok()
        .flatten()
    }

    /// Write `value` only if `key` holds nothing. True when it was written.
    ///
    /// One statement rather than a read then a write: two windows migrating at
    /// once would both see the key absent and the second would clobber the first.
    pub fn set_if_absent(&self, key: &str, value: &str) -> Result<bool> {
        let conn = self.conn.lock().expect("prefs mutex");
        let changed = conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO NOTHING",
            params![key, value],
        )?;
        Ok(changed > 0)
    }

    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().expect("prefs mutex");
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, value],
        )?;
        Ok(())
    }

    /// Forget a preference, so whatever reads it falls back to its default.
    ///
    /// **Not the same as writing the default in** (v0.10): the index roots are
    /// probed on every read (TBC-0005), so writing today's answer back turns a
    /// reset into a pin. Removing a key that was never set is not an error.
    pub fn remove(&self, key: &str) -> Result<()> {
        let conn = self.conn.lock().expect("prefs mutex");
        conn.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
        Ok(())
    }
}

/// Read a boolean preference, defaulting when unset or unparseable.
///
/// Unparseable defaults rather than failing: a hand-edited `settings.db` should
/// not be able to turn a feature off by holding a typo.
pub fn flag(prefs: &Prefs, key: &str, default: bool) -> bool {
    match prefs.get(key).as_deref().map(str::trim) {
        Some("1") | Some("true") | Some("on") => true,
        Some("0") | Some("false") | Some("off") => false,
        _ => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_5_a_written_value_reads_back() {
        let p = Prefs::open(None).unwrap();
        assert!(p.get(CLIPS_RETENTION).is_none());
        p.set(CLIPS_RETENTION, "forever").unwrap();
        assert_eq!(p.get(CLIPS_RETENTION).as_deref(), Some("forever"));
        p.set(CLIPS_RETENTION, "1-day").unwrap();
        assert_eq!(p.get(CLIPS_RETENTION).as_deref(), Some("1-day"));
    }

    /// The startup sweep reads this before anything can write it. An absent key
    /// must be absent, never an empty string that parses back to a short window.
    #[test]
    fn v0_5_an_unset_key_is_none_rather_than_empty() {
        let p = Prefs::open(None).unwrap();
        assert_eq!(p.get("nothing.here"), None);
    }

    /// v0.6 migrates the two `localStorage` preferences in. A migration that
    /// overwrote would undo a settings-window choice every time a window holding
    /// a stale key mounted, so a value already stored always wins.
    #[test]
    fn v0_6_a_migration_never_overwrites_a_stored_value() {
        let p = Prefs::open(None).unwrap();

        assert!(p.set_if_absent(UI_REDUCE_MOTION, "1").unwrap());
        assert_eq!(p.get(UI_REDUCE_MOTION).as_deref(), Some("1"));

        // A second migration carrying the other legacy value. Ignored.
        assert!(!p.set_if_absent(UI_REDUCE_MOTION, "0").unwrap());
        assert_eq!(p.get(UI_REDUCE_MOTION).as_deref(), Some("1"));
    }

    /// A value the settings window wrote must survive migration too, which is the
    /// same rule reached from the other direction: `set` then migrate.
    #[test]
    fn v0_6_a_written_value_survives_a_later_migration() {
        let p = Prefs::open(None).unwrap();
        p.set(CALC_POLICY, "explicit").unwrap();
        assert!(!p.set_if_absent(CALC_POLICY, "automatic").unwrap());
        assert_eq!(p.get(CALC_POLICY).as_deref(), Some("explicit"));
    }

    /// The zoom table, and that anything unrecognised is 100%.
    ///
    /// It lands in window arithmetic, so a stray value must mean "no scaling"
    /// rather than a height nobody can explain.
    #[test]
    fn v0_6_interface_size_maps_to_a_whole_percentage() {
        assert_eq!(ui_scale_percent(None), 100);
        assert_eq!(ui_scale_percent(Some("default")), 100);
        assert_eq!(ui_scale_percent(Some("small")), 90);
        assert_eq!(ui_scale_percent(Some("large")), 115);
        assert_eq!(ui_scale_percent(Some("enormous")), 100);
    }

    #[test]
    fn v0_5_a_flag_defaults_when_unset_or_unreadable() {
        let p = Prefs::open(None).unwrap();
        assert!(flag(&p, CLIPS_BANG, true));
        assert!(!flag(&p, CLIPS_BANG, false));

        p.set(CLIPS_BANG, "0").unwrap();
        assert!(!flag(&p, CLIPS_BANG, true));
        p.set(CLIPS_BANG, "1").unwrap();
        assert!(flag(&p, CLIPS_BANG, false));

        // A typo must not silently turn something off.
        p.set(CLIPS_BANG, "yes please").unwrap();
        assert!(flag(&p, CLIPS_BANG, true));
    }
}
