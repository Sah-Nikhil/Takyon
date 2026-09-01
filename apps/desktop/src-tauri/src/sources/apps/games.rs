//! Game launchers: one shape, one URI, and a different catalogue format each.
//!
//! Every launcher answers the same two questions — which games are installed, and
//! what id starts each — and every one invented its own way to answer them: Steam
//! a VDF text tree, Epic a directory of JSON manifests, GOG a SQLite database,
//! Battle.net a protobuf. The trait is where the sharing stops, and the bodies are
//! not meant to share anything.
//!
//! Xbox and Game Pass need nothing here. Those titles install as MSIX packages
//! with AUMIDs, so `appsfolder.rs` already lists them.

pub mod epic;
pub mod steam;

use crate::entry::GameLauncher;

/// One installed game, whichever launcher owns it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub launcher: GameLauncher,
    /// The launcher's own id — Steam app id, Epic `AppName`. Half the EntryId, so
    /// it must survive the game moving drive; a path never can.
    pub id: String,
    pub name: String,
}

/// One launcher's installed library.
///
/// Only constructed when the launcher is present, so `games` never answers "is
/// this installed" — an absent launcher is an absent implementor.
pub trait GameLibrary {
    fn launcher(&self) -> GameLauncher;
    fn games(&self) -> Vec<Game>;
}

/// Every launcher installed on this machine.
///
/// Adding GOG or EA is a module here plus a line below. Nothing in `apps.rs`
/// changes, which is the whole point of the trait.
pub fn all() -> Vec<Box<dyn GameLibrary>> {
    let mut libraries: Vec<Box<dyn GameLibrary>> = Vec::new();
    if let Some(steam) = steam::SteamLibrary::detect() {
        libraries.push(Box::new(steam));
    }
    if let Some(epic) = epic::EpicLibrary::detect() {
        libraries.push(Box::new(epic));
    }
    libraries
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A library reports the launcher its games carry, or `apps.rs` would build
    /// an EntryId under the wrong namespace and split that game's Frecency.
    #[test]
    fn v0_3_every_library_agrees_with_the_games_it_yields() {
        for library in all() {
            for game in library.games() {
                assert_eq!(
                    game.launcher,
                    library.launcher(),
                    "{} yielded a {:?} game",
                    library.launcher().slug(),
                    game.launcher
                );
                assert!(!game.id.is_empty(), "{} has no id", game.name);
                assert!(!game.name.is_empty(), "{} has no name", game.id);
            }
        }
    }
}
