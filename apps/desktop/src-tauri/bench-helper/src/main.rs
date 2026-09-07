//! The two things `bun run bench` needs from the OS, on either platform.
//!
//! Was three PowerShell scripts, which cannot run on macOS. Rust rather than
//! shell because `bench-input` feeds a 50 ms budget: driving it through
//! `osascript ... keystroke` puts an AppleScript round trip of tens of
//! milliseconds *inside* the measurement. Rust rather than Swift because Swift
//! would be a fourth language for one helper.
//!
//! Two subcommands match what `scripts/bench.ts` asked PowerShell for; `pids`
//! is new, and exists because a bench run against an already-running Takyon
//! measures nothing and blames the hotkey:
//!
//!   takyon-bench input --key <AltSpace|CtrlAltF9|Escape|LetterC>
//!   takyon-bench mem   --pid <pid>
//!   takyon-bench pids  --name <executable stem>

mod input;
mod mem;

use std::process::ExitCode;

/// The chords the harness sends. Named for the Windows spelling on both
/// platforms: macOS Option is Windows Alt, and both are the same physical key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    AltSpace,
    CtrlAltF9,
    Escape,
    LetterC,
}

impl Key {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "AltSpace" => Some(Self::AltSpace),
            "CtrlAltF9" => Some(Self::CtrlAltF9),
            "Escape" => Some(Self::Escape),
            "LetterC" => Some(Self::LetterC),
            _ => None,
        }
    }
}

const USAGE: &str = "\
usage:
  takyon-bench input --key <AltSpace|CtrlAltF9|Escape|LetterC>
  takyon-bench mem   --pid <pid>
  takyon-bench pids  --name <executable stem>
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(out) => {
            if !out.is_empty() {
                println!("{out}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("takyon-bench: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<String, String> {
    let (cmd, rest) = args.split_first().ok_or_else(|| USAGE.to_string())?;
    match cmd.as_str() {
        "input" => {
            let name = flag(rest, "--key").ok_or_else(|| format!("input needs --key\n{USAGE}"))?;
            let key = Key::parse(name).ok_or_else(|| format!("unknown key {name:?}\n{USAGE}"))?;
            input::send(key)?;
            Ok(String::new())
        }
        "mem" => {
            let raw = flag(rest, "--pid").ok_or_else(|| format!("mem needs --pid\n{USAGE}"))?;
            let pid: u32 = raw.parse().map_err(|_| format!("--pid {raw:?} is not a number"))?;
            Ok(mem::tree(pid)?.to_string())
        }
        "pids" => {
            let name = flag(rest, "--name").ok_or_else(|| format!("pids needs --name\n{USAGE}"))?;
            Ok(mem::pids(name)?.to_string())
        }
        other => Err(format!("unknown subcommand {other:?}\n{USAGE}")),
    }
}

/// Value following `name`, or `None`.
fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let i = args.iter().position(|a| a == name)?;
    args.get(i + 1).map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_the_harness_sends_parses() {
        for name in ["AltSpace", "CtrlAltF9", "Escape", "LetterC"] {
            assert!(Key::parse(name).is_some(), "{name} stopped parsing");
        }
        assert_eq!(Key::parse("Alt+Space"), None);
    }

    #[test]
    fn flag_reads_the_following_value() {
        let args: Vec<String> = ["--key", "Escape"].iter().map(|s| s.to_string()).collect();
        assert_eq!(flag(&args, "--key"), Some("Escape"));
        assert_eq!(flag(&args, "--pid"), None);
    }

    #[test]
    fn a_trailing_flag_with_no_value_is_not_a_panic() {
        let args = vec!["--key".to_string()];
        assert_eq!(flag(&args, "--key"), None);
    }

    #[test]
    fn unknown_subcommand_is_an_error_not_a_panic() {
        assert!(run(&["wat".to_string()]).is_err());
        assert!(run(&[]).is_err());
    }
}
