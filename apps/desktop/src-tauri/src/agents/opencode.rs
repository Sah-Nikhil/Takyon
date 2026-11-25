//! opencode, driven through `opencode run --format json`.
//!
//! opencode has no account of its own, so there is no sign-in to read. T3 Code
//! counts **connected upstream providers** instead and calls one or more
//! authenticated; we count the same thing from `opencode models`.
//!
//! What that means is worth stating: opencode ships free `opencode/*` models that
//! need no credential, so a fresh install reports one provider and answers
//! questions. Not a bug in the probe — "can this Agent answer right now" is the
//! question the card is actually asked.

use std::collections::BTreeSet;

use serde_json::Value;

use super::probe::{self, PROBE_TIMEOUT};
use super::turn::TurnEvent;
use super::{
    AgentDriver, AgentKind, Health, SignIn, SignInStatus, Snapshot, TurnRequest, TurnState,
};

pub struct OpenCodeDriver;

const LABEL: &str = "opencode";
const BINARY: &str = "opencode";

/// The read-only primary agent opencode ships. Its stand-in for tools-off, which
/// opencode does not have; Scratch covers the gap (ADR-0017).
const READ_ONLY_AGENT: &str = "plan";

impl AgentDriver for OpenCodeDriver {
    fn kind(&self) -> AgentKind {
        AgentKind::OpenCode
    }

    fn label(&self) -> &'static str {
        LABEL
    }

    fn binary(&self) -> &'static str {
        BINARY
    }

    fn probe(&self, exe: &std::path::Path) -> Snapshot {
        let version = probe::run(exe, &["--version"], PROBE_TIMEOUT)
            .ok()
            .filter(probe::Output::ok)
            .and_then(|out| probe::version_from(&out.stdout));

        match probe::run(exe, &["models"], PROBE_TIMEOUT) {
            Ok(out) if out.ok() => snapshot_from_models(version, &out.stdout),
            _ => Snapshot {
                kind: AgentKind::OpenCode,
                label: LABEL,
                binary: BINARY,
                installed: true,
                version,
                health: Health::Warning,
                sign_in: SignIn::unknown(),
                message: Some("opencode is installed but did not list its models.".into()),
                efforts: OpenCodeDriver.efforts(),
            },
        }
    }

    /// The model's variant: provider-specific reasoning effort. v2 dropped the
    /// `--variant` flag for `-m provider/model#variant`; the words are unchanged.
    fn efforts(&self) -> &'static [&'static str] {
        &["minimal", "high", "max"]
    }

    /// A model on opencode's own Zen provider: the one every install
    /// reaches without connecting a provider of its own.
    fn search_model(&self) -> Option<&'static str> {
        Some("opencode/big-pickle")
    }

    /// `opencode models`, which prints one `provider/model` per line.
    ///
    /// The same output the Sign-in probe counts providers from, read a second
    /// way. Whole slugs, because that is what `-m` takes.
    fn models(&self, exe: &std::path::Path) -> Vec<String> {
        let Ok(out) = probe::run(exe, &["models"], PROBE_TIMEOUT) else {
            return Vec::new();
        };
        if !out.ok() {
            return Vec::new();
        }
        out.stdout
            .lines()
            .map(str::trim)
            .filter(|line| line.contains('/') && !line.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// No `--dir` and no `--variant`: opencode v2.4 refuses both with
    /// `Unrecognized flag`. The cwd is the process's, the variant rides on `-m`.
    fn turn_args(&self, req: &TurnRequest) -> Vec<String> {
        let mut args = vec!["run".to_string(), "--format".into(), "json".into()];
        if !req.tools {
            args.push("--agent".into());
            args.push(READ_ONLY_AGENT.into());
        }
        if let Some(model) = &req.model {
            args.push("-m".into());
            // A variant needs a model to hang on; alone it has nowhere to go.
            args.push(match &req.effort {
                Some(effort) => format!("{model}#{effort}"),
                None => model.clone(),
            });
        }
        if let Some(session) = &req.session {
            args.push("-s".into());
            args.push(session.clone());
        }
        // No message argument: `run` reads stdin when it is not a TTY (ADR-0032).
        // Style leads the default `turn_input`, no system prompt to append to.
        args
    }

    /// v2 has no directory flag on `run`; it works where it is started.
    fn cwd_is_process_cwd(&self) -> bool {
        true
    }

    fn parse_line(&self, line: &str, state: &mut TurnState) -> Option<TurnEvent> {
        let json: Value = serde_json::from_str(line).ok()?;
        let kind = json.get("type").and_then(Value::as_str)?;

        // Every event carries the session, and the first one to arrive is what a
        // follow-up resumes. `step_start` is normally it.
        if state.session.is_none() {
            if let Some(session) = text_at(&json, "sessionID") {
                state.session = Some(session.clone());
                return Some(TurnEvent::Started {
                    session: Some(session),
                    model: None,
                });
            }
        }

        match kind {
            // opencode re-sends a growing part rather than appending, so only the
            // unseen tail is news.
            "text" => {
                let part = json.get("part")?;
                let id = text_at(part, "id")?;
                let text = part.get("text").and_then(Value::as_str)?;
                state
                    .delta(&id, text)
                    .map(|delta| TurnEvent::Text { delta })
            }
            "error" => Some(TurnEvent::agent_error(
                LABEL,
                text_at(&json, "message")
                    .or_else(|| {
                        json.pointer("/error/message")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .unwrap_or_else(|| "opencode stopped with an error.".into()),
            )),
            _ => None,
        }
    }
}

fn text_at(json: &Value, key: &str) -> Option<String> {
    json.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

/// Distinct provider ids in `opencode models` output — the part before the `/`.
pub fn connected_providers(stdout: &str) -> BTreeSet<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| line.split_once('/'))
        .map(|(provider, _)| provider.to_string())
        .filter(|provider| !provider.is_empty())
        .collect()
}

/// Turn `opencode models` output into a Snapshot, T3 Code's rule for the count.
pub fn snapshot_from_models(version: Option<String>, stdout: &str) -> Snapshot {
    let providers = connected_providers(stdout);
    let count = providers.len();
    let connected = count > 0;
    Snapshot {
        kind: AgentKind::OpenCode,
        label: LABEL,
        binary: BINARY,
        installed: true,
        version,
        health: if connected {
            Health::Ready
        } else {
            Health::Warning
        },
        sign_in: if connected {
            SignIn {
                status: SignInStatus::In,
                label: Some(format!(
                    "{count} provider{} connected",
                    if count == 1 { "" } else { "s" }
                )),
                account: None,
            }
        } else {
            SignIn::out()
        },
        message: (!connected).then(|| {
            "No providers are connected to opencode. Run `opencode providers login`.".into()
        }),
        efforts: OpenCodeDriver.efforts(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real `opencode models` output: one provider, many models.
    #[test]
    fn v0_8_opencode_counts_distinct_providers_not_models() {
        let stdout = "opencode/big-pickle\nopencode/mimo-v2.5-free\nanthropic/claude-opus-5\n";
        let providers = connected_providers(stdout);
        assert_eq!(providers.len(), 2);
        assert!(providers.contains("opencode"));

        let snap = snapshot_from_models(Some("1.18.27".into()), stdout);
        assert_eq!(snap.sign_in.status, SignInStatus::In);
        assert_eq!(snap.sign_in.label.as_deref(), Some("2 providers connected"));
        assert_eq!(snap.health, Health::Ready);
    }

    /// One provider is singular. The card reads as English or it reads as a bug.
    #[test]
    fn v0_8_one_connected_provider_is_singular() {
        let snap = snapshot_from_models(None, "opencode/big-pickle\n");
        assert_eq!(snap.sign_in.label.as_deref(), Some("1 provider connected"));
    }

    /// Nothing connected is a Warning with the command, not an Error: opencode
    /// itself is fine, it just has nothing to answer with.
    #[test]
    fn v0_8_no_connected_providers_says_what_to_run() {
        let snap = snapshot_from_models(None, "\n   \n");
        assert_eq!(snap.sign_in.status, SignInStatus::Out);
        assert_eq!(snap.health, Health::Warning);
        assert!(snap.message.unwrap().contains("opencode providers login"));
    }

    /// The first Turn runs the read-only `plan` agent; a follow-up does not.
    #[test]
    fn v0_8_the_inline_path_uses_opencodes_read_only_agent() {
        let base = TurnRequest {
            prompt: "hi".into(),
            cwd: std::path::PathBuf::from(r"C:\scratch"),
            session: None,
            model: None,
            effort: None,
            tools: false,
        };
        let args = OpenCodeDriver.turn_args(&base);
        let agent = args
            .iter()
            .position(|a| a == "--agent")
            .expect("agent flag");
        assert_eq!(args[agent + 1], READ_ONLY_AGENT);
        assert!(OpenCodeDriver.cwd_is_process_cwd());
        // The prompt travels on stdin, house style ahead of it.
        assert!(!args.iter().any(|a| a.ends_with("hi")));
        assert!(OpenCodeDriver.turn_input(&base).ends_with("hi"));

        let with_tools = OpenCodeDriver.turn_args(&TurnRequest {
            tools: true,
            ..base
        });
        assert!(!with_tools.contains(&"--agent".to_string()));
    }

    /// opencode v2.4.1 exits with `Unrecognized flag: --dir in command opencode
    /// run`, and has no `--variant`: neither may be sent. Effort rides on `-m`.
    #[test]
    fn v0_11_2_opencode_sends_no_flag_v2_refuses() {
        let base = TurnRequest {
            prompt: "hi".into(),
            cwd: std::path::PathBuf::from(r"C:\scratch"),
            session: None,
            model: Some("opencode/big-pickle".into()),
            effort: Some("high".into()),
            tools: false,
        };
        let args = OpenCodeDriver.turn_args(&base);
        assert!(
            !args.iter().any(|a| a == "--dir" || a == "--variant"),
            "{args:?}"
        );
        let model = args.iter().position(|a| a == "-m").expect("model flag");
        assert_eq!(args[model + 1], "opencode/big-pickle#high");

        // Effort with no model has nothing to attach to, so nothing is sent.
        let bare = OpenCodeDriver.turn_args(&TurnRequest {
            model: None,
            ..base
        });
        assert!(
            !bare.iter().any(|a| a == "-m" || a.contains('#')),
            "{bare:?}"
        );
    }

    /// The first event carrying a session id is what a follow-up resumes.
    #[test]
    fn v0_8_the_first_opencode_event_yields_the_session() {
        let mut state = TurnState::default();
        let event = OpenCodeDriver.parse_line(
            r#"{"type":"step_start","sessionID":"ses_1","part":{"type":"step-start"}}"#,
            &mut state,
        );
        assert_eq!(
            event,
            Some(TurnEvent::Started {
                session: Some("ses_1".into()),
                model: None
            })
        );
        assert_eq!(state.session.as_deref(), Some("ses_1"));
    }

    /// A re-sent part must arrive as a delta, or the answer reads "o ok okay".
    #[test]
    fn v0_8_a_regrown_text_part_arrives_as_a_delta() {
        let mut state = TurnState {
            session: Some("ses_1".into()),
            ..Default::default()
        };
        let line = |text: &str| {
            format!(
                r#"{{"type":"text","sessionID":"ses_1","part":{{"id":"p1","type":"text","text":"{text}"}}}}"#
            )
        };
        assert_eq!(
            OpenCodeDriver.parse_line(&line("Hel"), &mut state),
            Some(TurnEvent::Text {
                delta: "Hel".into()
            })
        );
        assert_eq!(
            OpenCodeDriver.parse_line(&line("Hello"), &mut state),
            Some(TurnEvent::Text { delta: "lo".into() })
        );
        // The same part again is not news.
        assert_eq!(OpenCodeDriver.parse_line(&line("Hello"), &mut state), None);
    }

    /// A second part is its own stream, not a continuation of the first.
    #[test]
    fn v0_8_two_text_parts_do_not_share_a_delta_cursor() {
        let mut state = TurnState {
            session: Some("ses_1".into()),
            ..Default::default()
        };
        let first = r#"{"type":"text","sessionID":"ses_1","part":{"id":"p1","text":"abcd"}}"#;
        let second = r#"{"type":"text","sessionID":"ses_1","part":{"id":"p2","text":"xy"}}"#;
        assert_eq!(
            OpenCodeDriver.parse_line(first, &mut state),
            Some(TurnEvent::Text {
                delta: "abcd".into()
            })
        );
        assert_eq!(
            OpenCodeDriver.parse_line(second, &mut state),
            Some(TurnEvent::Text { delta: "xy".into() })
        );
    }
}
