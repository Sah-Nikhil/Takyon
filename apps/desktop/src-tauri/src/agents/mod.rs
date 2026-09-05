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
        AgentKind::from_wire(value).unwrap_or(AgentKind::Claude)
    }

    /// Parse exactly, or `None`. What `parse_order` needs: there an unknown name
    /// silently becoming Claude would displace a real preference.
    pub fn from_wire(value: &str) -> Option<AgentKind> {
        match value.trim().to_lowercase().as_str() {
            "claude" => Some(AgentKind::Claude),
            "codex" => Some(AgentKind::Codex),
            "opencode" => Some(AgentKind::OpenCode),
            _ => None,
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

/// The house style every Turn answers in.
///
/// A launcher answer is read in a 560px box, one keystroke from whatever the
/// user was doing. Three paragraphs of preamble is the wrong shape for that, and
/// no Agent's default is this terse. Sent as a system prompt where the CLI has
/// one and prepended to the first Turn's prompt where it does not.
pub const ANSWER_STYLE: &str = "Answer in as few words as the question allows. No preamble, no restatement of the question, no closing offer, no summary of what you just said. Drop articles and filler; fragments are fine. One line when one line answers it. Prose, not bullets, unless the answer is genuinely a list. Never abbreviate identifiers, API names, file paths, error strings, names or numbers, and never drop a caveat that changes the answer.";

/// The prompt for an Agent with no system-prompt flag of its own.
///
/// First Turn only: a resumed session already carries the style, and repeating
/// it every follow-up spends tokens saying what was already said.
pub fn styled_prompt(req: &TurnRequest) -> String {
    match req.session {
        Some(_) => req.prompt.clone(),
        None => format!("{ANSWER_STYLE}

{}", req.prompt),
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

/// The order `!c` tries Agents in, from the stored `agents.order` JSON.
///
/// Never a partial list. Unknown names drop, duplicates collapse, and whatever
/// is missing is appended in `ALL` order — a list short one Agent would leave
/// `!c` with no fallback the day a fourth ships.
pub fn parse_order(stored: Option<&str>) -> Vec<AgentKind> {
    let named = stored
        .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
        .unwrap_or_default();
    normalise_order(named.iter().filter_map(|n| AgentKind::from_wire(n)).collect())
}

/// Dedupe an order and back-fill it. The only shape allowed to be stored.
pub fn normalise_order(chosen: Vec<AgentKind>) -> Vec<AgentKind> {
    let mut order: Vec<AgentKind> = Vec::with_capacity(AgentKind::ALL.len());
    for kind in chosen.into_iter().chain(AgentKind::ALL) {
        if !order.contains(&kind) {
            order.push(kind);
        }
    }
    order
}

/// The switched-on Agents in preference order — exactly what `!c` walks.
///
/// Preference only: no Agent is probed, because this is read at startup and on
/// every Settings write, and probing costs three process spawns. Empty means
/// every Agent is switched off, which the Palette says rather than guessing.
pub fn route(prefs: &crate::prefs::Prefs) -> Vec<AgentKind> {
    let stored = match prefs.get(crate::prefs::ASK_ORDER) {
        Some(order) => parse_order(Some(&order)),
        // Seeded from the older single-choice key, so an install made before
        // the order existed keeps its Agent first.
        None => normalise_order(vec![AgentKind::parse(
            prefs.get(crate::prefs::ASK_AGENT).unwrap_or_default().as_str(),
        )]),
    };
    stored
        .into_iter()
        .filter(|kind| {
            crate::prefs::flag(prefs, &crate::prefs::ask_enabled_key(*kind), true)
        })
        .collect()
}

/// The order as `settings.db` holds it: one JSON row rather than three keys.
pub fn order_to_json(order: &[AgentKind]) -> String {
    let names: Vec<&str> = order.iter().map(|kind| kind.as_str()).collect();
    serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_string())
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
    fn v0_8_agent_kinds_round_trip_through_their_stored_spelling() {
        for kind in AgentKind::ALL {
            assert_eq!(AgentKind::parse(kind.as_str()), kind);
        }
        assert_eq!(AgentKind::parse("OpenCode"), AgentKind::OpenCode);
        assert_eq!(AgentKind::parse("  codex "), AgentKind::Codex);
    }

    /// A typo must leave `!c` working rather than breaking it.
    #[test]
    fn v0_8_an_unrecognised_stored_agent_falls_back_rather_than_failing() {
        assert_eq!(AgentKind::parse("gemini"), AgentKind::Claude);
        assert_eq!(AgentKind::parse(""), AgentKind::Claude);
    }

    /// Every kind has a driver, or `!c` can select an Agent that cannot answer.
    #[test]
    fn v0_8_every_agent_kind_has_a_driver() {
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
    fn v0_8_every_agent_offers_its_own_effort_vocabulary() {
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
    fn v0_8_effort_vocabularies_are_not_interchangeable() {
        let claude = driver_for(AgentKind::Claude).unwrap();
        let opencode = driver_for(AgentKind::OpenCode).unwrap();
        assert!(claude.efforts().contains(&"medium"));
        assert!(!opencode.efforts().contains(&"medium"));
    }

    /// The stored order round trips, so Settings shows back what it wrote.
    #[test]
    fn v0_8_a_preference_order_round_trips_through_its_json() {
        let chosen = vec![AgentKind::OpenCode, AgentKind::Codex, AgentKind::Claude];
        let json = order_to_json(&chosen);
        assert_eq!(json, r#"["opencode","codex","claude"]"#);
        assert_eq!(parse_order(Some(&json)), chosen);
    }

    /// Every parse yields every Agent exactly once, whatever went in. A short
    /// list is what costs `!c` its fallback, so there is no way to store one.
    #[test]
    fn v0_8_a_preference_order_is_always_every_agent_once() {
        let cases = [
            None,
            Some(r#"[]"#),
            Some(r#"["codex"]"#),
            Some(r#"["codex","codex"]"#),
            Some(r#"["gemini","codex"]"#),
            Some("not json at all"),
        ];
        for stored in cases {
            let order = parse_order(stored);
            assert_eq!(order.len(), AgentKind::ALL.len(), "{stored:?}");
            for kind in AgentKind::ALL {
                assert!(order.contains(&kind), "{stored:?} lost {kind:?}");
            }
        }
        // A chosen Agent still leads, and the back-fill follows it.
        assert_eq!(parse_order(Some(r#"["codex"]"#))[0], AgentKind::Codex);
    }

    /// `!c` walks the switched-on Agents in preference order, and reads only
    /// preferences to know it — no Agent is probed (v0.8 Traps).
    #[test]
    fn v0_8_the_route_is_the_switched_on_agents_in_order() {
        let prefs = crate::prefs::Prefs::open(None).unwrap();
        assert_eq!(route(&prefs), AgentKind::ALL.to_vec());

        prefs
            .set(crate::prefs::ASK_ORDER, r#"["opencode","codex","claude"]"#)
            .unwrap();
        prefs
            .set(&crate::prefs::ask_enabled_key(AgentKind::Codex), "0")
            .unwrap();
        assert_eq!(route(&prefs), vec![AgentKind::OpenCode, AgentKind::Claude]);
    }

    /// Every Agent off is a real state, and an empty route is how `!c` learns it.
    #[test]
    fn v0_8_switching_every_agent_off_leaves_nothing_to_ask() {
        let prefs = crate::prefs::Prefs::open(None).unwrap();
        for kind in AgentKind::ALL {
            prefs.set(&crate::prefs::ask_enabled_key(kind), "0").unwrap();
        }
        assert!(route(&prefs).is_empty());
    }

    /// An install made before the order existed keeps its one chosen Agent
    /// first, rather than silently reverting to Claude.
    #[test]
    fn v0_8_the_older_single_choice_seeds_the_order() {
        let prefs = crate::prefs::Prefs::open(None).unwrap();
        prefs.set(crate::prefs::ASK_AGENT, "opencode").unwrap();
        assert_eq!(route(&prefs)[0], AgentKind::OpenCode);
    }

    /// Every Turn answers in the house style, whichever Agent writes it. Claude
    /// takes it as a system prompt; the other two have no flag for one, so it
    /// leads the prompt instead.
    #[test]
    fn v0_9_every_driver_carries_the_answer_style_on_a_first_turn() {
        let base = TurnRequest {
            prompt: "who directed fast five".into(),
            cwd: std::path::PathBuf::from(r"C:\scratch"),
            session: None,
            model: None,
            effort: None,
            tools: false,
        };
        for driver in drivers() {
            let args = driver.turn_args(&base);
            assert!(
                args.iter().any(|a| a.contains(ANSWER_STYLE)),
                "{} sent no answer style",
                driver.label()
            );
            // Never instead of the question.
            assert!(
                args.iter().any(|a| a.contains("who directed fast five")),
                "{} lost the question",
                driver.label()
            );
        }
    }

    /// A follow-up must not repeat the style: the session already carries it, and
    /// resending it every Turn is tokens spent saying what was already said.
    #[test]
    fn v0_9_a_resumed_turn_does_not_repeat_the_style_in_its_prompt() {
        let resumed = TurnRequest {
            prompt: "and the producer".into(),
            cwd: std::path::PathBuf::from(r"C:\scratch"),
            session: Some("abc".into()),
            model: None,
            effort: None,
            tools: true,
        };
        assert_eq!(styled_prompt(&resumed), "and the producer");
        assert!(styled_prompt(&TurnRequest { session: None, ..resumed }).contains(ANSWER_STYLE));
    }

    /// The style has to actually ask for brevity, or it is decoration that costs
    /// a system prompt on every Turn.
    #[test]
    fn v0_9_the_answer_style_asks_for_brevity_and_protects_exactness() {
        assert!(ANSWER_STYLE.contains("as few words"));
        assert!(ANSWER_STYLE.contains("No preamble"));
        assert!(ANSWER_STYLE.contains("Never abbreviate"));
    }

    /// The missing-CLI sentence names the binary, because that is the fix.
    #[test]
    fn v0_8_a_missing_agent_names_the_command_that_is_absent() {
        let snap = Snapshot::missing(AgentKind::Codex, "Codex", "codex");
        assert!(!snap.installed);
        assert_eq!(snap.health, Health::Error);
        assert_eq!(snap.sign_in.status, SignInStatus::Unknown);
        assert!(snap.message.unwrap().contains("`codex`"));
    }
}
