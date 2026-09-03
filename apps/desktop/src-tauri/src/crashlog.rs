//! Crash logs, written to disk and **never sent** (ADR-0010).
//!
//! A panic in a release build is otherwise completely silent: the process has no
//! console, so the default hook writes to a stderr nobody is reading. What is
//! left is a launcher that stopped answering its hotkey with no evidence why.
//!
//! One file, appended to, capped. Not a logging framework: the only thing worth
//! keeping is the panic itself, and a file that grows without bound is its own
//! bug on a machine that runs this at login every day.

use std::io::Write;
use std::path::PathBuf;

/// Where crash logs live, under the data directory (ADR-0011).
pub fn dir() -> Option<PathBuf> {
    crate::identity::data_dir().map(|d| d.join("logs"))
}

fn file() -> Option<PathBuf> {
    dir().map(|d| d.join("panic.log"))
}

/// Past this, the file is truncated before the next append. Roughly a thousand
/// panics, which is far more than anyone needs and still a trivial file.
const MAX_BYTES: u64 = 256 * 1024;

/// Install the panic hook. Called once, as early as possible.
///
/// Chains the previous hook rather than replacing it, so debug builds keep their
/// console output and the backtrace behaviour is unchanged.
pub fn install() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        write(&format!("{info}"));
        previous(info);
    }));
}

/// Append one entry. Best effort: a failure here must never panic, or a panic in
/// the hook replaces the report with an abort.
fn write(entry: &str) {
    let Some(path) = file() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::remove_file(&path);
    }
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let _ = writeln!(f, "[{stamp}] {} {entry}", crate::identity::DISPLAY_NAME);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The folder is under the data directory, not beside the executable — the
    /// v1.0 install location is `C:\Program Files` and is not writable.
    #[test]
    fn v0_6_logs_live_under_the_data_directory() {
        let Some(d) = dir() else { return };
        assert!(d.ends_with(r"launcher\logs"), "got {}", d.display());
    }

    /// The cap is what stops a login-every-day process growing a file forever.
    ///
    /// Asserted through `write` rather than against the constant: a constant
    /// compared to itself passes by construction and can never disagree.
    #[test]
    fn v0_6_the_log_is_truncated_once_it_passes_the_cap() {
        let Some(path) = file() else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, vec![b'x'; (MAX_BYTES + 1) as usize]);

        write("a panic after the cap was passed");

        let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        assert!(len < MAX_BYTES, "the log grew past its cap: {len} bytes");
        let _ = std::fs::remove_file(&path);
    }
}
