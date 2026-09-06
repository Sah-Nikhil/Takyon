//! TEMP diagnostic: replicate turn.rs's spawn + read loop against real claude.
use std::io::{BufRead, BufReader};
use std::process::Stdio;
use std::time::Instant;

use takyon_lib::agents::{self, AgentKind, TurnRequest, TurnState};

#[test]
fn live_turn_prints_events() {
    let driver = agents::driver_for(AgentKind::Claude).expect("claude driver");
    let req = TurnRequest {
        prompt: "who directed fast five".into(),
        cwd: agents::scratch::resolve(None),
        session: None,
        model: Some("sonnet".into()),
        effort: Some("medium".into()),
        tools: false,
    };
    let exe = agents::probe::resolve(driver.binary()).expect("claude on PATH");
    eprintln!("exe: {}", exe.display());
    eprintln!("cwd: {}", req.cwd.display());
    eprintln!("args: {:?}", driver.turn_args(&req));

    let t0 = Instant::now();
    let mut command = agents::probe::command(&exe);
    command
        .args(driver.turn_args(&req))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if driver.cwd_is_process_cwd() {
        command.current_dir(&req.cwd);
    }
    let mut child = command.spawn().expect("spawn");
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            eprintln!("STDERR: {line}");
        }
    });

    let mut state = TurnState::default();
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        let head: String = line.chars().take(90).collect();
        eprintln!("[{:>6.2}s] raw: {head}", t0.elapsed().as_secs_f32());
        if let Some(event) = driver.parse_line(&line, &mut state) {
            eprintln!("[{:>6.2}s] EVENT {event:?}", t0.elapsed().as_secs_f32());
        }
    }
    eprintln!("[{:>6.2}s] stdout EOF", t0.elapsed().as_secs_f32());
    let status = child.wait().expect("wait");
    eprintln!("[{:>6.2}s] exit {status:?}", t0.elapsed().as_secs_f32());
}
