//! The measurement side of `bun run bench`.
//!
//! v0.1 exists to produce four numbers, and how they are produced decides whether
//! they are evidence or theatre. Two rules follow from that:
//!
//! **Rust owns both ends of every span.** The hotkey timestamp and the
//! first-pixel timestamp are both taken here, on one clock. The frontend's only
//! job is to echo an id back once it has painted. Subtracting a
//! `performance.now()` from an `Instant` would produce a plausible number with no
//! defined meaning, and that is the usual way a latency claim becomes fiction.
//!
//! **The span's edges are stated, not hidden.** What this measures is
//! *hotkey handler entry -> the IPC call that follows the frame the renderer
//! committed*. It therefore **includes** one IPC round trip (sub-millisecond) and
//! **excludes** DWM's final composition and present. The manual high-FPS capture
//! recorded in `docs/tbc/0002` is what calibrates the gap; this number alone is a
//! regression gate, not a claim about what the user's eye sees.

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// Set this to a file path to turn measurement on. Absent in a normal run, so the
/// hot path costs one atomic increment and one `Option` check.
pub const LOG_ENV: &str = "TAKYON_BENCH_LOG";

pub struct Bench {
    log: Option<Mutex<std::fs::File>>,
    /// Only the newest show is tracked. A map would grow without bound whenever a
    /// show is never acknowledged, which happens on every dismissal faster than a
    /// frame, i.e. routinely.
    open_show: Mutex<Option<(u64, Instant)>>,
    next_id: AtomicU64,
    /// Process start, for the login-to-responsive budget.
    started: Instant,
}

impl Bench {
    pub fn from_env(started: Instant) -> Self {
        let log = std::env::var_os(LOG_ENV).and_then(|path| {
            match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                Ok(f) => Some(Mutex::new(f)),
                Err(e) => {
                    // Loud, because a silent failure here means a bench run that
                    // produces no output and no reason why.
                    eprintln!("[takyon] cannot open {LOG_ENV}={path:?}: {e}");
                    None
                }
            }
        });
        Self {
            log,
            open_show: Mutex::new(None),
            next_id: AtomicU64::new(1),
            started,
        }
    }

    pub fn enabled(&self) -> bool {
        self.log.is_some()
    }

    /// Called from the hotkey handler, before anything else happens. Returns the id
    /// the frontend will echo back.
    pub fn mark_show(&self) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        if self.enabled() {
            *self.open_show.lock().unwrap_or_else(|e| e.into_inner()) = Some((id, Instant::now()));
        }
        id
    }

    /// Called when the frontend reports that the frame for `id` has been painted.
    ///
    /// A mismatched id is dropped rather than measured: it means a newer show
    /// superseded this one, and attributing the newer show's paint to the older
    /// show's keypress would produce a number that is too *good*, which is the
    /// dangerous direction to be wrong in.
    pub fn first_pixel(&self, id: u64) {
        let taken = {
            let mut slot = self.open_show.lock().unwrap_or_else(|e| e.into_inner());
            match *slot {
                Some((open_id, at)) if open_id == id => slot.take().map(|_| at),
                _ => None,
            }
        };
        if let Some(at) = taken {
            self.record("show_to_first_pixel", at.elapsed().as_secs_f64() * 1000.0);
        }
    }

    /// Process start -> the global hotkey is registered and will answer.
    ///
    /// This is the honest half of the "login -> hotkey responsive < 500 ms" budget.
    /// The other half, session start to process start, belongs to Windows and can
    /// only be observed by rebooting, which is why it stays in the manual script.
    pub fn startup_ready(&self) {
        self.record(
            "start_to_hotkey_ready",
            self.started.elapsed().as_secs_f64() * 1000.0,
        );
    }

    pub fn record(&self, event: &str, ms: f64) {
        let Some(log) = &self.log else { return };
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let line = format!("{{\"event\":\"{event}\",\"ms\":{ms:.3},\"ts\":{ts}}}\n");
        let mut f = log.lock().unwrap_or_else(|e| e.into_inner());
        // Write failures are reported rather than swallowed: a bench run that
        // quietly loses half its samples is worse than one that fails.
        if let Err(e) = f.write_all(line.as_bytes()).and_then(|_| f.flush()) {
            eprintln!("[takyon] bench write failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("takyon-bench-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{tag}.jsonl"));
        let _ = std::fs::remove_file(&path);
        path
    }

    /// `Bench::from_env` reads a process-global variable, so construction is
    /// serialised. Only these tests touch it.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn bench_with_log(path: &std::path::Path) -> Bench {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        unsafe { std::env::set_var(LOG_ENV, path) };
        let b = Bench::from_env(Instant::now());
        unsafe { std::env::remove_var(LOG_ENV) };
        b
    }

    fn bench_without_log() -> Bench {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var_os(LOG_ENV);
        unsafe { std::env::remove_var(LOG_ENV) };
        let b = Bench::from_env(Instant::now());
        if let Some(v) = old {
            unsafe { std::env::set_var(LOG_ENV, v) };
        }
        b
    }

    #[test]
    fn v0_1_show_ids_are_unique_and_monotonic() {
        let b = bench_without_log();
        let a = b.mark_show();
        let c = b.mark_show();
        assert!(c > a);
    }

    /// A stale acknowledgement must not be credited to the newer show. If it were,
    /// a burst of hotkey presses would report the last show's paint against the
    /// first show's keypress, and the numbers would flatter us.
    #[test]
    fn v0_1_a_superseded_show_is_not_measured() {
        let path = temp_path("stale");
        let b = bench_with_log(&path);

        let first = b.mark_show();
        let second = b.mark_show();

        b.first_pixel(first); // late, superseded, must be dropped
        let after_stale = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(after_stale.lines().count(), 0);

        b.first_pixel(second);
        let after_live = std::fs::read_to_string(&path).unwrap();
        assert_eq!(after_live.lines().count(), 1);
        assert!(after_live.contains("show_to_first_pixel"));
    }

    /// Acknowledging twice must not double-count.
    #[test]
    fn v0_1_a_show_is_measured_at_most_once() {
        let path = temp_path("dup");
        let b = bench_with_log(&path);
        let id = b.mark_show();
        b.first_pixel(id);
        b.first_pixel(id);
        assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 1);
    }

    /// With no log path set, measurement is inert. The hot path must not pay for a
    /// feature that is off, and a normal run must not silently accumulate state.
    #[test]
    fn v0_1_measurement_is_inert_without_a_log_path() {
        let b = bench_without_log();
        assert!(!b.enabled());
        let id = b.mark_show();
        b.first_pixel(id);
        assert!(b.open_show.lock().unwrap().is_none());
    }

    /// One JSON object per line, parseable without a streaming parser.
    /// `scripts/bench.ts` reads it with `JSON.parse` per line.
    #[test]
    fn v0_1_records_are_one_json_object_per_line() {
        let path = temp_path("fmt");
        let b = bench_with_log(&path);
        b.record("some_event", 12.5);
        let text = std::fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(v["event"].as_str(), Some("some_event"));
        assert_eq!(v["ms"].as_f64(), Some(12.5));
    }
}
