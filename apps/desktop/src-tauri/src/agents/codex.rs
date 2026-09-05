//! Codex, driven through `codex exec --json`.
//!
//! Sign-in is an exit code: `codex login status` writes its answer to **stderr**
//! and exits 0 or 1. The line is shown as a label and never parsed for meaning,
//! because it is prose the CLI is free to reword.
//!
//! Codex has no tools-off switch. `--sandbox read-only` is the closest posture,
//! and the Scratch directory is what makes the difference immaterial: a read-only
//! Agent in an empty directory can read an empty directory (ADR-0017).

use serde_json::Value;

use super::probe::{self, PROBE_TIMEOUT};
use super::turn::TurnEvent;
use super::{AgentDriver, AgentKind, Health, SignIn, SignInStatus, Snapshot, TurnRequest, TurnState};

pub struct CodexDriver;

const LABEL: &str = "Codex";
const BINARY: &str = "codex";

/// T3 Code's wording for a signed-out Codex, kept to the letter.
const SIGNED_OUT: &str = "Codex CLI is not authenticated. Run `codex login` and try again.";

impl AgentDriver for CodexDriver {
    fn kind(&self) -> AgentKind {
        AgentKind::Codex
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

        match probe::run(exe, &["login", "status"], PROBE_TIMEOUT) {
            Ok(out) => snapshot_from_login(version, out.code, out.first_line()),
            Err(_) => Snapshot {
                kind: AgentKind::Codex,
                label: LABEL,
                binary: BINARY,
                installed: true,
                version,
                health: Health::Warning,
                sign_in: SignIn::unknown(),
                message: Some("Codex is installed but did not answer.".into()),
                efforts: CodexDriver.efforts(),
            },
        }
    }

    /// `ReasoningEffort` from `codex-rs/protocol/src/openai_models.rs`.
    fn efforts(&self) -> &'static [&'static str] {
        &[
            "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
        ]
    }

    /// `codex debug models --bundled`, which renders the catalogue as JSON.
    ///
    /// `--bundled` deliberately: the refreshing form goes to the network, and a
    /// Settings page that opens a connection to fill a dropdown is a surprise.
    fn models(&self, exe: &std::path::Path) -> Vec<String> {
        let Ok(out) = probe::run(exe, &["debug", "models", "--bundled"], PROBE_TIMEOUT) else {
            return Vec::new();
        };
        if !out.ok() {
            return Vec::new();
        }
        model_ids(&out.stdout)
    }

    fn turn_args(&self, req: &TurnRequest) -> Vec<String> {
        let mut args = vec!["exec".to_string()];
        if let Some(session) = &req.session {
            args.push("resume".into());
            args.push(session.clone());
        }
        args.push("--json".into());
        // Codex refuses to run outside a repository, and the Scratch directory is
        // deliberately not one.
        args.push("--skip-git-repo-check".into());
        args.push("-C".into());
        args.push(req.cwd.to_string_lossy().to_string());
        args.push("--sandbox".into());
        args.push(if req.tools { "workspace-write" } else { "read-only" }.into());
        if let Some(model) = &req.model {
            args.push("-m".into());
            args.push(model.clone());
        }
        if let Some(effort) = &req.effort {
            // Codex has no effort flag; it is a config key, overridden per run.
            args.push("-c".into());
            args.push(format!("model_reasoning_effort=\"{effort}\""));
        }
        // No system-prompt flag, so the style leads the first Turn's prompt.
        args.push(super::styled_prompt(req));
        args
    }

    fn parse_line(&self, line: &str, state: &mut TurnState) -> Option<TurnEvent> {
        let json: Value = serde_json::from_str(line).ok()?;
        match json.get("type").and_then(Value::as_str)? {
            "thread.started" => {
                state.session = text_at(&json, "thread_id");
                Some(TurnEvent::Started {
                    session: state.session.clone(),
                    model: None,
                })
            }
            // `item.details` is flattened into `item`, so the payload's own type
            // sits beside its id.
            "item.completed" => {
                let item = json.get("item")?;
                let is_message = item.get("type").and_then(Value::as_str) == Some("agent_message");
                let delta = is_message.then(|| text_at(item, "text")).flatten()?;
                Some(TurnEvent::Text { delta })
            }
            "turn.failed" => Some(TurnEvent::Failed {
                message: json
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("Codex stopped with an error.")
                    .to_string(),
            }),
            "error" => Some(TurnEvent::Failed {
                message: text_at(&json, "message")
                    .unwrap_or_else(|| "Codex stopped with an error.".into()),
            }),
            _ => None,
        }
    }
}

/// Every `id` or `slug` in the catalogue JSON, deduplicated, order kept.
///
/// Walks the tree rather than decoding it: the catalogue's shape is Codex's to
/// change, and a picker that empties on a schema tweak is worse than one built
/// from whatever ids are in there.
pub fn model_ids(stdout: &str) -> Vec<String> {
    let Ok(json) = serde_json::from_str::<Value>(stdout.trim()) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    collect_ids(&json, &mut found);
    found.dedup();
    found
}

fn collect_ids(json: &Value, out: &mut Vec<String>) {
    match json {
        Value::Object(map) => {
            for key in ["id", "slug"] {
                if let Some(id) = map.get(key).and_then(Value::as_str) {
                    if !id.is_empty() && !out.iter().any(|seen| seen == id) {
                        out.push(id.to_string());
                    }
                }
            }
            for value in map.values() {
                collect_ids(value, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_ids(item, out);
            }
        }
        _ => {}
    }
}

fn text_at(json: &Value, key: &str) -> Option<String> {
    json.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

/// Turn `codex login status`'s exit code and first line into a Snapshot.
pub fn snapshot_from_login(
    version: Option<String>,
    code: Option<i32>,
    first_line: Option<&str>,
) -> Snapshot {
    let signed_in = code == Some(0);
    Snapshot {
        kind: AgentKind::Codex,
        label: LABEL,
        binary: BINARY,
        installed: true,
        version,
        health: if signed_in { Health::Ready } else { Health::Error },
        sign_in: if signed_in {
            SignIn {
                status: SignInStatus::In,
                label: first_line.map(str::to_string),
                account: None,
            }
        } else {
            SignIn::out()
        },
        message: (!signed_in).then(|| SIGNED_OUT.to_string()),
        efforts: CodexDriver.efforts(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exit 0 is the signal; the line beside it is a label, nothing more.
    #[test]
    fn v0_8_codex_reads_sign_in_from_the_exit_code() {
        let snap = snapshot_from_login(
            Some("0.52.0".into()),
            Some(0),
            Some("Logged in using ChatGPT"),
        );
        assert_eq!(snap.sign_in.status, SignInStatus::In);
        assert_eq!(snap.sign_in.label.as_deref(), Some("Logged in using ChatGPT"));
        assert_eq!(snap.health, Health::Ready);
        assert!(snap.message.is_none());
    }

    /// The signed-out sentence is T3 Code's, and it carries the command.
    #[test]
    fn v0_8_a_signed_out_codex_says_what_to_run() {
        let snap = snapshot_from_login(None, Some(1), Some("Not logged in"));
        assert_eq!(snap.sign_in.status, SignInStatus::Out);
        assert_eq!(snap.health, Health::Error);
        assert_eq!(snap.message.as_deref(), Some(SIGNED_OUT));
    }

    /// A killed probe has no exit code, and must not read as signed in.
    #[test]
    fn v0_8_a_timed_out_codex_probe_is_not_signed_in() {
        assert_eq!(
            snapshot_from_login(None, None, None).sign_in.status,
            SignInStatus::Out
        );
    }

    /// Codex refuses to run outside a repository, and Scratch is not one.
    #[test]
    fn v0_8_codex_always_skips_the_git_repository_check() {
        let args = CodexDriver.turn_args(&TurnRequest {
            prompt: "hi".into(),
            cwd: std::path::PathBuf::from(r"C:\scratch"),
            session: None,
            model: None,
            effort: None,
            tools: false,
        });
        assert!(args.contains(&"--skip-git-repo-check".to_string()));
        let cd = args.iter().position(|a| a == "-C").expect("cd flag");
        assert_eq!(args[cd + 1], r"C:\scratch");
        let sandbox = args.iter().position(|a| a == "--sandbox").expect("sandbox flag");
        assert_eq!(args[sandbox + 1], "read-only");
        // The prompt is last, after every flag, with the house style ahead of it.
        assert!(args.last().unwrap().ends_with("hi"));
    }

    /// A follow-up is `exec resume <id>`, and the id comes right after `resume`.
    #[test]
    fn v0_8_a_codex_follow_up_resumes_its_thread() {
        let args = CodexDriver.turn_args(&TurnRequest {
            prompt: "and then?".into(),
            cwd: std::path::PathBuf::from("."),
            session: Some("th-1".into()),
            model: None,
            effort: None,
            tools: true,
        });
        assert_eq!(&args[..3], &["exec", "resume", "th-1"]);
        let sandbox = args.iter().position(|a| a == "--sandbox").expect("sandbox flag");
        assert_eq!(args[sandbox + 1], "workspace-write");
    }

    /// The thread id is what a follow-up resumes.
    #[test]
    fn v0_8_the_thread_started_event_yields_the_session() {
        let mut state = TurnState::default();
        let event =
            CodexDriver.parse_line(r#"{"type":"thread.started","thread_id":"th-1"}"#, &mut state);
        assert_eq!(
            event,
            Some(TurnEvent::Started { session: Some("th-1".into()), model: None })
        );
        assert_eq!(state.session.as_deref(), Some("th-1"));
    }

    /// Only `agent_message` items are answer text. Reasoning is not shown.
    #[test]
    fn v0_8_only_agent_messages_reach_the_palette() {
        let mut state = TurnState::default();
        let message =
            r#"{"type":"item.completed","item":{"id":"i1","type":"agent_message","text":"Hello"}}"#;
        assert_eq!(
            CodexDriver.parse_line(message, &mut state),
            Some(TurnEvent::Text { delta: "Hello".into() })
        );

        let reasoning =
            r#"{"type":"item.completed","item":{"id":"i2","type":"reasoning","text":"hmm"}}"#;
        assert_eq!(CodexDriver.parse_line(reasoning, &mut state), None);
    }

    /// The catalogue is Codex's to reshape, so ids are found rather than decoded.
    #[test]
    fn v0_8_model_ids_are_found_wherever_the_catalogue_puts_them() {
        let json = r#"{"models":[{"id":"gpt-5.3-codex","slug":"gpt-5.3-codex"},
                       {"id":"gpt-5.3-codex-mini"}],"default":{"id":"gpt-5.3-codex"}}"#;
        assert_eq!(model_ids(json), vec!["gpt-5.3-codex", "gpt-5.3-codex-mini"]);
    }

    /// Anything unparseable empties the picker rather than filling it with junk.
    #[test]
    fn v0_8_an_unreadable_catalogue_yields_no_models() {
        assert!(model_ids("not json").is_empty());
        assert!(model_ids("").is_empty());
    }

    /// Effort is a config override for Codex, not a flag, and it is quoted.
    #[test]
    fn v0_8_codex_spells_effort_as_a_config_override() {
        let args = CodexDriver.turn_args(&TurnRequest {
            prompt: "hi".into(),
            cwd: std::path::PathBuf::from("."),
            session: None,
            model: Some("gpt-5.3-codex".into()),
            effort: Some("high".into()),
            tools: false,
        });
        let flag = args.iter().position(|a| a == "-c").expect("config flag");
        assert_eq!(args[flag + 1], "model_reasoning_effort=\"high\"");
    }

    /// Both failure shapes carry their own message through unchanged.
    #[test]
    fn v0_8_codex_failures_keep_their_own_words() {
        let mut state = TurnState::default();
        assert_eq!(
            CodexDriver.parse_line(
                r#"{"type":"turn.failed","error":{"message":"rate limited"}}"#,
                &mut state
            ),
            Some(TurnEvent::Failed { message: "rate limited".into() })
        );
        assert_eq!(
            CodexDriver.parse_line(r#"{"type":"error","message":"boom"}"#, &mut state),
            Some(TurnEvent::Failed { message: "boom".into() })
        );
    }
}
