//! The Agent probes against whatever is actually installed on this machine.
//!
//! The unit tests parse captured payloads; this runs the real binaries. It is
//! the only layer that catches a CLI changing its flags or its output shape
//! under us, which is the failure mode ADR-0017's whole design leans on not
//! happening silently.
//!
//! **Machine-dependent by construction.** Nothing here asserts which Agents are
//! installed or whether anyone is signed in — only that the probe answers, and
//! that its answer is internally consistent.
//! Plus fake `.cmd` Agents in a `TempDir` (v0.11.1): npm's shim shape on any
//! Windows machine, so the spawn path is tested rather than assumed.

mod common;

use std::time::{Duration, Instant};

use takyon_lib::agents::{self, AgentKind, Health, SignInStatus};

/// One Turn's request with `prompt`, run in `cwd`. First Turn, tools off.
fn request(prompt: &str, cwd: &std::path::Path) -> agents::TurnRequest {
    agents::TurnRequest {
        prompt: prompt.into(),
        cwd: cwd.to_path_buf(),
        session: None,
        model: None,
        effort: None,
        tools: false,
    }
}

/// Write a fake Agent: a batch file named `name` holding `body`.
#[cfg(windows)]
fn fake_agent(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let exe = dir.join(name);
    std::fs::write(&exe, format!("{body}\r\n")).expect("fake agent");
    exe
}

/// Newlines, `%PATH%`, quotes, `&|^` and 30,000 characters reach a `.cmd` Agent
/// intact on stdin, through every driver's real spawn. Before v0.11.1 this was
/// `batch file arguments are invalid`.
#[cfg(windows)]
#[test]
fn v0_11_1_a_cmd_agent_reads_a_multi_line_prompt_on_stdin() {
    use std::io::Read;

    let temp = common::TempDir::new("agent-echo");
    let cwd = temp.path().join("scratch dir");
    std::fs::create_dir_all(&cwd).unwrap();
    let exe = fake_agent(temp.path(), "echo.cmd", "@findstr \"^\"");

    let mut prompt = String::from("who directed titanic\n\n%PATH% \"quoted\" &|^ <>\n");
    while prompt.len() < 30_000 {
        prompt.push_str(&"0123456789".repeat(12));
        prompt.push('\n');
    }

    let normalise = |text: &str| text.replace("\r\n", "\n").trim_end().to_string();
    for driver in agents::drivers() {
        let mut req = request(&prompt, &cwd);
        for session in [None, Some("s-1".to_string())] {
            req.session = session;
            let mut spawned =
                agents::turn::spawn(driver.as_ref(), &exe, &req).expect("a .cmd Agent spawns");
            let mut out = String::new();
            spawned
                .child
                .stdout
                .take()
                .unwrap()
                .read_to_string(&mut out)
                .unwrap();
            assert!(spawned.child.wait().unwrap().success());
            assert_eq!(
                normalise(&out),
                normalise(&driver.turn_input(&req)),
                "{} lost its input",
                driver.label()
            );
        }
    }
}

/// Poll `job` until no process is left in it, or two seconds pass.
#[cfg(windows)]
fn drains(job: &agents::job::Job) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if job.active_processes().unwrap() == 0 {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

/// Start a hanging `.cmd` Turn and return its job once `cmd.exe` and `ping` run.
#[cfg(windows)]
fn hanging_turn(
    turns: &std::sync::Arc<agents::turn::Turns>,
    turn_id: u64,
    exe: &std::path::Path,
    cwd: &std::path::Path,
    events: std::sync::mpsc::Sender<agents::turn::TurnEvent>,
) -> std::sync::Arc<agents::job::Job> {
    let driver = agents::driver_for(AgentKind::OpenCode).unwrap();
    turns
        .start_with(
            turn_id,
            driver,
            Some(exe.into()),
            request("hi", cwd),
            move |_, e| {
                let _ = events.send(e);
            },
        )
        .expect("the gate is open");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(job) = turns.job(turn_id) {
            if job.active_processes().unwrap() >= 2 {
                return job;
            }
        }
        assert!(Instant::now() < deadline, "the hanging Turn never started");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Cancel, `cancel_all` and closing the gate each kill the whole tree —
/// `cmd.exe` and the `ping` it started — and none of them emits an event.
#[cfg(windows)]
#[test]
fn v0_11_1_stopping_a_turn_kills_its_whole_tree() {
    let temp = common::TempDir::new("agent-hang");
    let exe = fake_agent(temp.path(), "hang.cmd", "@ping -n 60 127.0.0.1 >nul");
    let turns = std::sync::Arc::new(agents::turn::Turns::default());

    type Stop = fn(&agents::turn::Turns, u64);
    let stops: [(&str, Stop); 3] = [
        ("cancel", |turns, id| turns.cancel(id)),
        ("cancel_all", |turns, _| turns.cancel_all()),
        ("set_visible(false)", |turns, _| turns.set_visible(false)),
    ];
    for (i, (how, stop)) in stops.into_iter().enumerate() {
        turns.set_visible(true);
        let (tx, rx) = std::sync::mpsc::channel();
        let id = 1_000 + i as u64;
        let job = hanging_turn(&turns, id, &exe, temp.path(), tx);
        stop(&turns, id);
        assert!(drains(&job), "{how} left processes running");
        // The Turn's thread ends and says nothing: nobody is reading.
        std::thread::sleep(Duration::from_millis(200));
        assert!(rx.try_recv().is_err(), "{how} emitted an event");
    }
}

/// A shim whose runtime is missing fails as `launcherBroken`, not as noise.
#[cfg(windows)]
#[test]
fn v0_11_1_a_shim_with_no_runtime_is_a_broken_launcher() {
    use agents::turn::{TurnEvent, TurnFailure};

    let temp = common::TempDir::new("agent-nonode");
    let exe = fake_agent(temp.path(), "nonode.cmd", "@\"definitely-not-node\" %*");
    let turns = std::sync::Arc::new(agents::turn::Turns::default());
    turns.set_visible(true);
    let (tx, rx) = std::sync::mpsc::channel();
    let driver = agents::driver_for(AgentKind::OpenCode).unwrap();
    turns
        .start_with(
            7,
            driver,
            Some(exe.clone()),
            request("hi\nthere", temp.path()),
            move |_, e| {
                let _ = tx.send(e);
            },
        )
        .expect("the gate is open")
        .join()
        .unwrap();
    let events: Vec<TurnEvent> = rx.try_iter().collect();
    match events.last() {
        Some(TurnEvent::Failed {
            failure: TurnFailure::LauncherBroken { binary, detail, .. },
        }) => {
            assert_eq!(std::path::Path::new(binary), exe);
            assert!(detail.contains("definitely-not-node"), "{detail}");
        }
        other => panic!("expected launcherBroken, got {other:?}"),
    }
}

/// A missing binary is `notFound`, naming the command to install.
#[test]
fn v0_11_1_a_missing_agent_fails_as_not_found() {
    use agents::turn::{TurnEvent, TurnFailure};

    struct Ghost;
    impl agents::AgentDriver for Ghost {
        fn kind(&self) -> AgentKind {
            AgentKind::Claude
        }
        fn label(&self) -> &'static str {
            "Ghost"
        }
        fn binary(&self) -> &'static str {
            "takyon-agent-that-does-not-exist"
        }
        fn probe(&self, _: &std::path::Path) -> agents::Snapshot {
            unreachable!()
        }
        fn efforts(&self) -> &'static [&'static str] {
            &[]
        }
        fn models(&self, _: &std::path::Path) -> Vec<String> {
            Vec::new()
        }
        fn turn_args(&self, _: &agents::TurnRequest) -> Vec<String> {
            Vec::new()
        }
        fn parse_line(&self, _: &str, _: &mut agents::TurnState) -> Option<TurnEvent> {
            None
        }
    }

    let turns = std::sync::Arc::new(agents::turn::Turns::default());
    turns.set_visible(true);
    let (tx, rx) = std::sync::mpsc::channel();
    let temp = common::TempDir::new("agent-ghost");
    turns
        .start_with(
            8,
            Box::new(Ghost),
            None,
            request("hi", temp.path()),
            move |_, e| {
                let _ = tx.send(e);
            },
        )
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(
        rx.try_iter().last(),
        Some(TurnEvent::Failed {
            failure: TurnFailure::NotFound {
                agent: "Ghost".into(),
                binary: "takyon-agent-that-does-not-exist".into(),
            }
        })
    );
}

/// Every Agent answers, installed or not, and the two never disagree.
#[test]
fn v0_8_every_agent_produces_a_coherent_snapshot() {
    let snapshots = agents::snapshots();
    assert_eq!(snapshots.len(), AgentKind::ALL.len());

    for snapshot in &snapshots {
        assert!(!snapshot.binary.is_empty());
        assert!(!snapshot.label.is_empty());

        if snapshot.installed {
            // An installed Agent that would not answer is `Unknown`, never `In`:
            // claiming a sign-in nobody verified is the one wrong answer here.
            if snapshot.sign_in.status == SignInStatus::In {
                assert_eq!(snapshot.health, Health::Ready, "{}", snapshot.label);
            }
        } else {
            assert_eq!(snapshot.health, Health::Error);
            assert_eq!(snapshot.sign_in.status, SignInStatus::Unknown);
            let message = snapshot.message.as_deref().unwrap_or_default();
            assert!(message.contains(snapshot.binary), "{message}");
            assert!(snapshot.version.is_none());
        }
    }
}

/// A signed-out or missing Agent still gets a sentence. A card with a red dot
/// and no words is the shape ADR-0017 exists to prevent.
#[test]
fn v0_8_anything_that_is_not_ready_says_why() {
    for snapshot in agents::snapshots() {
        if snapshot.health == Health::Ready {
            continue;
        }
        assert!(
            snapshot.message.is_some(),
            "{} is {:?} with nothing to say",
            snapshot.label,
            snapshot.health
        );
    }
}

/// An installed Agent reports a version, because the card shows one.
///
/// Skipped rather than failed where nothing is installed. CI filters it out by
/// name instead (ci.yml), so a runner with no Agents reports it as not run.
#[test]
fn v0_8_an_installed_agent_reports_its_version() {
    let installed: Vec<_> = agents::snapshots()
        .into_iter()
        .filter(|s| s.installed)
        .collect();
    if installed.is_empty() {
        eprintln!("[takyon] no Agent CLI installed; version assertion skipped");
        return;
    }
    for snapshot in installed {
        assert!(
            snapshot.version.is_some(),
            "{} is installed but reported no version",
            snapshot.label
        );
    }
}

/// Probing three Agents stays inside the time a Settings mount can absorb.
///
/// Not a Palette budget — nothing here is on the keystroke path (v0.8 Traps) —
/// but a page that takes half a minute to fill in reads as broken.
#[test]
fn v0_8_probing_every_agent_is_bounded() {
    let started = Instant::now();
    let _ = agents::snapshots();
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(70),
        "probing every Agent took {elapsed:?}"
    );
    eprintln!(
        "[takyon] probed {} Agents in {elapsed:?}",
        AgentKind::ALL.len()
    );
}

/// A signed-in Agent lists models, because Settings will not let you pick one
/// otherwise and the model is locked down (v0.8 task 10).
///
/// Shape only: which models exist is the Agent's business and changes weekly.
/// Filtered out on CI by name (ci.yml): no runner is signed in to an Agent.
#[test]
fn v0_8_a_signed_in_agent_lists_models() {
    let mut checked = 0;
    for snapshot in agents::snapshots() {
        if snapshot.sign_in.status != SignInStatus::In {
            continue;
        }
        let models = agents::models_for(snapshot.kind);
        assert!(
            !models.is_empty(),
            "{} is signed in but offers no models to lock to",
            snapshot.label
        );
        assert!(models.iter().all(|m| !m.trim().is_empty()));
        eprintln!("[takyon] {} offers {} models", snapshot.label, models.len());
        checked += 1;
    }
    if checked == 0 {
        eprintln!("[takyon] no Agent signed in; model listing skipped");
    }
}

/// Run one Turn whole through `turn::spawn`, the path `Turns` takes, and return
/// stdout. Killed at three minutes. Not `run_with_input`: that has no cwd, and
/// Claude's cwd is its process's.
fn run_turn(
    driver: &dyn agents::AgentDriver,
    exe: &std::path::Path,
    req: &agents::TurnRequest,
) -> String {
    use std::io::Read;

    let mut spawned = agents::turn::spawn(driver, exe, req).expect("the Agent spawned");
    let mut stdout = spawned.child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        let _ = stdout.read_to_string(&mut out);
        out
    });
    if let Some(mut stderr) = spawned.child.stderr.take() {
        std::thread::spawn(move || std::io::copy(&mut stderr, &mut std::io::stderr()));
    }
    let deadline = Instant::now() + Duration::from_secs(180);
    while spawned.child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            eprintln!("[takyon] {} timed out", driver.label());
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    spawned.job.terminate();
    let _ = spawned.child.wait();
    reader.join().unwrap()
}

/// Every text delta a driver parses out of a whole run's stdout, and the state.
fn answer_of(driver: &dyn agents::AgentDriver, stdout: &str) -> (String, agents::TurnState) {
    let mut state = agents::TurnState::default();
    let mut answer = String::new();
    for line in stdout.lines() {
        if let Some(agents::turn::TurnEvent::Text { delta }) = driver.parse_line(line, &mut state) {
            answer.push_str(&delta);
        }
    }
    (answer, state)
}

/// A real Turn through a `.cmd` shim, multi-line prompt: the laptop's npm
/// install reproduced without touching `PATH`. `#[ignore]`: real tokens.
#[cfg(windows)]
#[test]
#[ignore]
fn v0_11_1_a_real_turn_answers_through_a_cmd_shim() {
    let temp = common::TempDir::new("agent-shim-live");
    for snapshot in agents::snapshots() {
        if snapshot.sign_in.status != SignInStatus::In {
            eprintln!("[takyon] {} is not signed in; skipped", snapshot.label);
            continue;
        }
        let driver = agents::driver_for(snapshot.kind).expect("a driver for every kind");
        let real = agents::probe::resolve(driver.binary()).expect("installed");
        let shim = fake_agent(
            temp.path(),
            &format!("{}.cmd", driver.binary()),
            &format!("@\"{}\" %*", real.display()),
        );
        let req = request(
            "Answer the second line only.\n\nReply with exactly one word: ok",
            &agents::scratch::dir(),
        );
        let (answer, state) = answer_of(driver.as_ref(), &run_turn(driver.as_ref(), &shim, &req));
        eprintln!(
            "[takyon] {} via {} answered {answer:?} (session {:?})",
            snapshot.label,
            shim.display(),
            state.session
        );
        assert!(!answer.trim().is_empty(), "{} said nothing", snapshot.label);
    }
}

/// One real Turn, end to end, against every signed-in Agent on this machine.
///
/// `#[ignore]` because it costs tokens and needs a network: run it by hand with
/// `cargo test --test agents_cli -- --ignored --nocapture`. It is the only thing
/// that proves the flags in each driver still spell what they meant to.
#[test]
#[ignore]
fn v0_8_a_real_turn_answers() {
    for snapshot in agents::snapshots() {
        if snapshot.sign_in.status != SignInStatus::In {
            eprintln!("[takyon] {} is not signed in; skipped", snapshot.label);
            continue;
        }
        let driver = agents::driver_for(snapshot.kind).expect("a driver for every kind");
        let exe = agents::probe::resolve(driver.binary()).expect("installed");
        // The locked pair is sent, not defaulted. Each Agent spells effort its
        // own way — `--effort`, a `-c` config override, `--variant` — and a
        // wrong spelling fails the Turn at runtime with nothing else to catch it.
        let model = agents::models_for(snapshot.kind).into_iter().next();
        let effort = driver.efforts().first().map(|e| e.to_string());
        eprintln!(
            "[takyon] {} asking with model {model:?} effort {effort:?}",
            snapshot.label
        );
        let req = agents::TurnRequest {
            prompt: "Reply with exactly one word: ok".into(),
            cwd: agents::scratch::dir(),
            session: None,
            model,
            effort,
            tools: false,
        };
        let (mut answer, mut state) =
            answer_of(driver.as_ref(), &run_turn(driver.as_ref(), &exe, &req));
        // The first model a catalogue lists is not always one this account may
        // use — Codex bundles `gpt-6-astra`, which a ChatGPT plan refuses. The
        // flags were still exercised; the Agent's own default answers the rest.
        if answer.trim().is_empty() && req.model.is_some() {
            eprintln!(
                "[takyon] {} refused model {:?}; retrying on its own default",
                snapshot.label, req.model
            );
            let fallback = agents::TurnRequest {
                model: None,
                ..req.clone()
            };
            (answer, state) =
                answer_of(driver.as_ref(), &run_turn(driver.as_ref(), &exe, &fallback));
        }
        eprintln!(
            "[takyon] {} answered {answer:?} (session {:?})",
            snapshot.label, state.session
        );
        assert!(!answer.trim().is_empty(), "{} said nothing", snapshot.label);
        assert!(
            state.session.is_some(),
            "{} reported no session",
            snapshot.label
        );
    }
}

/// A tools-off Turn writes nothing, asked directly to write something.
///
/// `docs/verify/v0.8.md` §5 as a test rather than a person, and the claim
/// ADR-0017 rests on. Its own empty directory, not the real Scratch, so a
/// failure is visible rather than mixed in. `#[ignore]`: real tokens.
#[test]
#[ignore]
fn v0_8_a_tools_off_turn_writes_nothing() {
    let sandbox = std::env::temp_dir().join(format!("takyon-toolsoff-{}", std::process::id()));
    std::fs::create_dir_all(&sandbox).expect("a directory to watch");

    let mut checked = 0;
    for snapshot in agents::snapshots() {
        if snapshot.sign_in.status != SignInStatus::In {
            continue;
        }
        let driver = agents::driver_for(snapshot.kind).expect("a driver for every kind");
        let exe = agents::probe::resolve(driver.binary()).expect("installed");
        let _ = run_turn(
            driver.as_ref(),
            &exe,
            &request(
                "Create a file called proof.txt in the current directory, containing the word proof.",
                &sandbox,
            ),
        );

        let left: Vec<_> = std::fs::read_dir(&sandbox)
            .expect("the sandbox still exists")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert!(
            left.is_empty(),
            "{} wrote {left:?} with tools off",
            snapshot.label
        );
        eprintln!("[takyon] {} wrote nothing with tools off", snapshot.label);
        checked += 1;
    }

    let _ = std::fs::remove_dir_all(&sandbox);
    if checked == 0 {
        eprintln!("[takyon] no Agent signed in; tools-off assertion skipped");
    }
}

/// The Scratch directory exists and is ours, so a Turn has somewhere to run.
#[test]
fn v0_8_the_scratch_directory_is_under_our_data_directory() {
    let scratch = agents::scratch::dir();
    assert!(scratch.is_dir());
    assert!(scratch.ends_with("scratch"));
    if let Some(data) = takyon_lib::identity::data_dir() {
        assert!(scratch.starts_with(data));
    }
}
