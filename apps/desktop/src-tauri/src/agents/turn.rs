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

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::job::{self, Job};
use super::{probe, AgentDriver, TurnRequest, TurnState};

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

/// Every Turn currently running, and the gate that says whether one may.
#[derive(Default)]
pub struct Turns {
    running: Mutex<HashMap<u64, Arc<Slot>>>,
    /// False while the Palette is hidden. Defaults hidden: tests set it first.
    visible: AtomicBool,
}

impl Turns {
    /// Spawn a Turn and stream it. Returns immediately; events arrive later.
    ///
    /// Never blocks the caller: a `#[tauri::command]` runs on the main thread and
    /// an Agent takes seconds to answer.
    pub fn start(
        self: &Arc<Self>,
        app: AppHandle,
        turn_id: u64,
        driver: Box<dyn AgentDriver>,
        req: TurnRequest,
    ) {
        self.start_with(turn_id, driver, None, req, move |id, event| {
            emit(&app, id, event)
        });
    }

    /// `start`, with the executable and the event sink supplied by the caller.
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
        if !self.is_visible() {
            return None;
        }
        let slot = Arc::new(Slot::default());
        self.running
            .lock()
            .expect("turns mutex")
            .insert(turn_id, slot.clone());
        let turns = self.clone();
        Some(std::thread::spawn(move || {
            let last = turns.run(&slot, driver.as_ref(), exe, &req, &|event| {
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

    /// Run one Turn to its end. Returns the closing event, or `None` when there
    /// is nothing to add: cancelled, or the Agent already reported its failure.
    fn run(
        &self,
        slot: &Slot,
        driver: &dyn AgentDriver,
        exe: Option<PathBuf>,
        req: &TurnRequest,
        emit: &dyn Fn(TurnEvent),
    ) -> Option<TurnEvent> {
        let agent = driver.label().to_string();
        let failed = |failure| Some(TurnEvent::Failed { failure });
        let Some(exe) = exe.or_else(|| probe::resolve(driver.binary())) else {
            return failed(TurnFailure::NotFound {
                agent,
                binary: driver.binary().to_string(),
            });
        };
        if slot.is_cancelled() {
            return None;
        }

        let Spawned { mut child, job } = match spawn(driver, &exe, req) {
            Ok(spawned) => spawned,
            Err(e) => {
                return failed(TurnFailure::SpawnFailed {
                    agent,
                    detail: e.to_string(),
                })
            }
        };
        if !slot.attach(job.clone()) {
            let _ = child.wait();
            return None;
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
        let (mut texted, mut reported) = (false, false);
        if let Some(stdout) = child.stdout.take() {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if line.trim().is_empty() {
                    continue;
                }
                if let Some(event) = driver.parse_line(&line, &mut state) {
                    texted |= matches!(event, TurnEvent::Text { .. });
                    reported |= matches!(event, TurnEvent::Failed { .. });
                    emit(event);
                }
            }
        }

        let status = child.wait().ok();
        // Anything the Agent left behind dies now, and lets go of stderr.
        job.terminate();
        if let Some(drain) = drain {
            let _ = drain.join();
        }
        if reported || slot.is_cancelled() {
            return None;
        }

        let code = status.and_then(|s| s.code());
        if status.is_some_and(|s| s.success()) {
            if !texted {
                return failed(TurnFailure::Silent { agent });
            }
            return Some(TurnEvent::Done {
                session: state.session,
            });
        }
        let tail = tail.lock().expect("stderr mutex");
        let detail = last_chars(tail.trim(), DETAIL_MAX_CHARS);
        if cfg!(windows) && launcher_broken(code, &detail) {
            return failed(TurnFailure::LauncherBroken {
                agent,
                binary: exe.to_string_lossy().to_string(),
                detail,
            });
        }
        failed(TurnFailure::Exited {
            agent,
            code,
            detail,
        })
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
