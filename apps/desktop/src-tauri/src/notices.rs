//! About's third-party licence list: `THIRD_PARTY_NOTICES.json`, read on demand.
//!
//! `scripts/third-party-notices.ts` writes it beside the notices text before
//! every `tauri build`, and `tauri.bundle.conf.json` ships both as resources.
//! Read only when the licence view opens, never at startup or on the Palette
//! path. Rust checks the file is JSON and passes it through unread: the shape
//! belongs to the generator and `ThirdPartyLicences` in `packages/shared`.

use std::path::{Path, PathBuf};

use tauri::{ipc::Response, AppHandle, Manager, Runtime};

pub const FILE: &str = "THIRD_PARTY_NOTICES.json";

/// Installed: the resource directory. Debug: also the generator's output
/// beside `Cargo.toml`, so `bun run dev` shows it after `bun run notices`.
fn candidates<R: Runtime>(app: &AppHandle<R>) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = app
        .path()
        .resource_dir()
        .ok()
        .map(|d| d.join(FILE))
        .into_iter()
        .collect();
    if cfg!(debug_assertions) {
        paths.push(Path::new(env!("CARGO_MANIFEST_DIR")).join(FILE));
    }
    paths
}

/// The first readable candidate, checked as JSON. Errors are user-facing copy.
pub fn read(paths: &[PathBuf]) -> Result<String, String> {
    let path = paths.iter().find(|p| p.is_file()).ok_or_else(|| {
        if cfg!(debug_assertions) {
            format!("{FILE} is missing. Run `bun run notices` to generate it.")
        } else {
            "The licence list is missing from this install. Reinstalling Takyon restores it."
                .to_string()
        }
    })?;
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("Could not read the licence list: {e}"))?;
    serde_json::from_str::<serde::de::IgnoredAny>(&text)
        .map_err(|e| format!("The licence list is damaged: {e}"))?;
    Ok(text)
}

/// Raw JSON body: the webview gets the parsed object, Rust never re-encodes it.
#[tauri::command]
pub async fn third_party_licences<R: Runtime>(app: AppHandle<R>) -> Result<Response, String> {
    let paths = candidates(&app);
    tauri::async_runtime::spawn_blocking(move || read(&paths))
        .await
        .map_err(|e| e.to_string())?
        .map(Response::new)
}
