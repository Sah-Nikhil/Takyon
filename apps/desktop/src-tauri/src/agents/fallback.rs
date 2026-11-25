//! Walking `!c`'s order past an Agent that cannot answer (ADR-0041).
//!
//! An Agent that fails before writing any text — not installed, signed out,
//! out of usage — hands the Turn to the next one in the order. One that failed
//! is benched: still asked, but last, until its usage window resets or
//! `COOLDOWN` passes. Without the bench every `!c` would pay one doomed spawn
//! (seconds, for Claude) before the Agent that can answer is asked.
//!
//! In memory only. A restart forgets every bench, which costs one doomed spawn
//! per benched Agent and nothing else.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::AgentKind;

/// How long a failed Agent stays benched when it named no reset time.
pub const COOLDOWN: Duration = Duration::from_secs(10 * 60);

/// Longest bench an Agent's own reset time can buy. Claude's weekly window is
/// seven days; past that the number is more likely wrong than real.
pub const MAX_BENCH: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Agents that failed recently, and until when they are asked last.
#[derive(Default, Debug)]
pub struct Bench {
    until: Mutex<HashMap<AgentKind, Instant>>,
}

impl Bench {
    /// Bench `kind`. `resets_at` is the Agent's own window end, Unix seconds.
    pub fn bench(&self, kind: AgentKind, resets_at: Option<u64>, now: Instant) {
        let span = resets_at
            // Checked: `SystemTime + Duration` panics on overflow, and this
            // number is whatever the Agent printed.
            .and_then(|at| UNIX_EPOCH.checked_add(Duration::from_secs(at)))
            .and_then(|at| at.duration_since(SystemTime::now()).ok())
            .map(|left| left.min(MAX_BENCH))
            .unwrap_or(COOLDOWN);
        self.lock().insert(kind, now + span);
    }

    /// Unbench one Agent: it answered, or Settings changed something about it.
    pub fn clear(&self, kind: AgentKind) {
        self.lock().remove(&kind);
    }

    pub fn clear_all(&self) {
        self.lock().clear();
    }

    pub fn is_benched(&self, kind: AgentKind, now: Instant) -> bool {
        self.lock().get(&kind).is_some_and(|until| *until > now)
    }

    /// The order with benched Agents moved last, each half keeping its order.
    ///
    /// Never drops one: a benched Agent may have reset early, and asking it last
    /// costs nothing when an earlier one answers.
    pub fn arrange(&self, order: &[AgentKind], now: Instant) -> Vec<AgentKind> {
        let (benched, ready): (Vec<AgentKind>, Vec<AgentKind>) = order
            .iter()
            .copied()
            .partition(|kind| self.is_benched(*kind, now));
        ready.into_iter().chain(benched).collect()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<AgentKind, Instant>> {
        self.until.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The process-wide bench. `Turns` writes it; the Palette's `!c` row reads it.
pub fn shared() -> Arc<Bench> {
    static SHARED: OnceLock<Arc<Bench>> = OnceLock::new();
    SHARED.get_or_init(|| Arc::new(Bench::default())).clone()
}

/// The Agents one Turn may walk, first to last.
///
/// `first` leads because the Palette already named it. A resumed session is
/// that Agent's alone: another Agent cannot resume it, and answering a
/// follow-up with no context is worse than saying why the first one stopped.
pub fn chain(first: AgentKind, route: &[AgentKind], resumed: bool) -> Vec<AgentKind> {
    if resumed {
        return vec![first];
    }
    std::iter::once(first)
        .chain(route.iter().copied().filter(|kind| *kind != first))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use AgentKind::{Claude, Codex, OpenCode};

    /// A benched Agent is asked last, not dropped, and the rest keep their order.
    #[test]
    fn v0_11_2_a_benched_agent_moves_last_and_stays_in_the_order() {
        let bench = Bench::default();
        let now = Instant::now();
        bench.bench(Claude, None, now);
        assert_eq!(
            bench.arrange(&[Claude, Codex, OpenCode], now),
            vec![Codex, OpenCode, Claude]
        );
    }

    /// The bench expires on its own: a limit that reset must not keep the
    /// preferred Agent demoted for the life of the process.
    #[test]
    fn v0_11_2_a_bench_expires_after_its_cooldown() {
        let bench = Bench::default();
        let now = Instant::now();
        bench.bench(Claude, None, now);
        assert!(bench.is_benched(Claude, now + COOLDOWN - Duration::from_secs(1)));
        assert!(!bench.is_benched(Claude, now + COOLDOWN));
        assert_eq!(
            bench.arrange(&[Claude, Codex], now + COOLDOWN),
            vec![Claude, Codex]
        );
    }

    /// An Agent's own reset time outranks the flat cooldown, capped at a week.
    #[test]
    fn v0_11_2_a_reset_time_sets_the_bench_length() {
        let bench = Bench::default();
        let now = Instant::now();
        let in_an_hour = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3_600;
        bench.bench(Claude, Some(in_an_hour), now);
        assert!(bench.is_benched(Claude, now + COOLDOWN * 2));
        assert!(!bench.is_benched(Claude, now + Duration::from_secs(3_700)));

        bench.bench(Codex, Some(u64::MAX / 4), now);
        assert!(!bench.is_benched(Codex, now + MAX_BENCH + Duration::from_secs(1)));
    }

    /// A reset time already past falls back to the cooldown rather than to no
    /// bench at all: the Agent did just fail.
    #[test]
    fn v0_11_2_a_past_reset_time_still_benches_for_the_cooldown() {
        let bench = Bench::default();
        let now = Instant::now();
        bench.bench(Claude, Some(1), now);
        assert!(bench.is_benched(Claude, now + Duration::from_secs(1)));
    }

    #[test]
    fn v0_11_2_clearing_unbenches() {
        let bench = Bench::default();
        let now = Instant::now();
        bench.bench(Claude, None, now);
        bench.bench(Codex, None, now);
        bench.clear(Claude);
        assert!(!bench.is_benched(Claude, now));
        assert!(bench.is_benched(Codex, now));
        bench.clear_all();
        assert!(!bench.is_benched(Codex, now));
    }

    /// The Palette's Agent leads, the rest of the route follows once each.
    #[test]
    fn v0_11_2_a_chain_leads_with_the_named_agent() {
        assert_eq!(
            chain(Codex, &[Claude, Codex, OpenCode], false),
            vec![Codex, Claude, OpenCode]
        );
        // Switched off is absent from the route, so absent from the chain.
        assert_eq!(chain(Claude, &[Claude], false), vec![Claude]);
    }

    /// A follow-up never changes Agent: the session lives in the first one.
    #[test]
    fn v0_11_2_a_resumed_turn_has_no_fallback() {
        assert_eq!(
            chain(Claude, &[Claude, Codex, OpenCode], true),
            vec![Claude]
        );
    }
}
