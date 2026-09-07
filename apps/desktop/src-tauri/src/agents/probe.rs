//! Finding an Agent's binary, and running it once with a deadline.
//!
//! Every driver spawns through here. Two reasons that is a rule rather than a
//! convenience: `CREATE_NO_WINDOW` must be on every spawn or a launcher that
//! promises tens of milliseconds blinks a console on each probe, and a probe with
//! no timeout is a hang wearing a status card.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// No console window for any child. `CREATE_NO_WINDOW` from `processthreadsapi`.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Long enough for a cold Node start on a busy machine, short enough that a wedged
/// CLI still yields a card. Nothing on the keystroke path waits on this.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// One finished run.
#[derive(Debug, Clone)]
pub struct Output {
    /// `None` when the child was killed at the deadline.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }

    /// First non-empty line of stderr then stdout. Codex answers on stderr,
    /// which is why this looks there first.
    pub fn first_line(&self) -> Option<&str> {
        self.stderr
            .lines()
            .chain(self.stdout.lines())
            .map(str::trim)
            .find(|line| !line.is_empty())
    }
}

/// Executable extensions to try, most likely first.
///
/// `.cmd` is not an afterthought: an `npm i -g` install is a `.cmd` shim on
/// Windows, so half the installs in the wild have no `.exe` at all.
#[cfg(windows)]
const EXTS: [&str; 4] = ["exe", "cmd", "bat", "ps1"];
#[cfg(not(windows))]
const EXTS: [&str; 1] = [""];

/// Locate `binary` on `PATH`, then in the places these CLIs actually install to.
///
/// The second half matters because `PATH` in a GUI process is the `PATH` that
/// existed at login: a `bun add -g` afterwards is invisible until a re-probe.
pub fn resolve(binary: &str) -> Option<PathBuf> {
    let from_path = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default();
    for dir in from_path.into_iter().chain(extra_dirs()) {
        if let Some(hit) = in_dir(&dir, binary) {
            return Some(hit);
        }
    }
    None
}

/// Where `claude`, `codex` and `opencode` land when their installers run.
#[cfg(windows)]
fn extra_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from);
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    [
        home.as_ref().map(|h| h.join(".local").join("bin")),
        home.as_ref().map(|h| h.join(".bun").join("bin")),
        home.as_ref().map(|h| h.join(".cargo").join("bin")),
        appdata.map(|a| a.join("npm")),
        local.map(|l| l.join("pnpm")),
    ]
    .into_iter()
    .flatten()
    .collect()
}

#[cfg(not(windows))]
fn extra_dirs() -> Vec<PathBuf> {
    Vec::new()
}

fn in_dir(dir: &Path, binary: &str) -> Option<PathBuf> {
    for ext in EXTS {
        let candidate = if ext.is_empty() {
            dir.join(binary)
        } else {
            dir.join(format!("{binary}.{ext}"))
        };
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Run `exe` once and collect both streams, killing it at `timeout`.
///
/// Both pipes are drained on their own threads. Reading them in sequence
/// deadlocks the moment a child fills the pipe we are not reading.
pub fn run(exe: &Path, args: &[&str], timeout: Duration) -> std::io::Result<Output> {
    let mut child = command(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let (tx, rx) = mpsc::channel::<(bool, String)>();
    for (is_out, pipe) in [
        (true, child.stdout.take().map(PipeSource::Out)),
        (false, child.stderr.take().map(PipeSource::Err)),
    ] {
        let Some(pipe) = pipe else { continue };
        let tx = tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send((is_out, pipe.read_to_string()));
        });
    }
    drop(tx);

    let deadline = Instant::now() + timeout;
    let code = loop {
        match child.try_wait()? {
            Some(status) => break status.code(),
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    };

    let (mut stdout, mut stderr) = (String::new(), String::new());
    while let Ok((is_out, text)) = rx.recv() {
        if is_out {
            stdout = text;
        } else {
            stderr = text;
        }
    }
    Ok(Output {
        code,
        stdout,
        stderr,
    })
}

/// A `Command` with the window suppressed. The only place a child is built.
pub fn command(exe: &Path) -> Command {
    // Only the Windows arm below mutates it.
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut cmd = Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Erases which pipe is being drained so both reader threads share one body.
enum PipeSource {
    Out(std::process::ChildStdout),
    Err(std::process::ChildStderr),
}

impl PipeSource {
    fn read_to_string(self) -> String {
        let mut buf = Vec::new();
        let read = match self {
            PipeSource::Out(mut pipe) => pipe.read_to_end(&mut buf),
            PipeSource::Err(mut pipe) => pipe.read_to_end(&mut buf),
        };
        if read.is_err() {
            return String::new();
        }
        String::from_utf8_lossy(&buf).into_owned()
    }
}

/// Parse a version from `--version` output: the first `1.2.3`-shaped token.
///
/// Shapes differ — `2.1.261 (Claude Code)`, a bare `1.18.27` — so the number is
/// found rather than the line being trusted.
pub fn version_from(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut start = None;
    for (i, byte) in bytes.iter().enumerate() {
        let is_token = byte.is_ascii_digit() || *byte == b'.';
        match (is_token, start) {
            (true, None) => start = Some(i),
            (false, Some(from)) => {
                if let Some(found) = semver_like(&text[from..i]) {
                    return Some(found);
                }
                start = None;
            }
            _ => {}
        }
    }
    start.and_then(|from| semver_like(&text[from..]))
}

fn semver_like(token: &str) -> Option<String> {
    let parts: Vec<&str> = token.split('.').collect();
    let numeric = parts.len() >= 2 && parts.iter().all(|p| !p.is_empty() && p.parse::<u32>().is_ok());
    numeric.then(|| token.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_8_a_version_is_found_inside_whatever_the_cli_printed() {
        assert_eq!(version_from("2.1.261 (Claude Code)").as_deref(), Some("2.1.261"));
        assert_eq!(version_from("1.18.27").as_deref(), Some("1.18.27"));
        assert_eq!(version_from("codex-cli 0.52.0\n").as_deref(), Some("0.52.0"));
        assert_eq!(version_from("no numbers here"), None);
        // A lone integer is a count, not a version.
        assert_eq!(version_from("7 models"), None);
    }

    /// Codex writes its Sign-in answer to stderr, so stderr is read first.
    #[test]
    fn v0_8_the_first_line_prefers_stderr_because_codex_answers_there() {
        let out = Output {
            code: Some(1),
            stdout: "  \nsomething".into(),
            stderr: "\n  Not logged in\n".into(),
        };
        assert_eq!(out.first_line(), Some("Not logged in"));
        assert!(!out.ok());

        let quiet = Output {
            code: Some(0),
            stdout: "ready".into(),
            stderr: String::new(),
        };
        assert_eq!(quiet.first_line(), Some("ready"));
    }

    /// Nothing is resolvable under a name no installer uses. Guards the negative
    /// path the "not found" card depends on.
    #[test]
    fn v0_8_an_unknown_binary_resolves_to_nothing() {
        assert!(resolve("takyon-agent-that-does-not-exist").is_none());
    }
}
