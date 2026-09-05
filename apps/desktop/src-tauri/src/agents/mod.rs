//! Agents: the coding-agent CLIs Takyon drives (`CONTEXT.md` §Agents).
//!
//! One `AgentDriver` per Agent, one registry holding them. Shape follows T3
//! Code's `ProviderDriver` SPI — a driver is a value, the registry is the only
//! singleton — with its vocabulary swapped for ours.
//!
//! Two rules the whole module exists to keep. Takyon never changes an Agent's
//! Sign-in state, only reads it (ADR-0017). And nothing Agent-specific lives
//! outside a driver file, so a fourth Agent is a new file rather than a
//! search-and-replace.

pub mod claude;
pub mod codex;
pub mod ipc;
pub mod opencode;
pub mod probe;
pub mod scratch;
pub mod turn;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Which Agent. The wire spellings are stored in `settings.db`, so renaming one
/// breaks a saved preference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentKind {
    Claude,
    Codex,
    #[serde(rename = "opencode")]
    OpenCode,
}

impl AgentKind {
    /// Every Agent, in the order Settings lists them.
    pub const ALL: [AgentKind; 3] = [AgentKind::Claude, AgentKind::Codex, AgentKind::OpenCode];

    pub fn as_str(self) -> &'static str {
        match self {
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
            AgentKind::OpenCode => "opencode",
        }
    }

    /// Parse a stored preference. Anything unrecognised is the default Agent
    /// rather than an error: a hand-edited `settings.db` must not break `!c`.
    pub fn parse(value: &str) -> AgentKind {
        match value.trim().to_lowercase().as_str() {
            "codex" => AgentKind::Codex,
            "opencode" => AgentKind::OpenCode,
            _ => AgentKind::Claude,
        }
    }
}

/// What Takyon can say about an Agent's credentials, asked of the Agent itself.
///
/// `Unknown` is not a failure: it means installed but would not answer, which is
/// a different sentence from `Out` and must stay one (ADR-0017).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SignInStatus {
    In,
    Out,
    Unknown,
}

/// Sign-in state plus the label the Agent gave for it.
///
/// `label` is the Agent's own words — "Pro", "ChatGPT", "2 providers connected" —
/// shown verbatim and never parsed for meaning.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignIn {
    pub status: SignInStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
}

impl SignIn {
    pub fn unknown() -> Self {
        SignIn {
            status: SignInStatus::Unknown,
            label: None,
            account: None,
        }
    }

    pub fn out() -> Self {
        SignIn {
            status: SignInStatus::Out,
            label: None,
            account: None,
        }
    }
}

/// How usable the Agent is right now. T3 Code's four states, same meanings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Health {
    Ready,
    Warning,
    Error,
}

/// Everything a Settings card and the `!c` empty state need, in one value.
///
/// Facts only, no rendered copy: the headline is assembled in TypeScript, the
/// way T3 Code's `providerStatus.ts` does it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub kind: AgentKind,
    pub label: &'static str,
    /// The command the user would type. Also the "not found" message's subject.
    pub binary: &'static str,
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub health: Health,
    pub sign_in: SignIn,
    /// The Agent's own sentence, including the command to run when signed out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Effort levels this Agent accepts, weakest first. Static, so it costs
    /// nothing — models are a separate call because they cost a spawn.
    pub efforts: &'static [&'static str],
}

impl Snapshot {
    /// The snapshot for an Agent whose binary is nowhere on `PATH`.
    ///
    /// Wording is T3 Code's, with its CLI name swapped in. Recoverable by
    /// re-probing, never by restarting — a `bun add -g` after login is invisible
    /// to an already-running Takyon until then.
    pub fn missing(kind: AgentKind, label: &'static str, binary: &'static str) -> Self {
        Snapshot {
            kind,
            label,
            binary,
            installed: false,
            version: None,
            health: Health::Error,
            sign_in: SignIn::unknown(),
            message: Some(format!("{label} (`{binary}`) was not found on PATH.")),
            efforts: &[],
        }
    }
}

/// What one Turn needs to know before it can be spawned.
#[derive(Clone, Debug)]
pub struct TurnRequest {
    pub prompt: String,
    pub cwd: PathBuf,
    /// Resume this Agent session rather than starting one. `None` is turn one.
    pub session: Option<String>,
    /// The model Settings locked in. `None` is the Agent's own default, which is
    /// what a fresh install has until someone chooses.
    pub model: Option<String>,
    /// The effort level Settings locked in, from that Agent's own vocabulary.
    pub effort: Option<String>,
    /// False on the inline Palette path. Each driver spells this differently and
    /// says so at its own site; the Scratch directory is what makes it safe.
    pub tools: bool,
}

/// Mutable state one Turn's line parser carries across lines.
#[derive(Default, Debug)]
pub struct TurnState {
    /// The Agent's session id, learned from its first event and needed to resume.
    pub session: Option<String>,
    /// Text already emitted per message part, so a re-sent part becomes a delta.
    ///
    /// opencode re-emits a growing part rather than appending; without this the
    /// answer arrives as "o", "ok", "okay" concatenated.
    pub emitted: std::collections::HashMap<String, usize>,
}

impl TurnState {
    /// The unseen tail of `text` for `part`, or `None` when nothing is new.
    pub fn delta(&mut self, part: &str, text: &str) -> Option<String> {
        let seen = self.emitted.entry(part.to_string()).or_insert(0);
        if text.len() <= *seen {
            return None;
        }
        let tail = text[*seen..].to_string();
        *seen = text.len();
        Some(tail)
    }
}

/// One Agent, as far as the rest of Takyon is concerned.
///
/// `probe` and `turn_args` are the whole SPI: everything else — spawning,
/// timeouts, line buffering, cancellation — is shared and lives in `probe.rs`
/// and `turn.rs`.
pub trait AgentDriver: Send + Sync {
    fn kind(&self) -> AgentKind;

    /// Display name. UI copy only, never keyed off.
    fn label(&self) -> &'static str;

    /// The command to resolve on `PATH`, without an extension.
    fn binary(&self) -> &'static str;

    /// Read installed state, version and Sign-in state. One or two spawns, no
    /// writes, and never a credential file (ADR-0017).
    fn probe(&self, exe: &std::path::Path) -> Snapshot;

    /// Effort levels this Agent accepts, weakest first.
    ///
    /// Static rather than probed: it is a fixed vocabulary per CLI, and putting
    /// a spawn behind it would cost the Settings page a round trip for a list
    /// that cannot change between releases.
    fn efforts(&self) -> &'static [&'static str];

    /// Models this Agent will answer with, for the Settings picker.
    ///
    /// Its own call rather than part of `probe`, because it costs a spawn and
    /// only Settings needs it — `!c` reads the locked-in choice, not the list.
    /// An Agent that will not say returns empty, and the picker says so.
    fn models(&self, exe: &std::path::Path) -> Vec<String>;

    /// The arguments for one Turn. The prompt is included; the cwd is not,
    /// because two of the three take it as a flag and one takes it as the
    /// process cwd.
    fn turn_args(&self, req: &TurnRequest) -> Vec<String>;

    /// Whether this Agent wants the cwd as the spawned process's directory
    /// rather than as a flag. Only Claude does.
    fn cwd_is_process_cwd(&self) -> bool {
        false
    }

    /// Map one complete line of stdout to an event, or to nothing.
    ///
    /// Never called with a partial line — `turn.rs` buffers to the newline, which
    /// is the bug this shape of code is famous for.
    fn parse_line(&self, line: &str, state: &mut TurnState) -> Option<turn::TurnEvent>;
}

/// Every driver this build ships with, in Settings order.
///
/// Mirrors T3 Code's `BUILT_IN_DRIVERS`: a static list the registry iterates,
/// so adding an Agent is one file plus one line here.
pub fn drivers() -> Vec<Box<dyn AgentDriver>> {
    vec![
        Box::new(claude::ClaudeDriver),
        Box::new(codex::CodexDriver),
        Box::new(opencode::OpenCodeDriver),
    ]
}

/// The driver for one kind, or `None` if this build does not ship it.
pub fn driver_for(kind: AgentKind) -> Option<Box<dyn AgentDriver>> {
    drivers().into_iter().find(|d| d.kind() == kind)
}

/// The models one Agent offers, or empty where it is missing or will not say.
///
/// Settings only. A spawn per call, so it is never on the `!c` path.
pub fn models_for(kind: AgentKind) -> Vec<String> {
    let Some(driver) = driver_for(kind) else {
        return Vec::new();
    };
    match probe::resolve(driver.binary()) {
        Some(exe) => driver.models(&exe),
        None => Vec::new(),
    }
}

/// Probe every Agent. Lazy by contract: never called on the login path.
pub fn snapshots() -> Vec<Snapshot> {
    drivers()
        .iter()
        .map(|driver| match probe::resolve(driver.binary()) {
            Some(exe) => driver.probe(&exe),
            None => Snapshot::missing(driver.kind(), driver.label(), driver.binary()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire spellings are stored preferences. A rename here is a migration.
    #[test]
    fn v0_9_agent_kinds_round_trip_through_their_stored_spelling() {
        for kind in AgentKind::ALL {
            assert_eq!(AgentKind::parse(kind.as_str()), kind);
        }
        assert_eq!(AgentKind::parse("OpenCode"), AgentKind::OpenCode);
        assert_eq!(AgentKind::parse("  codex "), AgentKind::Codex);
    }

    /// A typo must leave `!c` working rather than breaking it.
    #[test]
    fn v0_9_an_unrecognised_stored_agent_falls_back_rather_than_failing() {
        assert_eq!(AgentKind::parse("gemini"), AgentKind::Claude);
        assert_eq!(AgentKind::parse(""), AgentKind::Claude);
    }

    /// Every kind has a driver, or `!c` can select an Agent that cannot answer.
    #[test]
    fn v0_9_every_agent_kind_has_a_driver() {
        for kind in AgentKind::ALL {
            let driver = driver_for(kind).expect("every kind ships a driver");
            assert_eq!(driver.kind(), kind);
            assert!(!driver.binary().is_empty());
            assert!(!driver.label().is_empty());
        }
        assert_eq!(drivers().len(), AgentKind::ALL.len());
    }

    /// Every Agent offers effort levels, and every one of them is a word that
    /// Agent will actually accept — Settings refuses anything else.
    #[test]
    fn v0_9_every_agent_offers_its_own_effort_vocabulary() {
        for driver in drivers() {
            let efforts = driver.efforts();
            assert!(!efforts.is_empty(), "{} offers no effort", driver.label());
            for effort in efforts {
                assert!(!effort.is_empty());
                assert_eq!(*effort, effort.to_lowercase(), "{} is not a wire value", effort);
            }
        }
    }

    /// The three vocabularies genuinely differ, which is why the list is per
    /// Agent rather than one shared enum.
    #[test]
    fn v0_9_effort_vocabularies_are_not_interchangeable() {
        let claude = driver_for(AgentKind::Claude).unwrap();
        let opencode = driver_for(AgentKind::OpenCode).unwrap();
        assert!(claude.efforts().contains(&"medium"));
        assert!(!opencode.efforts().contains(&"medium"));
    }

    /// The missing-CLI sentence names the binary, because that is the fix.
    #[test]
    fn v0_9_a_missing_agent_names_the_command_that_is_absent() {
        let snap = Snapshot::missing(AgentKind::Codex, "Codex", "codex");
        assert!(!snap.installed);
        assert_eq!(snap.health, Health::Error);
        assert_eq!(snap.sign_in.status, SignInStatus::Unknown);
        assert!(snap.message.unwrap().contains("`codex`"));
    }
}
