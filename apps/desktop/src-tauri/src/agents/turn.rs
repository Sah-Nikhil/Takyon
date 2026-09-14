//! Running one Turn and streaming it to the Palette.
//!
//! The subprocess is spawned on its own thread, stdout is read **line by line**,
//! and each complete line goes to the driver's parser. Buffering to the newline
//! is the whole trick: a half-parsed event rendered as text puts a fragment of
//! JSON where the answer should be, which is this shape of code's famous bug.
//!
//! Nothing here stops a Turn but `cancel`. Escape over a conversation goes back
//! one step; the frontend cancels only when the view is actually unmounted.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::{AgentDriver, TurnRequest, TurnState};

/// The event every Turn streams over. One channel, `turnId` discriminates.
pub const EVENT_TURN: &str = "takyon://turn";

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
    /// The Turn ended without an answer. `message` is shown as written.
    Failed { message: String },
}

/// What the frontend receives: an event plus the Turn it belongs to.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    turn_id: u64,
    #[serde(flatten)]
    event: TurnEvent,
}

/// Every Turn currently running, so `cancel` has something to kill.
///
/// `visible` gates spawning: a Turn may only run while the Palette is shown.
/// `window::show` sets it; `window::hide` clears it and calls `cancel_all`.
#[derive(Default)]
pub struct Turns {
    running: Mutex<HashMap<u64, Arc<Mutex<Child>>>>,
    /// False while the Palette is hidden. Prevents races where a Turn starts
    /// after the Palette has been dismissed.
    visible: AtomicBool,
}

impl Turns {
    /// Mark the Palette visible. Turns may now be started.
    pub fn set_visible(&self, visible: bool) {
        self.visible.store(visible, Ordering::Relaxed);
    }

    /// Kill every running Turn. Called from `window::hide` before EVENT_HIDE.
    pub fn cancel_all(&self) {
        let mut guard = self.running.lock().expect("turns mutex");
        for (_, child) in guard.drain() {
            let _ = child.lock().expect("child mutex").kill();
        }
    }

    /// Spawn a Turn and stream it. Returns immediately; events arrive later.
    ///
    /// Refused while the Palette is hidden: a Turn that starts after hide would
    /// have no surface to stream to and no way to be cancelled.
    pub fn start(
        self: &Arc<Self>,
        app: AppHandle,
        turn_id: u64,
        driver: Box<dyn AgentDriver>,
        req: TurnRequest,
    ) {
        if !self.visible.load(Ordering::Relaxed) {
            return;
        }
        let turns = self.clone();
        std::thread::spawn(move || {
            let outcome = turns.run(&app, turn_id, driver.as_ref(), &req);
            turns.running.lock().expect("turns mutex").remove(&turn_id);
            if let Err(message) = outcome {
                emit(&app, turn_id, TurnEvent::Failed { message });
            }
        });
    }

    /// Kill a running Turn. Silent when it has already finished.
    pub fn cancel(&self, turn_id: u64) {
        let child = self.running.lock().expect("turns mutex").remove(&turn_id);
        if let Some(child) = child {
            let _ = child.lock().expect("child mutex").kill();
        }
    }

    fn run(
        &self,
        app: &AppHandle,
        turn_id: u64,
        driver: &dyn AgentDriver,
        req: &TurnRequest,
    ) -> Result<(), String> {
        let exe = super::probe::resolve(driver.binary()).ok_or_else(|| {
            format!(
                "{} (`{}`) was not found on PATH.",
                driver.label(),
                driver.binary()
            )
        })?;

        let input = driver.turn_input(req);

        let mut command = super::probe::command(&exe);
        command
            .args(driver.turn_args(req))
            // Piped so we can write the prompt. Must be closed after writing or
            // Claude and opencode block waiting for EOF before they answer.
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if driver.cwd_is_process_cwd() {
            command.current_dir(&req.cwd);
        }

        let mut child = command
            .spawn()
            .map_err(|e| format!("Could not start {}: {e}", driver.label()))?;

        // Write the prompt and close the handle on its own thread.
        // Writing 26K chars while nobody reads stdout deadlocks as soon as
        // the child's output pipe fills. Same rule as the stderr drain.
        if let Some(mut stdin) = child.stdin.take() {
            let input_bytes = input.into_bytes();
            std::thread::spawn(move || {
                let _ = stdin.write_all(&input_bytes);
                // Handle is dropped here, signalling EOF to the child.
            });
        }

        let stdout = child.stdout.take().ok_or("No output from the Agent.")?;
        let stderr = child.stderr.take();

        // Drained on its own thread. Left unread it fills, and the child blocks
        // writing to it while we wait for stdout that will never come.
        let tail = Arc::new(Mutex::new(String::new()));
        if let Some(stderr) = stderr {
            let tail = tail.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    let mut tail = tail.lock().expect("stderr mutex");
                    tail.push_str(&line);
                    tail.push('\n');
                }
            });
        }

        let child = Arc::new(Mutex::new(child));

        // Re-check visibility after spawning: a hide that landed between
        // `start`'s gate and here must still kill this Turn.
        if !self.visible.load(Ordering::Relaxed) {
            let _ = child.lock().expect("child mutex").kill();
            return Ok(());
        }

        self.running
            .lock()
            .expect("turns mutex")
            .insert(turn_id, child.clone());

        // Re-check once more after registration: the window may have hidden
        // in the narrow gap between the visibility check and insertion.
        if !self.visible.load(Ordering::Relaxed) {
            let _ = child.lock().expect("child mutex").kill();
            self.running.lock().expect("turns mutex").remove(&turn_id);
            return Ok(());
        }

        let mut state = TurnState::default();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }
            if let Some(event) = driver.parse_line(&line, &mut state) {
                emit(app, turn_id, event);
            }
        }

        let status = loop {
            let waited = child.lock().expect("child mutex").try_wait();
            match waited {
                Ok(Some(status)) => break Some(status),
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(5)),
                Err(_) => break None,
            }
        };

        if status.map(|s| s.success()).unwrap_or(false) {
            emit(
                app,
                turn_id,
                TurnEvent::Done {
                    session: state.session.clone(),
                },
            );
            return Ok(());
        }

        let tail = tail.lock().expect("stderr mutex").trim().to_string();
        Err(if tail.is_empty() {
            format!("{} ended without answering.", driver.label())
        } else {
            tail
        })
    }
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

    /// Cancelling a Turn that never ran must be silent, not a panic — the Palette
    /// can dismiss between spawn and registration.
    #[test]
    fn v0_8_cancelling_an_unknown_turn_does_nothing() {
        let turns = Turns::default();
        turns.cancel(404);
    }

    /// `cancel_all` drains all running Turns without panicking.
    #[test]
    fn v0_11_1_cancel_all_on_empty_map_is_silent() {
        let turns = Turns::default();
        turns.cancel_all();
    }

    /// Turns are refused while the Palette is hidden. The gate defaults closed.
    #[test]
    fn v0_11_1_visibility_defaults_to_hidden() {
        let turns = Turns::default();
        assert!(!turns.visible.load(Ordering::Relaxed));
        turns.set_visible(true);
        assert!(turns.visible.load(Ordering::Relaxed));
        turns.set_visible(false);
        assert!(!turns.visible.load(Ordering::Relaxed));
    }
}
