//! ADR-0011: what Windows keys off is a fixed neutral slug; "Takyon" is display
//! copy and may change freely.
//!
//! The separation only survives if it is enforced somewhere, because the natural
//! thing for every future contributor to do is derive one from the other. This
//! module is that enforcement point, and its tests are the thing that will catch
//! a third rename leaking into the registry.

use std::path::PathBuf;

/// The MSIX package identity, the single-instance mutex, the `Run` value and the
/// updater feed all key off this. Never derive it from [`DISPLAY_NAME`].
pub const IDENTITY: &str = "com.v3sper.launcher";

/// UI copy and the installer title. This is the string that is allowed to change.
pub const DISPLAY_NAME: &str = "Takyon";

/// `%LOCALAPPDATA%\v3sper\launcher\`.
///
/// Deliberately *not* Tauri's `app_local_data_dir()`, which would hand back
/// `%LOCALAPPDATA%\com.v3sper.launcher\` — correct in spirit, wrong on disk. The
/// layout in ADR-0011 is `<vendor>\<app>\`, matching how Raycast for Windows lays
/// its own data out and how a second `com.v3sper.*` product would sit beside this
/// one rather than under a sibling directory with a dotted name.
pub fn data_dir() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(local).join("v3sper").join("launcher"))
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

    /// The whole ADR in one assertion: if someone renames the product and reaches
    /// for the obvious `format!("com.v3sper.{}", DISPLAY_NAME.to_lowercase())`,
    /// this fails. Without it the decoupling is a comment, not a constraint.
    #[test]
    fn v0_1_the_slug_does_not_contain_the_display_name() {
        assert!(
            !IDENTITY.to_lowercase().contains(&DISPLAY_NAME.to_lowercase()),
            "the identity slug must stay independent of the display name (ADR-0011)"
        );
    }

    /// The data directory is `<vendor>\<app>`, not a single dotted folder. Getting
    /// this wrong is invisible until someone goes looking for their clipboard
    /// history, or until a migration is needed to move it.
    #[test]
    fn v0_1_data_dir_is_vendor_then_app() {
        // Set rather than read, so the test does not depend on the machine.
        temp_env_localappdata(r"C:\Users\t\AppData\Local", || {
            let dir = data_dir().expect("LOCALAPPDATA was set");
            assert!(dir.ends_with(r"v3sper\launcher"), "got {}", dir.display());
        });
    }

    #[test]
    fn v0_1_data_dir_is_absent_without_localappdata() {
        temp_env_unset_localappdata(|| assert!(data_dir().is_none()));
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
