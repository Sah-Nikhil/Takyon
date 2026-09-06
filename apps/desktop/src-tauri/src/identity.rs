//! ADR-0020: identity is `com.v3sper.takyon`, matching the settled display name.
//!
//! Supersedes ADR-0011, which held a neutral slug as insurance against a third
//! rename. Name is settled, so the slug now names the product. Still one constant
//! and one function, because every Windows key must agree on the same string.

use std::path::PathBuf;

/// The MSIX package identity, the single-instance mutex, the `Run` value and the
/// updater feed all key off this. Change it and every one of them needs a
/// migration — see [`migrate_legacy_data_dir`] for the last time that happened.
pub const IDENTITY: &str = "com.v3sper.takyon";

/// Pre-ADR-0020 slug. Kept only so the migration can find what it left behind.
pub const LEGACY_IDENTITY: &str = "com.v3sper.launcher";

/// UI copy and the installer title.
pub const DISPLAY_NAME: &str = "Takyon";

/// `%LOCALAPPDATA%\v3sper\takyon\`.
///
/// Deliberately *not* Tauri's `app_local_data_dir()`, which would hand back
/// `%LOCALAPPDATA%\com.v3sper.takyon\` — correct in spirit, wrong on disk. The
/// layout is `<vendor>\<app>\`, matching how Raycast for Windows lays its own data
/// out and how a second `com.v3sper.*` product would sit beside this one.
pub fn data_dir() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(local).join("v3sper").join("takyon"))
}

/// Pre-ADR-0020 data directory, `%LOCALAPPDATA%\v3sper\launcher\`.
pub fn legacy_data_dir() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(local).join("v3sper").join("launcher"))
}

/// Move the pre-rename data directory across, once, on startup.
///
/// Same parent, so this is a rename and not a copy: atomic, and it cannot leave
/// half a clipboard history behind. Only ever fires when the new directory is
/// absent, so a user who has already migrated keeps what they have.
///
/// Must run before anything opens a database or writes a log, which is why
/// `run()` calls it above `crashlog::install`. Failure is not fatal: the app
/// starts on an empty directory rather than not starting.
pub fn migrate_legacy_data_dir() {
    let (Some(new), Some(old)) = (data_dir(), legacy_data_dir()) else {
        return;
    };
    migrate_dir(&old, &new);
}

/// The move itself, on explicit paths.
///
/// Entry by entry rather than one directory rename, and **not** guarded on "the
/// new directory is absent". Anything that resolves a path through `data_dir()`
/// creates it — a crash log, a scratch directory, the test suite — and a guard
/// that treats an empty stray directory as "already migrated" abandons the real
/// data in place, silently, which is how this was found.
///
/// Whatever is already at the destination wins, so the migration is idempotent
/// and can never overwrite live data with something staler.
///
/// Split out from [`migrate_legacy_data_dir`] so tests drive it on explicit paths:
/// `LOCALAPPDATA` is process-global, and holding it across a filesystem operation
/// raced every other test that resolves a path through it.
fn migrate_dir(old: &std::path::Path, new: &std::path::Path) {
    if !old.exists() {
        return;
    }
    if let Err(e) = std::fs::create_dir_all(new) {
        eprintln!("[takyon] could not create {}: {e}", new.display());
        return;
    }

    let entries = match std::fs::read_dir(old) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("[takyon] could not read {}: {e}", old.display());
            return;
        }
    };
    for entry in entries.flatten() {
        let dest = new.join(entry.file_name());
        if dest.exists() {
            continue;
        }
        if let Err(e) = std::fs::rename(entry.path(), &dest) {
            eprintln!("[takyon] could not migrate {}: {e}", entry.path().display());
        }
    }

    // Only when it emptied. `remove_dir` refusing a non-empty directory is the
    // check, so nothing that failed to move is deleted.
    let _ = std::fs::remove_dir(old);
}

/// Create the data directory if it does not exist, returning it.
///
/// v0.1 stores exactly one thing here (the first-run marker). The SQLite files
/// arrive from v0.3 onward, and `creds\` at v0.5.
pub fn ensure_data_dir() -> std::io::Result<PathBuf> {
    let dir = data_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "LOCALAPPDATA is not set; there is nowhere to put application data",
        )
    })?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The slug is a literal, never built from the display name. Reaching for
    /// `format!("com.v3sper.{}", DISPLAY_NAME.to_lowercase())` happens to produce
    /// the right string today, and would silently rewrite every registry key the
    /// next time the name changes (ADR-0020, superseding ADR-0011).
    #[test]
    fn v0_10_the_slug_is_a_literal_not_derived_from_the_name() {
        assert_eq!(IDENTITY, "com.v3sper.takyon");
        assert_eq!(LEGACY_IDENTITY, "com.v3sper.launcher");
        assert_ne!(IDENTITY, LEGACY_IDENTITY);
    }

    /// The data directory is `<vendor>\<app>`, not a single dotted folder. Getting
    /// this wrong is invisible until someone goes looking for their clipboard
    /// history, or until a migration is needed to move it.
    #[test]
    fn v0_1_data_dir_is_vendor_then_app() {
        // Set rather than read, so the test does not depend on the machine.
        temp_env_localappdata(r"C:\Users\t\AppData\Local", || {
            let dir = data_dir().expect("LOCALAPPDATA was set");
            assert!(dir.ends_with(r"v3sper\takyon"), "got {}", dir.display());
            let old = legacy_data_dir().expect("LOCALAPPDATA was set");
            assert!(old.ends_with(r"v3sper\launcher"), "got {}", old.display());
        });
    }

    #[test]
    fn v0_1_data_dir_is_absent_without_localappdata() {
        temp_env_unset_localappdata(|| assert!(data_dir().is_none()));
    }

    /// The rename is only safe if the migration actually carries the data. A user
    /// upgrading past ADR-0020 loses clipboard history and Frecency if this
    /// regresses, and loses it silently — an empty history looks like a fresh
    /// install, not like a bug.
    #[test]
    fn v0_10_migration_moves_the_legacy_directory_across() {
        let root = scratch_root("migrate-moves");
        let (old, new) = (root.join("launcher"), root.join("takyon"));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("clips.db"), b"rows").unwrap();

        migrate_dir(&old, &new);

        assert_eq!(std::fs::read(new.join("clips.db")).unwrap(), b"rows");
        assert!(!old.exists(), "the legacy directory should be gone, not copied");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Already-migrated users must not be rolled back by a stale legacy directory
    /// that a downgrade or a manual copy left behind. The stale file stays put
    /// rather than being deleted: this migration never destroys anything.
    #[test]
    fn v0_10_migration_never_overwrites_an_existing_file() {
        let root = scratch_root("migrate-keeps");
        let (old, new) = (root.join("launcher"), root.join("takyon"));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(old.join("clips.db"), b"stale").unwrap();
        std::fs::write(new.join("clips.db"), b"current").unwrap();

        migrate_dir(&old, &new);

        assert_eq!(std::fs::read(new.join("clips.db")).unwrap(), b"current");
        assert_eq!(std::fs::read(old.join("clips.db")).unwrap(), b"stale");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The bug this was found by. Anything that resolves a path through
    /// `data_dir()` creates the directory — a crash log, the test suite — so an
    /// empty destination must not read as "already migrated". It did, and the
    /// real clipboard history was abandoned in place with no error anywhere.
    #[test]
    fn v0_10_an_empty_destination_does_not_block_the_migration() {
        let root = scratch_root("migrate-empty-dest");
        let (old, new) = (root.join("launcher"), root.join("takyon"));
        std::fs::create_dir_all(old.join("creds")).unwrap();
        std::fs::create_dir_all(new.join("logs")).unwrap();
        std::fs::write(old.join("clips.db"), b"rows").unwrap();
        std::fs::write(old.join("creds").join("clip.key.dpapi"), b"wrapped").unwrap();

        migrate_dir(&old, &new);

        assert_eq!(std::fs::read(new.join("clips.db")).unwrap(), b"rows");
        assert_eq!(
            std::fs::read(new.join("creds").join("clip.key.dpapi")).unwrap(),
            b"wrapped"
        );
        assert!(new.join("logs").exists(), "what was already there stays");
        assert!(!old.exists(), "an emptied legacy directory is removed");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Running twice must not undo the first run, because the first run can be
    /// interrupted: a file that would not move is retried, everything else is
    /// left alone.
    #[test]
    fn v0_10_migration_is_idempotent() {
        let root = scratch_root("migrate-twice");
        let (old, new) = (root.join("launcher"), root.join("takyon"));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("settings.db"), b"prefs").unwrap();

        migrate_dir(&old, &new);
        migrate_dir(&old, &new);

        assert_eq!(std::fs::read(new.join("settings.db")).unwrap(), b"prefs");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A first-ever install has neither directory. The common path must not fail.
    #[test]
    fn v0_10_migration_is_a_no_op_on_a_fresh_machine() {
        let root = scratch_root("migrate-fresh");
        let (old, new) = (root.join("launcher"), root.join("takyon"));

        migrate_dir(&old, &new);

        assert!(!new.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A private directory per test. Named rather than random: a leaked directory
    /// is then obvious, and re-running the test cleans the previous one up.
    fn scratch_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("takyon-identity-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Every place the slug is written down has to agree, and the two that live
    /// outside Rust are the ones nothing else would catch: `tauri.conf.json`'s
    /// identifier (which is what `tauri-plugin-single-instance` names its mutex
    /// after) and the NSIS uninstall hook (which fails only at uninstall time, on
    /// a machine that no longer has the app to debug it with).
    #[test]
    fn v0_1_config_and_installer_hook_agree_with_the_slug() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

        let conf: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("tauri.conf.json")).unwrap())
                .unwrap();
        assert_eq!(conf["identifier"].as_str(), Some(IDENTITY));
        assert_eq!(conf["productName"].as_str(), Some(DISPLAY_NAME));

        let hook = std::fs::read_to_string(root.join("installer-hooks.nsh")).unwrap();
        assert!(
            hook.contains(IDENTITY),
            "the uninstall hook must delete the Run value named after the slug"
        );
        assert!(
            hook.contains("StartupApproved"),
            "auto-launch writes two values; deleting only the Run value leaves an \
             approval record for an app that is gone"
        );
    }

    /// The display name is written down on the TypeScript side too, for the title
    /// bar v0.6 draws itself. Two copies of a name that is *allowed* to change is
    /// exactly the drift ADR-0011 exists to catch.
    #[test]
    fn v0_6_the_frontend_display_name_agrees() {
        let ipc = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../packages/shared/src/ipc.ts"),
        )
        .expect("packages/shared/src/ipc.ts");
        assert!(
            ipc.contains(&format!(r#"DISPLAY_NAME = "{DISPLAY_NAME}""#)),
            "ipc.ts does not declare DISPLAY_NAME as {DISPLAY_NAME}"
        );
    }

    /// The capability file has to grant `autostart:default`, and the failure mode
    /// if it does not is invisible at build time: `isEnabled()` fails at *runtime*,
    /// in a window nobody opens twice.
    #[test]
    fn v0_1_settings_capability_grants_autostart() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("capabilities/settings.json");
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let perms = json["permissions"].as_array().unwrap();
        assert!(perms.iter().any(|p| p.as_str() == Some("autostart:default")));
    }

    // Environment variables are process-global, so these helpers exist to keep the
    // mutation obviously scoped. Rust runs tests in threads within one process;
    // only these two tests touch LOCALAPPDATA, and they are serialised by the lock.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn temp_env_localappdata(value: &str, f: impl FnOnce()) {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var_os("LOCALAPPDATA");
        unsafe { std::env::set_var("LOCALAPPDATA", value) };
        f();
        match old {
            Some(v) => unsafe { std::env::set_var("LOCALAPPDATA", v) },
            None => unsafe { std::env::remove_var("LOCALAPPDATA") },
        }
    }

    fn temp_env_unset_localappdata(f: impl FnOnce()) {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var_os("LOCALAPPDATA");
        unsafe { std::env::remove_var("LOCALAPPDATA") };
        f();
        if let Some(v) = old {
            unsafe { std::env::set_var("LOCALAPPDATA", v) };
        }
    }
}
