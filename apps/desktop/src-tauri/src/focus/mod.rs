//! Focus sessions (v0.16): block apps and websites for a set time.
//!
//! Plan `docs/plans/v0.16-focus.md`. Websites per ADR-0035, strictness per
//! ADR-0036. Terms from `CONTEXT.md` §Focus.

pub mod bar;
pub mod browser;
pub mod categories;
pub mod duration;
pub mod identity;
pub mod ipc;
pub mod procs;
pub mod protected;
pub mod rules;
pub mod runtime;
pub mod service;
pub mod session;
pub mod settings;
pub mod site;
pub mod store;

use std::path::PathBuf;

pub use service::Focus;

/// This machine's games and install dir, for [`Focus`].
pub struct ThisMachine;

impl service::Machine for ThisMachine {
    /// Walks every Game launcher's library: disk reads, so never on startup.
    fn games(&self) -> Vec<(String, Option<PathBuf>)> {
        crate::sources::apps::games::all()
            .iter()
            .flat_map(|library| library.games())
            .map(|g| {
                (
                    format!("{}:{}", g.launcher.slug(), g.id.to_lowercase()),
                    g.install_dir,
                )
            })
            .collect()
    }

    fn takyon_dir(&self) -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from))
            .unwrap_or_default()
    }
}
