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

use std::time::{Duration, Instant};

use takyon_lib::agents::{self, AgentKind, Health, SignInStatus};

/// Every Agent answers, installed or not, and the two never disagree.
#[test]
fn v0_9_every_agent_produces_a_coherent_snapshot() {
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
fn v0_9_anything_that_is_not_ready_says_why() {
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
/// Skipped rather than failed where nothing is installed: CI machines and fresh
/// checkouts have no Agents, and a test that only passes on this laptop is not a
/// test.
#[test]
fn v0_9_an_installed_agent_reports_its_version() {
    let installed: Vec<_> = agents::snapshots().into_iter().filter(|s| s.installed).collect();
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
/// Not a Palette budget — nothing here is on the keystroke path (v0.9 Traps) —
/// but a page that takes half a minute to fill in reads as broken.
#[test]
fn v0_9_probing_every_agent_is_bounded() {
    let started = Instant::now();
    let _ = agents::snapshots();
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(70),
        "probing every Agent took {elapsed:?}"
    );
    eprintln!("[takyon] probed {} Agents in {elapsed:?}", AgentKind::ALL.len());
}

/// A signed-in Agent lists models, because Settings will not let you pick one
/// otherwise and the model is locked down (v0.9 task 10).
///
/// Shape only: which models exist is the Agent's business and changes weekly.
#[test]
fn v0_9_a_signed_in_agent_lists_models() {
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

/// One real Turn, end to end, against every signed-in Agent on this machine.
///
/// `#[ignore]` because it costs tokens and needs a network: run it by hand with
/// `cargo test --test agents_cli -- --ignored --nocapture`. It is the only thing
/// that proves the flags in each driver still spell what they meant to.
#[test]
#[ignore]
fn v0_9_a_real_turn_answers() {
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
        let args = driver.turn_args(&agents::TurnRequest {
            prompt: "Reply with exactly one word: ok".into(),
            cwd: agents::scratch::dir(),
            session: None,
            model,
            effort,
            tools: false,
        });
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = agents::probe::run(&exe, &borrowed, Duration::from_secs(180))
            .expect("the Agent spawned");

        let mut state = agents::TurnState::default();
        let mut answer = String::new();
        for line in out.stdout.lines() {
            if let Some(agents::turn::TurnEvent::Text { delta }) =
                driver.parse_line(line, &mut state)
            {
                answer.push_str(&delta);
            }
        }
        eprintln!(
            "[takyon] {} answered {answer:?} (session {:?})",
            snapshot.label, state.session
        );
        assert!(!answer.trim().is_empty(), "{} said nothing", snapshot.label);
        assert!(state.session.is_some(), "{} reported no session", snapshot.label);
    }
}

/// A tools-off Turn writes nothing, asked directly to write something.
///
/// `docs/verify/v0.9.md` §5 as a test rather than a person, and the claim
/// ADR-0017 rests on. Its own empty directory, not the real Scratch, so a
/// failure is visible rather than mixed in. `#[ignore]`: real tokens.
#[test]
#[ignore]
fn v0_9_a_tools_off_turn_writes_nothing() {
    let sandbox = std::env::temp_dir().join(format!("takyon-toolsoff-{}", std::process::id()));
    std::fs::create_dir_all(&sandbox).expect("a directory to watch");

    let mut checked = 0;
    for snapshot in agents::snapshots() {
        if snapshot.sign_in.status != SignInStatus::In {
            continue;
        }
        let driver = agents::driver_for(snapshot.kind).expect("a driver for every kind");
        let exe = agents::probe::resolve(driver.binary()).expect("installed");
        let args = driver.turn_args(&agents::TurnRequest {
            prompt: "Create a file called proof.txt in the current directory, containing the word proof.".into(),
            cwd: sandbox.clone(),
            session: None,
            model: None,
            effort: None,
            tools: false,
        });
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let _ = agents::probe::run(&exe, &borrowed, Duration::from_secs(180));

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
fn v0_9_the_scratch_directory_is_under_our_data_directory() {
    let scratch = agents::scratch::dir();
    assert!(scratch.is_dir());
    assert!(scratch.ends_with("scratch"));
    if let Some(data) = takyon_lib::identity::data_dir() {
        assert!(scratch.starts_with(data));
    }
}
