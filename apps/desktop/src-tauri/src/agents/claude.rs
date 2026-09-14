//! Claude Code, driven through `claude -p --output-format stream-json`.
//!
//! Sign-in comes from `claude auth status --json`, which answers
//! `{loggedIn, authMethod, apiProvider, email, subscriptionType}` in one cheap
//! call. T3 Code reaches the same three facts by running a query through the
//! Agent SDK and reading its init result, because the SDK is what it had.
//!
//! The account labels below are T3 Code's `claudeAuthMetadata` ported verbatim,
//! so a Takyon card and a T3 Code card say the same words for the same account.

use serde_json::Value;

use super::probe::{self, PROBE_TIMEOUT};
use super::turn::TurnEvent;
use super::{
    AgentDriver, AgentKind, Health, SignIn, SignInStatus, Snapshot, TurnRequest, TurnState,
};

pub struct ClaudeDriver;

const LABEL: &str = "Claude Code";
const BINARY: &str = "claude";

impl AgentDriver for ClaudeDriver {
    fn kind(&self) -> AgentKind {
        AgentKind::Claude
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

        let status = probe::run(exe, &["auth", "status", "--json"], PROBE_TIMEOUT);
        let Ok(status) = status else {
            return unverified(version, "Claude Code is installed but did not answer.");
        };
        let Some(json) = serde_json::from_str::<Value>(status.stdout.trim()).ok() else {
            return unverified(
                version,
                "Could not read Claude Code's authentication status.",
            );
        };
        snapshot_from_auth(version, &json)
    }

    /// `--effort` from `claude --help`, weakest first.
    fn efforts(&self) -> &'static [&'static str] {
        &["low", "medium", "high", "xhigh", "max"]
    }

    /// The aliases `--model` documents, not a catalogue.
    ///
    /// `claude` has no models command, and an alias always resolves to the
    /// latest of its family — a bundled catalogue would be a list that goes
    /// stale between releases, which is what `docs/tbd/v0.8.md` §5 records.
    fn models(&self, _exe: &std::path::Path) -> Vec<String> {
        ["opus", "sonnet", "haiku", "fable"]
            .iter()
            .map(|m| m.to_string())
            .collect()
    }

    fn turn_args(&self, req: &TurnRequest) -> Vec<String> {
        let mut args = vec![
            // No positional prompt: `-p` reads it from stdin (ADR-0032).
            "-p".into(),
            "--output-format".into(),
            "stream-json".into(),
            // stream-json refuses to emit without it, rather than warning.
            "--verbose".into(),
            // Token deltas as `stream_event` lines, whole messages still after.
            "--include-partial-messages".into(),
            // Nobody is there to answer a permission prompt: a Turn that waits on
            // one waits forever. Denying is the only honest setting until v1.0
            // gives follow-up Turns a permission UI (docs/tbd/v0.8.md).
            "--permission-prompts".into(),
            "none".into(),
            // The one driver with a real system prompt. Appended, not replaced:
            // Claude's own default carries behaviour a Turn still needs.
            "--append-system-prompt".into(),
            super::ANSWER_STYLE.to_string(),
        ];
        if !req.tools {
            // A real switch, unlike Codex and opencode, which only have a
            // read-only posture. `""` removes the whole built-in set.
            args.push("--tools".into());
            args.push(String::new());
        }
        if let Some(model) = &req.model {
            args.push("--model".into());
            args.push(model.clone());
        }
        if let Some(effort) = &req.effort {
            args.push("--effort".into());
            args.push(effort.clone());
        }
        if let Some(session) = &req.session {
            args.push("--resume".into());
            args.push(session.clone());
        }
        args
    }

    /// The bare prompt: the style already travels in `--append-system-prompt`.
    fn turn_input(&self, req: &TurnRequest) -> String {
        req.prompt.clone()
    }

    /// Claude has no working-directory flag; it uses the process cwd.
    fn cwd_is_process_cwd(&self) -> bool {
        true
    }

    fn parse_line(&self, line: &str, state: &mut TurnState) -> Option<TurnEvent> {
        let json: Value = serde_json::from_str(line).ok()?;
        match json.get("type").and_then(Value::as_str)? {
            "system" if json.get("subtype").and_then(Value::as_str) == Some("init") => {
                state.session = text_at(&json, "session_id");
                Some(TurnEvent::Started {
                    session: state.session.clone(),
                    model: text_at(&json, "model"),
                })
            }
            // Partial messages. A text delta renders now; `message_start` resets
            // `streamed` so the next whole message knows whether it is news.
            "stream_event" => {
                let event = json.get("event")?;
                match event.get("type").and_then(Value::as_str)? {
                    "message_start" => {
                        state.streamed = false;
                        None
                    }
                    "content_block_delta" => {
                        let delta = event.get("delta")?;
                        if delta.get("type").and_then(Value::as_str) != Some("text_delta") {
                            return None;
                        }
                        let text = text_at(delta, "text")?;
                        state.streamed = true;
                        Some(TurnEvent::Text { delta: text })
                    }
                    _ => None,
                }
            }
            // Text blocks arrive whole, one per assistant message: rendered only
            // when no delta streamed them, else the answer doubles. Thinking
            // blocks sit in the same array and are dropped.
            "assistant" if state.streamed => None,
            "assistant" => {
                let blocks = json.pointer("/message/content")?.as_array()?;
                let delta: String = blocks
                    .iter()
                    .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                    .filter_map(|b| b.get("text").and_then(Value::as_str))
                    .collect();
                (!delta.is_empty()).then_some(TurnEvent::Text { delta })
            }
            // The result line repeats the whole answer, which is already on
            // screen. Only its error form is news.
            "result" if json.get("is_error").and_then(Value::as_bool) == Some(true) => {
                Some(TurnEvent::agent_error(
                    LABEL,
                    text_at(&json, "result")
                        .unwrap_or_else(|| "Claude Code stopped with an error.".into()),
                ))
            }
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

/// Installed, but the Sign-in state could not be established.
///
/// `Warning` rather than `Error`: `!c` may still work, and T3 Code draws the same
/// distinction between "would not answer" and "said no".
fn unverified(version: Option<String>, message: &str) -> Snapshot {
    Snapshot {
        kind: AgentKind::Claude,
        label: LABEL,
        binary: BINARY,
        installed: true,
        version,
        health: Health::Warning,
        sign_in: SignIn::unknown(),
        message: Some(message.to_string()),
        efforts: ClaudeDriver.efforts(),
    }
}

/// Turn one `claude auth status --json` payload into a Snapshot.
pub fn snapshot_from_auth(version: Option<String>, json: &Value) -> Snapshot {
    let logged_in = json
        .get("loggedIn")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !logged_in {
        return Snapshot {
            kind: AgentKind::Claude,
            label: LABEL,
            binary: BINARY,
            installed: true,
            version,
            health: Health::Error,
            sign_in: SignIn::out(),
            message: Some("Claude Code is not authenticated. Run `claude auth login`.".into()),
            efforts: ClaudeDriver.efforts(),
        };
    }

    let label = auth_label(
        text_at(json, "subscriptionType").as_deref(),
        text_at(json, "authMethod").as_deref(),
    )
    .or_else(|| api_provider_label(text_at(json, "apiProvider").as_deref()));

    Snapshot {
        kind: AgentKind::Claude,
        label: LABEL,
        binary: BINARY,
        installed: true,
        version,
        health: Health::Ready,
        sign_in: SignIn {
            status: SignInStatus::In,
            label,
            account: text_at(json, "email"),
        },
        message: None,
        efforts: ClaudeDriver.efforts(),
    }
}

/// T3 Code's `claudeAuthMetadata`: an API key wins, then the subscription.
fn auth_label(subscription: Option<&str>, method: Option<&str>) -> Option<String> {
    if is_api_key(method) {
        return Some("Claude API Key".into());
    }
    subscription.map(subscription_auth_label)
}

fn is_api_key(method: Option<&str>) -> bool {
    matches!(
        method.map(normalise).as_deref(),
        Some("apikey") | Some("anthropicapikey") | Some("anthropicauthtoken")
    )
}

fn api_provider_label(provider: Option<&str>) -> Option<String> {
    (provider == Some("bedrock")).then(|| "Amazon Bedrock".into())
}

/// `pro` becomes `Claude Pro Subscription`, and the words are never doubled.
fn subscription_auth_label(subscription: &str) -> String {
    let label = subscription_label(subscription);
    let normalised = normalise(&label);
    match (
        normalised.starts_with("claude"),
        normalised.ends_with("subscription"),
    ) {
        (true, true) => label,
        (true, false) => format!("{label} Subscription"),
        (false, true) => format!("Claude {label}"),
        (false, false) => format!("Claude {label} Subscription"),
    }
}

fn subscription_label(subscription: &str) -> String {
    match normalise(subscription).as_str() {
        "claudemaxsubscription" | "max" | "maxplan" => "Max".into(),
        "claudemax5xsubscription" | "max5" => "Max 5x".into(),
        "claudemax20xsubscription" | "max20" => "Max 20x".into(),
        "claudeenterprisesubscription" | "enterprise" => "Enterprise".into(),
        "claudeteamsubscription" | "team" => "Team".into(),
        "claudeprosubscription" | "pro" => "Pro".into(),
        "claudefreesubscription" | "free" => "Free".into(),
        _ => title_case_words(subscription),
    }
}

fn normalise(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_' && *c != '-')
        .flat_map(char::to_lowercase)
        .collect()
}

fn title_case_words(value: &str) -> String {
    value
        .split(|c: char| c.is_whitespace() || c == '_' || c == '-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The real payload from `claude auth status --json` on a Pro account.
    #[test]
    fn v0_8_a_signed_in_claude_reports_its_plan_and_account() {
        let snap = snapshot_from_auth(
            Some("2.1.261".into()),
            &json!({
                "loggedIn": true,
                "authMethod": "claude.ai",
                "apiProvider": "firstParty",
                "email": "someone@example.com",
                "subscriptionType": "pro"
            }),
        );
        assert!(snap.installed);
        assert_eq!(snap.health, Health::Ready);
        assert_eq!(snap.sign_in.status, SignInStatus::In);
        assert_eq!(
            snap.sign_in.label.as_deref(),
            Some("Claude Pro Subscription")
        );
        assert_eq!(snap.sign_in.account.as_deref(), Some("someone@example.com"));
        assert!(snap.message.is_none());
    }

    /// Signed out is an Error with the command to run, never a silent card.
    #[test]
    fn v0_8_a_signed_out_claude_says_what_to_run() {
        let snap = snapshot_from_auth(None, &json!({ "loggedIn": false }));
        assert_eq!(snap.sign_in.status, SignInStatus::Out);
        assert_eq!(snap.health, Health::Error);
        assert!(snap.message.unwrap().contains("claude auth login"));
    }

    /// An API key beats the subscription, and Bedrock is read from apiProvider.
    #[test]
    fn v0_8_claude_account_labels_follow_t3_codes_rules() {
        assert_eq!(
            auth_label(Some("pro"), Some("apiKey")).as_deref(),
            Some("Claude API Key")
        );
        assert_eq!(
            auth_label(Some("max20"), None).as_deref(),
            Some("Claude Max 20x Subscription")
        );
        assert_eq!(
            auth_label(Some("claudeEnterpriseSubscription"), None).as_deref(),
            Some("Claude Enterprise Subscription")
        );
        assert_eq!(auth_label(None, None), None);
        assert_eq!(
            api_provider_label(Some("bedrock")).as_deref(),
            Some("Amazon Bedrock")
        );
        assert_eq!(api_provider_label(Some("firstParty")), None);
    }

    /// An unknown plan name is title-cased rather than dropped.
    #[test]
    fn v0_8_an_unknown_claude_plan_still_reads_as_english() {
        assert_eq!(
            subscription_auth_label("super_duper"),
            "Claude Super Duper Subscription"
        );
    }

    /// The init line is where the session id to resume comes from.
    #[test]
    fn v0_8_the_init_event_yields_the_session_to_resume() {
        let mut state = TurnState::default();
        let event = ClaudeDriver.parse_line(
            r#"{"type":"system","subtype":"init","session_id":"s-1","model":"claude-opus-5"}"#,
            &mut state,
        );
        assert_eq!(
            event,
            Some(TurnEvent::Started {
                session: Some("s-1".into()),
                model: Some("claude-opus-5".into())
            })
        );
        assert_eq!(state.session.as_deref(), Some("s-1"));
    }

    /// Text blocks come through; thinking blocks are dropped in the same message.
    #[test]
    fn v0_8_only_text_blocks_reach_the_palette() {
        let mut state = TurnState::default();
        let line = r#"{"type":"assistant","message":{"content":[
            {"type":"thinking","thinking":"secret"},
            {"type":"text","text":"Hello"}]}}"#;
        assert_eq!(
            ClaudeDriver.parse_line(line, &mut state),
            Some(TurnEvent::Text {
                delta: "Hello".into()
            })
        );

        let thinking_only =
            r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"x"}]}}"#;
        assert_eq!(ClaudeDriver.parse_line(thinking_only, &mut state), None);
    }

    /// `--include-partial-messages` on Claude 2.1, haiku, trimmed to the fields
    /// read: thinking block, two text deltas, then the whole text message.
    const PARTIAL_RUN: [&str; 8] = [
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","role":"assistant","content":[]}},"session_id":"s-1"}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}},"session_id":"s-1"}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"","estimated_tokens":null}},"session_id":"s-1"}"#,
        r#"{"type":"assistant","message":{"id":"msg_1","role":"assistant","content":[{"type":"thinking","thinking":"","signature":"Er"}]},"session_id":"s-1"}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"ok"}},"session_id":"s-1"}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":" then"}},"session_id":"s-1"}"#,
        r#"{"type":"assistant","message":{"id":"msg_1","role":"assistant","content":[{"type":"text","text":"ok then"}]},"session_id":"s-1"}"#,
        r#"{"type":"stream_event","event":{"type":"message_stop"},"session_id":"s-1"}"#,
    ];

    fn texts(lines: &[&str], state: &mut TurnState) -> Vec<String> {
        lines
            .iter()
            .filter_map(|line| match ClaudeDriver.parse_line(line, state) {
                Some(TurnEvent::Text { delta }) => Some(delta),
                _ => None,
            })
            .collect()
    }

    /// Deltas stream, and the whole message after them does not repeat them.
    #[test]
    fn v0_11_1_claude_deltas_stream_and_render_once() {
        let mut state = TurnState::default();
        assert_eq!(texts(&PARTIAL_RUN, &mut state), ["ok", " then"]);
    }

    /// A message with no deltas, a Claude ignoring the flag, still renders whole.
    #[test]
    fn v0_11_1_a_claude_message_without_deltas_falls_back_to_its_text() {
        let mut state = TurnState::default();
        let whole = [PARTIAL_RUN[0], PARTIAL_RUN[6]];
        assert_eq!(texts(&whole, &mut state), ["ok then"]);
    }

    /// `message_start` resets: a second message with no deltas is still news.
    #[test]
    fn v0_11_1_each_claude_message_decides_for_itself() {
        let mut state = TurnState::default();
        let second =
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"more"}]}}"#;
        let lines = [PARTIAL_RUN.as_slice(), &[PARTIAL_RUN[0], second]].concat();
        assert_eq!(texts(&lines, &mut state), ["ok", " then", "more"]);
    }

    /// The prompt is stdin, bare: the style is already the system prompt.
    #[test]
    fn v0_11_1_claude_reads_the_bare_prompt_from_stdin() {
        let req = TurnRequest {
            prompt: "line one\nline two".into(),
            cwd: std::path::PathBuf::from("."),
            session: None,
            model: None,
            effort: None,
            tools: false,
        };
        assert_eq!(ClaudeDriver.turn_input(&req), "line one\nline two");
        let args = ClaudeDriver.turn_args(&req);
        assert_eq!(args[0], "-p");
        assert!(args.contains(&"--include-partial-messages".to_string()));
    }

    /// The result line repeats the answer, so only its error form is news.
    #[test]
    fn v0_8_a_successful_result_line_is_not_re_rendered() {
        let mut state = TurnState::default();
        let ok = r#"{"type":"result","subtype":"success","is_error":false,"result":"Hello"}"#;
        assert_eq!(ClaudeDriver.parse_line(ok, &mut state), None);

        let bad = r#"{"type":"result","is_error":true,"result":"Credit balance too low"}"#;
        assert_eq!(
            ClaudeDriver.parse_line(bad, &mut state),
            Some(TurnEvent::agent_error(LABEL, "Credit balance too low"))
        );
    }

    /// A partial line must never render. `turn.rs` buffers, so the parser only
    /// has to refuse rather than guess.
    #[test]
    fn v0_8_a_half_line_parses_to_nothing_rather_than_to_text() {
        let mut state = TurnState::default();
        assert_eq!(
            ClaudeDriver.parse_line(r#"{"type":"assis"#, &mut state),
            None
        );
        assert_eq!(ClaudeDriver.parse_line("not json at all", &mut state), None);
    }

    /// The first Turn must carry `--tools ""`; a follow-up must not.
    #[test]
    fn v0_8_the_inline_path_disables_claudes_tools() {
        let base = TurnRequest {
            prompt: "hi".into(),
            cwd: std::path::PathBuf::from("."),
            session: None,
            model: None,
            effort: None,
            tools: false,
        };
        let args = ClaudeDriver.turn_args(&base);
        let tools = args
            .iter()
            .position(|a| a == "--tools")
            .expect("tools flag");
        assert_eq!(args[tools + 1], "");
        assert!(args.contains(&"stream-json".to_string()));
        assert!(args.contains(&"--verbose".to_string()));

        let with_tools = ClaudeDriver.turn_args(&TurnRequest {
            tools: true,
            ..base.clone()
        });
        assert!(!with_tools.contains(&"--tools".to_string()));
    }

    /// Model and effort are both flags on Claude, and both are sent when locked.
    #[test]
    fn v0_8_claude_sends_the_locked_model_and_effort() {
        let args = ClaudeDriver.turn_args(&TurnRequest {
            prompt: "hi".into(),
            cwd: std::path::PathBuf::from("."),
            session: None,
            model: Some("opus".into()),
            effort: Some("high".into()),
            tools: false,
        });
        let model = args
            .iter()
            .position(|a| a == "--model")
            .expect("model flag");
        assert_eq!(args[model + 1], "opus");
        let effort = args
            .iter()
            .position(|a| a == "--effort")
            .expect("effort flag");
        assert_eq!(args[effort + 1], "high");
    }

    /// Nothing locked means no flag at all, which is the Agent's own default —
    /// never a guess of ours.
    #[test]
    fn v0_8_an_unlocked_model_sends_no_flag() {
        let args = ClaudeDriver.turn_args(&TurnRequest {
            prompt: "hi".into(),
            cwd: std::path::PathBuf::from("."),
            session: None,
            model: None,
            effort: None,
            tools: false,
        });
        assert!(!args.contains(&"--model".to_string()));
        assert!(!args.contains(&"--effort".to_string()));
    }

    /// A follow-up resumes rather than starting over, or a conversation has no
    /// memory between Turns.
    #[test]
    fn v0_8_a_follow_up_resumes_the_session() {
        let args = ClaudeDriver.turn_args(&TurnRequest {
            prompt: "and then?".into(),
            cwd: std::path::PathBuf::from("."),
            session: Some("s-1".into()),
            model: Some("opus".into()),
            effort: None,
            tools: true,
        });
        let resume = args
            .iter()
            .position(|a| a == "--resume")
            .expect("resume flag");
        assert_eq!(args[resume + 1], "s-1");
        let model = args
            .iter()
            .position(|a| a == "--model")
            .expect("model flag");
        assert_eq!(args[model + 1], "opus");
    }
}
