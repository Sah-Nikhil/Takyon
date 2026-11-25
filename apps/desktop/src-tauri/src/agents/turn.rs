//! Running one Turn and streaming it to the Palette.
//!
//! Spawned on its own thread inside a `job`, prompt written to stdin (ADR-0032),
//! stdout read **line by line**, each complete line to the driver's parser.
//! Buffering to the newline is the trick: a half-parsed event rendered as text
//! puts a fragment of JSON where the answer should be.
//!
//! A Turn runs only while the Palette is shown (ADR-0033). `window::hide` clears
//! the gate and kills every job in Rust, so dismissal never waits on React.
//! Rust reports failures as facts (`TurnFailure`); TypeScript owns the copy.
//! A Turn is a chain of Attempts: one that fails before any text hands over to
//! the next Agent in the order (ADR-0041).

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::fallback::Bench;
use super::job::{self, Job};
use super::{probe, AgentDriver, AgentKind, TurnRequest, TurnState};

/// The event every Turn streams over. One channel, `turnId` discriminates.
pub const EVENT_TURN: &str = "takyon://turn";

/// Most of stderr a failure carries. The tail: the last lines say what broke.
pub const DETAIL_MAX_CHARS: usize = 2_000;

/// One thing that happened during a Turn.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum TurnEvent {
    /// The Agent accepted the Turn. `session` is what a follow-up resumes.
    Started {
        session: Option<String>,
        model: Option<String>,
    },
    /// More answer text. Deltas, appended in arrival order.
    Text { delta: String },
    /// The Turn finished. `session` is repeated here because Claude only reports
    /// it on the first event and a follow-up needs it to resume.
    Done { session: Option<String> },
    /// The Turn ended without an answer. Facts only; copy is `turnFailureCopy`.
    Failed {
        #[serde(flatten)]
        failure: TurnFailure,
    },
    /// `failure.agent` could not answer and wrote nothing; `to` is asked next.
    /// Any session a `started` gave belongs to the Agent that failed: drop it.
    Fallback {
        to: AgentKind,
        label: String,
        #[serde(flatten)]
        failure: TurnFailure,
    },
}

impl TurnEvent {
    /// The Agent's own error event, its words unedited.
    pub fn agent_error(agent: &str, message: impl Into<String>) -> TurnEvent {
        TurnEvent::Failed {
            failure: TurnFailure::AgentError {
                agent: agent.to_string(),
                message: message.into(),
            },
        }
    }
}

/// Why a Turn failed. `agent` is the display label, `binary` the command or path.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "reason")]
pub enum TurnFailure {
    /// `probe::resolve` found nothing.
    NotFound { agent: String, binary: String },
    /// Exit 9009 or "is not recognized": a `.cmd` shim whose `node` is gone.
    LauncherBroken {
        agent: String,
        binary: String,
        detail: String,
    },
    /// `spawn` returned an `io::Error`, or the job could not be set up.
    SpawnFailed { agent: String, detail: String },
    /// The driver's parser saw the Agent's own error event.
    AgentError { agent: String, message: String },
    /// Non-zero exit. `detail` is stderr's tail, capped at `DETAIL_MAX_CHARS`.
    Exited {
        agent: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<i32>,
        detail: String,
    },
    /// Exited cleanly having produced no text.
    Silent { agent: String },
    /// Every Agent in a chain of two or more failed. `last` says why the final
    /// one did; the earlier ones each had a `fallback` event of their own.
    Exhausted {
        tried: Vec<String>,
        last: Box<TurnFailure>,
    },
}

/// What the frontend receives: an event plus the Turn it belongs to.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    turn_id: u64,
    #[serde(flatten)]
    event: TurnEvent,
}

/// One started Turn process: the child with its pipes, and its tree's killer.
pub struct Spawned {
    pub child: Child,
    pub job: Arc<Job>,
}

/// Spawn one Turn inside a job, write its input on a thread, close stdin.
///
/// The only Turn spawn path: `Turns::run` and the integration tests share it.
pub fn spawn(driver: &dyn AgentDriver, exe: &Path, req: &TurnRequest) -> std::io::Result<Spawned> {
    let mut command = probe::command(exe);
    command
        .args(driver.turn_args(req))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if driver.cwd_is_process_cwd() {
        command.current_dir(&req.cwd);
    }
    let (mut child, job) = job::spawn(&mut command)?;
    if let Some(stdin) = child.stdin.take() {
        probe::write_input(stdin, driver.turn_input(req));
    }
    Ok(Spawned {
        child,
        job: Arc::new(job),
    })
}

/// A registered Turn: its job once spawned, and whether it was cancelled.
#[derive(Default)]
struct Slot {
    job: Mutex<Option<Arc<Job>>>,
    cancelled: AtomicBool,
}

impl Slot {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(job) = self.job.lock().expect("slot mutex").as_ref() {
            job.terminate();
        }
    }

    /// Hand over the job. False, with the tree killed, when already cancelled.
    /// A fallback attaches again: the failed Attempt's job is already dead.
    fn attach(&self, job: Arc<Job>) -> bool {
        let mut held = self.job.lock().expect("slot mutex");
        if self.is_cancelled() {
            job.terminate();
            return false;
        }
        *held = Some(job);
        true
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// One Agent a Turn may be answered by. `exe` overrides resolution, for tests.
pub struct Attempt {
    pub driver: Box<dyn AgentDriver>,
    pub exe: Option<PathBuf>,
    pub req: TurnRequest,
}

/// How one Attempt ended, before the chain decides what the Palette hears.
enum End {
    Done(Option<String>),
    Failed(TurnFailure),
    Cancelled,
}

/// One Attempt's end, plus what the chain needs to judge it.
struct Ran {
    end: End,
    /// Text reached the Palette. Past this point no fallback: it would restart
    /// an answer the user is already reading.
    texted: bool,
    /// The Agent's own usage-window reset, Unix seconds, if it reported one.
    resets_at: Option<u64>,
}

/// Every Turn currently running, and the gate that says whether one may.
pub struct Turns {
    running: Mutex<HashMap<u64, Arc<Slot>>>,
    /// False while the Palette is hidden. Defaults hidden: tests set it first.
    visible: AtomicBool,
    /// Agents that failed recently. Shared with the Palette in production.
    bench: Arc<Bench>,
}

/// A private bench, so a test's failed Turn cannot reorder another test's `!c`.
impl Default for Turns {
    fn default() -> Self {
        Turns::with_bench(Arc::new(Bench::default()))
    }
}

impl Turns {
    pub fn with_bench(bench: Arc<Bench>) -> Self {
        Turns {
            running: Mutex::default(),
            visible: AtomicBool::new(false),
            bench,
        }
    }

    pub fn bench(&self) -> &Arc<Bench> {
        &self.bench
    }

    /// Spawn a Turn and stream it. Returns immediately; events arrive later.
    ///
    /// Never blocks the caller: a `#[tauri::command]` runs on the main thread and
    /// an Agent takes seconds to answer.
    pub fn start(self: &Arc<Self>, app: AppHandle, turn_id: u64, attempts: Vec<Attempt>) {
        self.start_chain_with(turn_id, attempts, move |id, event| emit(&app, id, event));
    }

    /// One Agent, no fallback, with the executable and the event sink supplied.
    ///
    /// `None` when refused because the Palette is hidden; refusal emits nothing.
    pub fn start_with(
        self: &Arc<Self>,
        turn_id: u64,
        driver: Box<dyn AgentDriver>,
        exe: Option<PathBuf>,
        req: TurnRequest,
        sink: impl Fn(u64, TurnEvent) + Send + 'static,
    ) -> Option<JoinHandle<()>> {
        self.start_chain_with(turn_id, vec![Attempt { driver, exe, req }], sink)
    }

    /// Walk `attempts` until one answers. `None` when the Palette is hidden.
    pub fn start_chain_with(
        self: &Arc<Self>,
        turn_id: u64,
        attempts: Vec<Attempt>,
        sink: impl Fn(u64, TurnEvent) + Send + 'static,
    ) -> Option<JoinHandle<()>> {
        if !self.is_visible() || attempts.is_empty() {
            return None;
        }
        let slot = Arc::new(Slot::default());
        self.running
            .lock()
            .expect("turns mutex")
            .insert(turn_id, slot.clone());
        let turns = self.clone();
        Some(std::thread::spawn(move || {
            let last = turns.walk(&slot, attempts, &|event| {
                if !slot.is_cancelled() {
                    sink(turn_id, event)
                }
            });
            turns.running.lock().expect("turns mutex").remove(&turn_id);
            // Cancellation emits nothing: nobody is reading.
            if let (Some(event), false) = (last, slot.is_cancelled()) {
                sink(turn_id, event);
            }
        }))
    }

    /// Open or close the gate. Closing kills every Turn: `window::hide` calls it.
    pub fn set_visible(&self, visible: bool) {
        self.visible.store(visible, Ordering::SeqCst);
        if !visible {
            self.cancel_all();
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible.load(Ordering::SeqCst)
    }

    /// Kill a running Turn's whole tree. Silent when it has already finished.
    pub fn cancel(&self, turn_id: u64) {
        let slot = self.running.lock().expect("turns mutex").remove(&turn_id);
        if let Some(slot) = slot {
            slot.cancel();
        }
    }

    /// Kill every Turn. Terminates and returns; each Turn's thread reaps its own.
    pub fn cancel_all(&self) {
        let slots: Vec<_> = self
            .running
            .lock()
            .expect("turns mutex")
            .drain()
            .map(|(_, slot)| slot)
            .collect();
        for slot in slots {
            slot.cancel();
        }
    }

    /// The job behind a running Turn, once spawned. For tests that count processes.
    pub fn job(&self, turn_id: u64) -> Option<Arc<Job>> {
        let running = self.running.lock().expect("turns mutex");
        let slot = running.get(&turn_id)?;
        let job = slot.job.lock().expect("slot mutex").clone();
        job
    }

    /// Run Attempts in order until one answers. Returns the closing event.
    ///
    /// A failure with no text behind it benches that Agent and, if another is
    /// left, emits `fallback` and asks it. Text means no fallback, ever.
    fn walk(
        &self,
        slot: &Slot,
        attempts: Vec<Attempt>,
        emit: &dyn Fn(TurnEvent),
    ) -> Option<TurnEvent> {
        let chained = attempts.len() > 1;
        let mut tried = Vec::with_capacity(attempts.len());
        let mut attempts = attempts.into_iter().peekable();
        while let Some(attempt) = attempts.next() {
            let kind = attempt.driver.kind();
            tried.push(attempt.driver.label().to_string());
            let ran = self.run(
                slot,
                attempt.driver.as_ref(),
                attempt.exe,
                &attempt.req,
                emit,
            );
            let failure = match ran.end {
                End::Cancelled => return None,
                End::Done(session) => {
                    self.bench.clear(kind);
                    return Some(TurnEvent::Done { session });
                }
                End::Failed(failure) => failure,
            };
            if ran.texted {
                return Some(TurnEvent::Failed { failure });
            }
            self.bench
                .bench(kind, ran.resets_at, std::time::Instant::now());
            match attempts.peek() {
                Some(_) if slot.is_cancelled() => return None,
                Some(next) => emit(TurnEvent::Fallback {
                    to: next.driver.kind(),
                    label: next.driver.label().to_string(),
                    failure,
                }),
                None if chained => {
                    return Some(TurnEvent::Failed {
                        failure: TurnFailure::Exhausted {
                            tried,
                            last: Box::new(failure),
                        },
                    })
                }
                None => return Some(TurnEvent::Failed { failure }),
            }
        }
        None
    }

    /// Run one Attempt to its end. Failures are returned, never emitted: only
    /// the chain knows whether another Agent will be asked.
    fn run(
        &self,
        slot: &Slot,
        driver: &dyn AgentDriver,
        exe: Option<PathBuf>,
        req: &TurnRequest,
        emit: &dyn Fn(TurnEvent),
    ) -> Ran {
        let agent = driver.label().to_string();
        let ran = |end, texted, resets_at| Ran {
            end,
            texted,
            resets_at,
        };
        let Some(exe) = exe.or_else(|| probe::resolve(driver.binary())) else {
            let failure = TurnFailure::NotFound {
                agent,
                binary: driver.binary().to_string(),
            };
            return ran(End::Failed(failure), false, None);
        };
        if slot.is_cancelled() {
            return ran(End::Cancelled, false, None);
        }

        let Spawned { mut child, job } = match spawn(driver, &exe, req) {
            Ok(spawned) => spawned,
            Err(e) => {
                let failure = TurnFailure::SpawnFailed {
                    agent,
                    detail: e.to_string(),
                };
                return ran(End::Failed(failure), false, None);
            }
        };
        if !slot.attach(job.clone()) {
            let _ = child.wait();
            return ran(End::Cancelled, false, None);
        }
        // A hide between `start`'s check and here: `cancel_all` may have missed us.
        if !self.is_visible() {
            slot.cancel();
        }

        // Drained on its own thread. Left unread it fills, and the child blocks
        // writing to it while we wait for stdout that will never come.
        let tail = Arc::new(Mutex::new(String::new()));
        let drain = child.stderr.take().map(|stderr| {
            let tail = tail.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    let mut tail = tail.lock().expect("stderr mutex");
                    tail.push_str(&line);
                    tail.push('\n');
                }
            })
        });

        let mut state = TurnState::default();
        let mut texted = false;
        // The Agent's own error event. First one wins: it is the cause, and the
        // exit code after it is the consequence.
        let mut reported: Option<TurnFailure> = None;
        if let Some(stdout) = child.stdout.take() {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if line.trim().is_empty() {
                    continue;
                }
                match driver.parse_line(&line, &mut state) {
                    Some(TurnEvent::Failed { failure }) => {
                        reported.get_or_insert(failure);
                    }
                    Some(event) => {
                        texted |= matches!(event, TurnEvent::Text { .. });
                        emit(event);
                    }
                    None => {}
                }
            }
        }

        let status = child.wait().ok();
        // Anything the Agent left behind dies now, and lets go of stderr.
        job.terminate();
        if let Some(drain) = drain {
            let _ = drain.join();
        }
        let resets_at = state.resets_at;
        if slot.is_cancelled() {
            return ran(End::Cancelled, texted, resets_at);
        }
        if let Some(failure) = reported {
            return ran(End::Failed(failure), texted, resets_at);
        }

        let code = status.and_then(|s| s.code());
        if status.is_some_and(|s| s.success()) {
            if !texted {
                return ran(End::Failed(TurnFailure::Silent { agent }), false, resets_at);
            }
            return ran(End::Done(state.session), true, resets_at);
        }
        let tail = tail.lock().expect("stderr mutex");
        let detail = last_chars(tail.trim(), DETAIL_MAX_CHARS);
        let failure = if cfg!(windows) && launcher_broken(code, &detail) {
            TurnFailure::LauncherBroken {
                agent,
                binary: exe.to_string_lossy().to_string(),
                detail,
            }
        } else {
            TurnFailure::Exited {
                agent,
                code,
                detail,
            }
        };
        ran(End::Failed(failure), texted, resets_at)
    }
}

/// t3code's `WINDOWS_COMMAND_NOT_FOUND_PATTERNS`. `.` is any one character,
/// which also absorbs an OEM code page decoded lossily.
const NOT_RECOGNIZED: [&str; 6] = [
    "is not recognized as an internal or external command",
    "n.o . reconhecido como um comando interno",
    "non . riconosciuto come comando interno o esterno",
    "n.est pas reconnu en tant que commande interne",
    "no se reconoce como un comando interno o externo",
    "wird nicht als interner oder externer befehl",
];

/// Whether a `.cmd` failed because a command inside it does not exist.
pub fn launcher_broken(code: Option<i32>, stderr: &str) -> bool {
    if code == Some(9009) {
        return true;
    }
    let haystack: Vec<char> = stderr.to_lowercase().chars().collect();
    NOT_RECOGNIZED.iter().any(|pattern| {
        let needle: Vec<char> = pattern.chars().collect();
        haystack.windows(needle.len()).any(|window| {
            window
                .iter()
                .zip(&needle)
                .all(|(have, want)| *want == '.' || have == want)
        })
    })
}

/// The last `max` characters of `text`, on a char boundary.
fn last_chars(text: &str, max: usize) -> String {
    let count = text.chars().count();
    text.chars().skip(count.saturating_sub(max)).collect()
}

fn emit(app: &AppHandle, turn_id: u64, event: TurnEvent) {
    let _ = app.emit(EVENT_TURN, Envelope { turn_id, event });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire shape the frontend switches on. A rename here is an IPC break.
    #[test]
    fn v0_8_turn_events_serialise_with_a_kind_tag() {
        let json = serde_json::to_string(&TurnEvent::Text { delta: "hi".into() }).unwrap();
        assert_eq!(json, r#"{"kind":"text","delta":"hi"}"#);

        let started = serde_json::to_string(&TurnEvent::Started {
            session: Some("abc".into()),
            model: None,
        })
        .unwrap();
        assert!(started.contains(r#""kind":"started""#));
        assert!(started.contains(r#""session":"abc""#));
    }

    /// The envelope flattens, so the frontend reads one object rather than two.
    #[test]
    fn v0_8_an_envelope_carries_the_turn_id_beside_the_event() {
        let json = serde_json::to_string(&Envelope {
            turn_id: 7,
            event: TurnEvent::Done { session: None },
        })
        .unwrap();
        assert!(json.contains(r#""turnId":7"#));
        assert!(json.contains(r#""kind":"done""#));
    }

    /// A failure flattens beside `kind`: `packages/shared/src/ipc.ts` reads one
    /// object with `reason`, never a nested `failure`.
    #[test]
    fn v0_11_1_a_failure_serialises_flat_with_a_reason() {
        let json = serde_json::to_value(Envelope {
            turn_id: 3,
            event: TurnEvent::Failed {
                failure: TurnFailure::LauncherBroken {
                    agent: "opencode".into(),
                    binary: r"C:\npm\opencode.cmd".into(),
                    detail: "'node' is not recognized".into(),
                },
            },
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "turnId": 3,
                "kind": "failed",
                "reason": "launcherBroken",
                "agent": "opencode",
                "binary": r"C:\npm\opencode.cmd",
                "detail": "'node' is not recognized",
            })
        );

        let exited = serde_json::to_value(TurnEvent::Failed {
            failure: TurnFailure::Exited {
                agent: "Codex".into(),
                code: None,
                detail: String::new(),
            },
        })
        .unwrap();
        assert_eq!(exited["reason"], "exited");
        assert!(
            exited.get("code").is_none(),
            "absent code is omitted, not null"
        );

        for (failure, reason) in [
            (
                TurnFailure::NotFound {
                    agent: "a".into(),
                    binary: "b".into(),
                },
                "notFound",
            ),
            (
                TurnFailure::SpawnFailed {
                    agent: "a".into(),
                    detail: "d".into(),
                },
                "spawnFailed",
            ),
            (
                TurnFailure::AgentError {
                    agent: "a".into(),
                    message: "m".into(),
                },
                "agentError",
            ),
            (TurnFailure::Silent { agent: "a".into() }, "silent"),
        ] {
            let json = serde_json::to_value(TurnEvent::Failed { failure }).unwrap();
            assert_eq!(json["reason"], reason);
        }
    }

    /// The two ADR-0041 shapes exactly as `packages/shared/src/ipc.ts` reads
    /// them: `fallback` flat beside the failed Agent's facts, `exhausted` with
    /// no `agent` and its last failure nested whole.
    #[test]
    fn v0_11_2_fallback_and_exhausted_serialise_as_the_frontend_reads_them() {
        let missing = TurnFailure::NotFound {
            agent: "Claude Code".into(),
            binary: "claude".into(),
        };
        let fallback = serde_json::to_value(Envelope {
            turn_id: 4,
            event: TurnEvent::Fallback {
                to: AgentKind::Codex,
                label: "Codex".into(),
                failure: missing.clone(),
            },
        })
        .unwrap();
        assert_eq!(
            fallback,
            serde_json::json!({
                "turnId": 4,
                "kind": "fallback",
                "to": "codex",
                "label": "Codex",
                "reason": "notFound",
                "agent": "Claude Code",
                "binary": "claude",
            })
        );

        let exhausted = serde_json::to_value(TurnEvent::Failed {
            failure: TurnFailure::Exhausted {
                tried: vec!["Claude Code".into(), "Codex".into()],
                last: Box::new(missing),
            },
        })
        .unwrap();
        assert_eq!(
            exhausted,
            serde_json::json!({
                "kind": "failed",
                "reason": "exhausted",
                "tried": ["Claude Code", "Codex"],
                "last": { "reason": "notFound", "agent": "Claude Code", "binary": "claude" },
            })
        );
    }

    /// 9009 and each of t3code's six localised sentences mean the launcher broke.
    #[test]
    fn v0_11_1_a_missing_command_inside_a_shim_is_a_broken_launcher() {
        assert!(launcher_broken(Some(9009), ""));
        for stderr in [
            "'node' is not recognized as an internal or external command,\noperable program or batch file.",
            "'node' não é reconhecido como um comando interno\nou externo",
            "'node' non è riconosciuto come comando interno o esterno,",
            "'node' n'est pas reconnu en tant que commande interne\nou externe",
            "\"node\" no se reconoce como un comando interno o externo,",
            "\"node\" wird nicht als interner oder externer Befehl,\nbetriebsfähiges Programm oder Batch-Datei erkannt.",
        ] {
            assert!(launcher_broken(Some(1), stderr), "{stderr}");
        }
        assert!(!launcher_broken(Some(1), "rate limited"));
        assert!(!launcher_broken(None, ""));
    }

    /// stderr's tail is what a failure carries, never a split character.
    #[test]
    fn v0_11_1_a_failure_detail_keeps_the_tail() {
        let long = format!("{}é-end", "x".repeat(3_000));
        let kept = last_chars(&long, DETAIL_MAX_CHARS);
        assert_eq!(kept.chars().count(), DETAIL_MAX_CHARS);
        assert!(kept.ends_with("é-end"));
    }

    /// Cancelling a Turn that never ran must be silent, not a panic — the Palette
    /// can dismiss between spawn and registration.
    #[test]
    fn v0_8_cancelling_an_unknown_turn_does_nothing() {
        let turns = Turns::default();
        turns.cancel(404);
        turns.cancel_all();
    }

    /// While the Palette is hidden no Turn starts, and refusal emits nothing.
    #[test]
    fn v0_11_1_a_hidden_palette_refuses_turns() {
        let turns = Arc::new(Turns::default());
        assert!(!turns.is_visible(), "the gate defaults to hidden");
        let (tx, rx) = std::sync::mpsc::channel();
        let started = turns.start_with(
            1,
            super::super::driver_for(super::super::AgentKind::Claude).unwrap(),
            Some(PathBuf::from("takyon-never-spawned")),
            TurnRequest {
                prompt: "hi".into(),
                cwd: PathBuf::from("."),
                session: None,
                model: None,
                effort: None,
                tools: false,
            },
            move |id, event| {
                let _ = tx.send((id, event));
            },
        );
        assert!(started.is_none());
        assert!(rx.try_recv().is_err());
        assert!(turns.running.lock().unwrap().is_empty());
    }
}
