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
use super::{AgentDriver, AgentKind, Health, SignIn, SignInStatus, Snapshot, TurnRequest, TurnState};

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
            "-p".into(),
            req.prompt.clone(),
            "--output-format".into(),
            "stream-json".into(),
            // stream-json refuses to emit without it, rather than warning.
            "--verbose".into(),
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
            // Text blocks arrive whole, one per assistant message. Thinking
            // blocks are in the same array and are deliberately dropped.
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
                Some(TurnEvent::Failed {
                    message: text_at(&json, "result")
                        .unwrap_or_else(|| "Claude Code stopped with an error.".into()),
                })
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
    let logged_in = json.get("loggedIn").and_then(Value::as_bool).unwrap_or(false);
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
                Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
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
        assert_eq!(snap.sign_in.label.as_deref(), Some("Claude Pro Subscription"));
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
        assert_eq!(auth_label(Some("pro"), Some("apiKey")).as_deref(), Some("Claude API Key"));
        assert_eq!(auth_label(Some("max20"), None).as_deref(), Some("Claude Max 20x Subscription"));
        assert_eq!(
            auth_label(Some("claudeEnterpriseSubscription"), None).as_deref(),
            Some("Claude Enterprise Subscription")
        );
        assert_eq!(auth_label(None, None), None);
        assert_eq!(api_provider_label(Some("bedrock")).as_deref(), Some("Amazon Bedrock"));
        assert_eq!(api_provider_label(Some("firstParty")), None);
    }

    /// An unknown plan name is title-cased rather than dropped.
    #[test]
    fn v0_8_an_unknown_claude_plan_still_reads_as_english() {
        assert_eq!(subscription_auth_label("super_duper"), "Claude Super Duper Subscription");
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
            Some(TurnEvent::Text { delta: "Hello".into() })
        );

        let thinking_only =
            r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"x"}]}}"#;
        assert_eq!(ClaudeDriver.parse_line(thinking_only, &mut state), None);
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
            Some(TurnEvent::Failed { message: "Credit balance too low".into() })
        );
    }

    /// A partial line must never render. `turn.rs` buffers, so the parser only
    /// has to refuse rather than guess.
    #[test]
    fn v0_8_a_half_line_parses_to_nothing_rather_than_to_text() {
        let mut state = TurnState::default();
        assert_eq!(ClaudeDriver.parse_line(r#"{"type":"assis"#, &mut state), None);
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
        let tools = args.iter().position(|a| a == "--tools").expect("tools flag");
        assert_eq!(args[tools + 1], "");
        assert!(args.contains(&"stream-json".to_string()));
        assert!(args.contains(&"--verbose".to_string()));

        let with_tools = ClaudeDriver.turn_args(&TurnRequest { tools: true, ..base.clone() });
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
        let model = args.iter().position(|a| a == "--model").expect("model flag");
        assert_eq!(args[model + 1], "opus");
        let effort = args.iter().position(|a| a == "--effort").expect("effort flag");
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
        let resume = args.iter().position(|a| a == "--resume").expect("resume flag");
        assert_eq!(args[resume + 1], "s-1");
        let model = args.iter().position(|a| a == "--model").expect("model flag");
        assert_eq!(args[model + 1], "opus");
    }
}
