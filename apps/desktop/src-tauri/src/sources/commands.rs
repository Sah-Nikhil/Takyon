//! Built-in commands: things Takyon itself does, found by typing their name.
//!
//! Raycast's model, and the reason clipboard history is reachable at all without
//! knowing a Bang exists. A Command is a destination like an application — you
//! ask for it by name — so it shares the App rank tier rather than sitting under
//! one (`EntryKind::Command`).
//!
//! **A Command is not a Clip.** ADR-0006 keeps clipboard *content* out of every
//! Bangless list; a row that merely opens the history holds no content and leaks
//! nothing. That distinction is the whole reason this Source can exist.

use std::sync::Arc;
use std::time::Duration;

use crate::entry::{
    Action, Entry, EntryId, EntryKind, Query, Source, SourceId, SOURCE_SHORTLIST,
};
use crate::rank::{self, Haystack};

/// EntryId namespace. Frozen: it is the Frecency key.
pub const PREFIX: &str = "command:";

/// One built-in command.
pub struct Command {
    pub id: EntryId,
    pub title: &'static str,
    /// Shown after the title, the way Raycast shows the extension a command came
    /// from. Ours all come from Takyon, so it names the product.
    pub subtitle: &'static str,
    hay: Haystack,
}

/// Which command an Entry is, for activation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandId {
    ClipboardHistory,
}

impl CommandId {
    /// The stable id slug. Never change one: Frecency is keyed on it.
    pub fn slug(self) -> &'static str {
        match self {
            CommandId::ClipboardHistory => "clipboard-history",
        }
    }

    pub fn entry_id(self) -> EntryId {
        EntryId(format!("{PREFIX}{}", self.slug()))
    }

    /// Parse an EntryId back, or `None` if it is not a command.
    pub fn from_entry(id: &EntryId) -> Option<Self> {
        match id.as_str().strip_prefix(PREFIX)? {
            "clipboard-history" => Some(CommandId::ClipboardHistory),
            _ => None,
        }
    }
}

/// The table. One command at v0.5; v0.6 and beyond add rows, not code.
const TABLE: &[(CommandId, &str, &str, &[&str])] = &[(
    CommandId::ClipboardHistory,
    "Clipboard History",
    "Takyon",
    &["clipboard", "history", "clip", "hist", "paste", "copied"],
)];

/// Built-in commands as a Source.
pub struct CommandSource {
    commands: Vec<Command>,
}

impl Default for CommandSource {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandSource {
    pub fn new() -> Self {
        let commands = TABLE
            .iter()
            .map(|(id, title, subtitle, keywords)| {
                // The shipped ladder, not a second one. `keywords` lands on
                // `TIER_KEYWORD`, below any name a user can see — the `disk` /
                // Disk Cleanup lesson from v0.3 (`docs/tbd/v0.3.md` §10).
                let mut hay = Haystack::new(title, None);
                hay.keywords = keywords.iter().map(|k| k.to_string()).collect();
                Command {
                    id: id.entry_id(),
                    title,
                    subtitle,
                    hay,
                }
            })
            .collect();
        CommandSource { commands }
    }

    /// Look one up by id, for activation and the action menu.
    pub fn find(&self, id: &EntryId) -> Option<&Command> {
        self.commands.iter().find(|c| &c.id == id)
    }
}

impl Source for CommandSource {
    fn id(&self) -> SourceId {
        SourceId("commands")
    }

    fn query(&self, q: &Query, _budget: Duration) -> Vec<Entry> {
        let mut out: Vec<Entry> = Vec::new();
        for command in &self.commands {
            let Some(score) = rank::score(q, &command.hay) else {
                continue;
            };
            out.push(Entry {
                id: command.id.clone(),
                title: command.title.to_string(),
                subtitle: Some(command.subtitle.to_string()),
                kind: EntryKind::Command,
                icon: None,
                score,
                actions: crate::actions::for_command(),
                version: None,
            });
        }
        out.truncate(SOURCE_SHORTLIST);
        out
    }

    fn actions(&self, entry: &Entry) -> Vec<Action> {
        crate::actions::for_entry(entry)
    }
}

/// Shared so `lib.rs` and the tests build it the same way.
pub fn shared() -> Arc<CommandSource> {
    Arc::new(CommandSource::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hits(needle: &str) -> Vec<Entry> {
        CommandSource::new().query(&Query::new(needle), Duration::from_millis(20))
    }

    /// The screenshot's query. `hist` is a keyword rather than a prefix of the
    /// title, and it still has to find the command.
    #[test]
    fn v0_5_hist_finds_clipboard_history() {
        let found = hits("hist");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "Clipboard History");
        assert_eq!(found[0].kind, EntryKind::Command);
    }

    #[test]
    fn v0_5_the_command_is_found_by_its_own_name_too() {
        for needle in ["clip", "clipboard", "clipboard his", "Clipboard History"] {
            assert_eq!(hits(needle).len(), 1, "{needle} found nothing");
        }
    }

    /// A shipped keyword must never outrank a name the user can see — the `disk`
    /// / Disk Cleanup lesson from v0.3, which this Source could reintroduce.
    ///
    /// Asserted on the rung, not on two of our own scores: "clipboard" is *both*
    /// a keyword and the title's first word, so comparing them measured nothing.
    #[test]
    fn v0_5_a_keyword_match_scores_below_a_name_match() {
        // The ladder's own ordering is a `const` assertion in `rank.rs`. What is
        // worth checking here is that our keywords land on that rung at all.
        let hay = &CommandSource::new().commands[0].hay;
        assert_eq!(rank::tier_of("paste", hay), Some(rank::TIER_KEYWORD));
        assert_eq!(rank::tier_of("copied", hay), Some(rank::TIER_KEYWORD));
    }

    /// The screenshot's exact query is a prefix of the title's *second word*, not
    /// a keyword at all. Worth pinning: it is why the command is findable before
    /// anyone has learned a keyword exists.
    #[test]
    fn v0_5_his_matches_the_titles_own_second_word() {
        let hay = Haystack::new("Clipboard History", None);
        assert_eq!(rank::tier_of("his", &hay), Some(rank::TIER_WORD_PREFIX));
    }

    #[test]
    fn v0_5_an_unrelated_query_finds_no_command() {
        assert!(hits("photoshop").is_empty());
        assert!(hits("zzzz").is_empty());
    }

    /// The id is the Frecency key and is written down in two places. If they
    /// drift, activation silently stops finding the command.
    #[test]
    fn v0_5_a_command_id_round_trips() {
        let id = CommandId::ClipboardHistory.entry_id();
        assert_eq!(id.as_str(), "command:clipboard-history");
        assert_eq!(
            CommandId::from_entry(&id),
            Some(CommandId::ClipboardHistory)
        );
        assert_eq!(CommandId::from_entry(&EntryId("c:\\app.exe".into())), None);
        assert_eq!(CommandId::from_entry(&EntryId("command:nope".into())), None);
    }

    /// A Command shares the App tier: it is a destination, not a document.
    #[test]
    fn v0_5_a_command_competes_with_applications_rather_than_below_them() {
        assert_eq!(EntryKind::Command.tier(), EntryKind::App.tier());
        assert_eq!(EntryKind::Command.weight(), EntryKind::App.weight());
    }
}
