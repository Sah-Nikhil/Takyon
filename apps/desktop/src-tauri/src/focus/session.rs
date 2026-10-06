//! A Focus session's clock, Snoozes, and Strict friction. Pure state, no OS.
//!
//! Times are wall-clock ms since the Unix epoch, passed in by the caller. A
//! session ends at start + planned length + every Pause, sleep included.

use serde::{Deserialize, Serialize};

use super::duration::{DurationError, MAX_MS};

/// A session's clock. Serialised whole into focus.db.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clock {
    started_ms: u64,
    planned_ms: u64,
    /// Finished Pauses, summed.
    paused_ms: u64,
    pause: Option<Pause>,
    /// Suspended since, if asleep.
    asleep: Option<u64>,
}

/// The Pause in force. `until` None = open-ended (sleep, or no break length).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Pause {
    since: u64,
    until: Option<u64>,
}

impl Clock {
    pub fn start(now: u64, planned_ms: u64) -> Self {
        Clock {
            started_ms: now,
            planned_ms,
            paused_ms: 0,
            pause: None,
            asleep: None,
        }
    }

    pub fn started_at(&self) -> u64 {
        self.started_ms
    }

    pub fn planned_ms(&self) -> u64 {
        self.planned_ms
    }

    /// Stop the clock. `break_ms` ends it by itself; None waits for [`resume`].
    ///
    /// [`resume`]: Clock::resume
    pub fn pause(&mut self, now: u64, break_ms: Option<u64>) {
        self.settle(now);
        if self.pause.is_none() {
            self.pause = Some(Pause {
                since: now,
                until: break_ms.map(|b| now + b),
            });
        }
    }

    pub fn resume(&mut self, now: u64) {
        self.settle(now);
        if let Some(p) = self.pause.take() {
            self.paused_ms += now.saturating_sub(p.since);
        }
    }

    pub fn is_paused(&self, now: u64) -> bool {
        self.pause
            .is_some_and(|p| p.until.map_or(true, |u| now < u))
    }

    /// When the running break ends, if one is running.
    pub fn break_ends_at(&self) -> Option<u64> {
        self.pause.and_then(|p| p.until)
    }

    /// Add time to a running session. Planned length stays within a day.
    pub fn extend(&mut self, ms: u64) -> Result<(), DurationError> {
        let planned = self.planned_ms.saturating_add(ms);
        if planned > MAX_MS {
            return Err(DurationError::OverDay);
        }
        self.planned_ms = planned;
        Ok(())
    }

    /// Machine suspending (`PBT_APMSUSPEND`).
    pub fn sleep(&mut self, now: u64) {
        self.settle(now);
        self.asleep.get_or_insert(now);
    }

    /// Machine resumed (`PBT_APMRESUMEAUTOMATIC`).
    ///
    /// Sleep counts as Pause. A break that ran out mid-sleep ends at wake, not
    /// at its own end: the session never runs while asleep.
    pub fn wake(&mut self, now: u64) {
        let Some(slept) = self.asleep.take() else {
            return;
        };
        match self.pause {
            None => self.paused_ms += now.saturating_sub(slept),
            Some(Pause {
                since,
                until: Some(until),
            }) if until <= now => {
                self.paused_ms += now.saturating_sub(since);
                self.pause = None;
            }
            Some(_) => {}
        }
    }

    /// Fold a break that has run out into `paused_ms`.
    fn settle(&mut self, now: u64) {
        if let Some(Pause {
            since,
            until: Some(until),
        }) = self.pause
        {
            if now >= until {
                self.paused_ms += until - since;
                self.pause = None;
            }
        }
    }

    /// Pause time up to `now`, finished and current.
    fn paused_total(&self, now: u64) -> u64 {
        let current = self.pause.map_or(0, |p| {
            let end = p.until.map_or(now, |u| u.min(now));
            end.saturating_sub(p.since)
        });
        self.paused_ms + current
    }

    /// ms left; zero once over.
    pub fn remaining(&self, now: u64) -> u64 {
        self.ends_at(now).saturating_sub(now)
    }

    pub fn is_over(&self, now: u64) -> bool {
        self.remaining(now) == 0
    }

    /// When the session ends, as things stand at `now`.
    pub fn ends_at(&self, now: u64) -> u64 {
        self.started_ms + self.planned_ms + self.paused_total(now)
    }
}

/// How hard a session is to leave early (ADR-0036).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Strictness {
    Soft,
    Strict,
}

impl Strictness {
    pub fn allows_snooze(self) -> bool {
        self == Strictness::Soft
    }

    /// Pause and early Complete go through [`Friction`] first.
    pub fn needs_friction(self) -> bool {
        self == Strictness::Strict
    }
}

/// Strict's countdown after the goal is retyped.
pub const STRICT_COUNTDOWN_MS: u64 = 30_000;

/// Strict's hurdle before Pause or early Complete.
#[derive(Clone, Debug)]
pub struct Friction {
    goal: String,
    typed_at: Option<u64>,
}

impl Friction {
    pub fn new(goal: &str) -> Self {
        Friction {
            goal: goal.to_string(),
            typed_at: None,
        }
    }

    /// Exact match, case included; only outer whitespace forgiven.
    pub fn type_goal(&mut self, typed: &str, now: u64) -> bool {
        let ok = typed.trim() == self.goal.trim();
        if ok {
            self.typed_at = Some(now);
        }
        ok
    }

    /// Countdown left, or None before the goal is typed.
    pub fn remaining(&self, now: u64) -> Option<u64> {
        self.typed_at
            .map(|t| (t + STRICT_COUNTDOWN_MS).saturating_sub(now))
    }

    pub fn is_ready(&self, now: u64) -> bool {
        self.remaining(now) == Some(0)
    }

    pub fn reset(&mut self) {
        self.typed_at = None;
    }
}

/// Targets let back in for a while, keyed by Block list item key.
#[derive(Clone, Debug, Default)]
pub struct Snoozes {
    until: std::collections::HashMap<String, u64>,
}

impl Snoozes {
    pub fn snooze(&mut self, target: &str, now: u64, ms: u64) {
        self.until.insert(target.to_string(), now + ms);
    }

    /// Whether `target` may run now. `in_front`: it has the foreground.
    ///
    /// Run-out snooze holds while `in_front`, so nothing quits mid-keystroke;
    /// first check with it behind drops the snooze for good.
    pub fn allows(&mut self, target: &str, now: u64, in_front: bool) -> bool {
        match self.until.get(target) {
            None => false,
            Some(&until) if now < until || in_front => true,
            Some(_) => {
                self.until.remove(target);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: u64 = 60_000;
    const T0: u64 = 1_760_000_000_000;

    #[test]
    fn v0_16_a_running_session_counts_down() {
        let clock = Clock::start(T0, 30 * MIN);
        assert_eq!(clock.remaining(T0), 30 * MIN);
        assert_eq!(clock.remaining(T0 + 10 * MIN), 20 * MIN);
        assert_eq!(clock.remaining(T0 + 30 * MIN), 0);
        assert_eq!(clock.remaining(T0 + 99 * MIN), 0);
        assert!(clock.is_over(T0 + 30 * MIN));
        assert!(!clock.is_over(T0 + 29 * MIN));
    }

    #[test]
    fn v0_16_a_pause_stops_the_clock_and_resume_restarts_it() {
        let mut clock = Clock::start(T0, 30 * MIN);
        clock.pause(T0 + 10 * MIN, None);
        assert!(clock.is_paused(T0 + 15 * MIN));
        assert_eq!(clock.remaining(T0 + 15 * MIN), 20 * MIN);
        clock.resume(T0 + 25 * MIN);
        assert!(!clock.is_paused(T0 + 25 * MIN));
        assert_eq!(clock.remaining(T0 + 25 * MIN), 20 * MIN);
        assert_eq!(clock.ends_at(T0 + 25 * MIN), T0 + 45 * MIN);
    }

    /// A Pause picked with a break length ends by itself; nothing calls resume.
    #[test]
    fn v0_16_a_break_ends_on_its_own() {
        let mut clock = Clock::start(T0, 30 * MIN);
        clock.pause(T0 + 10 * MIN, Some(5 * MIN));
        assert!(clock.is_paused(T0 + 14 * MIN));
        assert!(!clock.is_paused(T0 + 15 * MIN));
        assert_eq!(clock.remaining(T0 + 15 * MIN), 20 * MIN);
        assert_eq!(clock.remaining(T0 + 20 * MIN), 15 * MIN);
        assert_eq!(clock.break_ends_at(), Some(T0 + 15 * MIN));
    }

    #[test]
    fn v0_16_pauses_accumulate() {
        let mut clock = Clock::start(T0, 30 * MIN);
        clock.pause(T0 + 5 * MIN, Some(5 * MIN));
        clock.pause(T0 + 20 * MIN, None);
        clock.resume(T0 + 30 * MIN);
        assert_eq!(clock.ends_at(T0 + 30 * MIN), T0 + 45 * MIN);
    }

    #[test]
    fn v0_16_sleep_stops_the_clock_until_wake() {
        let mut clock = Clock::start(T0, 30 * MIN);
        clock.sleep(T0 + 10 * MIN);
        clock.wake(T0 + 70 * MIN);
        assert_eq!(clock.remaining(T0 + 70 * MIN), 20 * MIN);
        assert!(!clock.is_paused(T0 + 70 * MIN));
    }

    #[test]
    fn v0_16_a_break_outlasting_sleep_still_ends_on_time() {
        let mut clock = Clock::start(T0, 30 * MIN);
        clock.pause(T0 + 10 * MIN, Some(30 * MIN));
        clock.sleep(T0 + 15 * MIN);
        clock.wake(T0 + 20 * MIN);
        assert!(clock.is_paused(T0 + 39 * MIN));
        assert!(!clock.is_paused(T0 + 40 * MIN));
        assert_eq!(clock.ends_at(T0 + 40 * MIN), T0 + 60 * MIN);
    }

    /// The session never runs while the machine sleeps, even after a break
    /// would have ended.
    #[test]
    fn v0_16_a_break_ending_during_sleep_pauses_until_wake() {
        let mut clock = Clock::start(T0, 30 * MIN);
        clock.pause(T0 + 10 * MIN, Some(5 * MIN));
        clock.sleep(T0 + 12 * MIN);
        clock.wake(T0 + 60 * MIN);
        assert!(!clock.is_paused(T0 + 60 * MIN));
        assert_eq!(clock.remaining(T0 + 60 * MIN), 20 * MIN);
    }

    #[test]
    fn v0_16_wake_without_sleep_changes_nothing() {
        let mut clock = Clock::start(T0, 30 * MIN);
        clock.wake(T0 + 5 * MIN);
        assert_eq!(clock.ends_at(T0 + 5 * MIN), T0 + 30 * MIN);
    }

    #[test]
    fn v0_16_edit_adds_time_up_to_a_day_of_planned_length() {
        let mut clock = Clock::start(T0, 30 * MIN);
        assert_eq!(clock.extend(10 * MIN), Ok(()));
        assert_eq!(clock.ends_at(T0), T0 + 40 * MIN);
        assert_eq!(clock.extend(MAX_MS), Err(DurationError::OverDay));
        assert_eq!(clock.ends_at(T0), T0 + 40 * MIN);
        assert_eq!(clock.extend(MAX_MS - 40 * MIN), Ok(()));
    }

    #[test]
    fn v0_16_an_unsnoozed_target_is_never_allowed() {
        let mut snoozes = Snoozes::default();
        assert!(!snoozes.allows("exe:discord.exe", T0, true));
    }

    #[test]
    fn v0_16_a_snooze_allows_its_target_until_it_runs_out() {
        let mut snoozes = Snoozes::default();
        snoozes.snooze("exe:discord.exe", T0, 3 * MIN);
        assert!(snoozes.allows("exe:discord.exe", T0 + MIN, false));
        assert!(!snoozes.allows("exe:slack.exe", T0 + MIN, true));
        assert!(!snoozes.allows("exe:discord.exe", T0 + 3 * MIN, false));
    }

    /// Expiry never quits an app mid-keystroke: it waits for the app to lose
    /// the foreground, then the snooze is gone for good.
    #[test]
    fn v0_16_an_expired_snooze_holds_while_its_app_is_in_front() {
        let mut snoozes = Snoozes::default();
        snoozes.snooze("exe:discord.exe", T0, 3 * MIN);
        assert!(snoozes.allows("exe:discord.exe", T0 + 4 * MIN, true));
        assert!(snoozes.allows("exe:discord.exe", T0 + 9 * MIN, true));
        assert!(!snoozes.allows("exe:discord.exe", T0 + 10 * MIN, false));
        assert!(!snoozes.allows("exe:discord.exe", T0 + 11 * MIN, true));
    }

    #[test]
    fn v0_16_snoozing_again_starts_a_fresh_snooze() {
        let mut snoozes = Snoozes::default();
        snoozes.snooze("site:youtube.com", T0, 3 * MIN);
        snoozes.snooze("site:youtube.com", T0 + 2 * MIN, 3 * MIN);
        assert!(snoozes.allows("site:youtube.com", T0 + 4 * MIN, false));
    }

    #[test]
    fn v0_16_soft_lets_everything_through_strict_puts_friction_in_front() {
        assert!(Strictness::Soft.allows_snooze());
        assert!(!Strictness::Soft.needs_friction());
        assert!(!Strictness::Strict.allows_snooze());
        assert!(Strictness::Strict.needs_friction());
    }

    #[test]
    fn v0_16_friction_is_the_goal_retyped_then_thirty_seconds() {
        let mut friction = Friction::new("Write report");
        assert!(!friction.is_ready(T0));
        assert_eq!(friction.remaining(T0), None);
        assert!(!friction.type_goal("write report", T0));
        assert!(friction.type_goal("  Write report ", T0));
        assert_eq!(friction.remaining(T0 + 10_000), Some(20_000));
        assert!(!friction.is_ready(T0 + 29_999));
        assert!(friction.is_ready(T0 + STRICT_COUNTDOWN_MS));
    }

    /// Dismissing the Palette mid-countdown starts it over.
    #[test]
    fn v0_16_friction_resets_on_dismiss() {
        let mut friction = Friction::new("Write report");
        friction.type_goal("Write report", T0);
        friction.reset();
        assert!(!friction.is_ready(T0 + 60_000));
        assert_eq!(friction.remaining(T0 + 60_000), None);
    }

    /// focus.db stores the clock as JSON; a restart must resume it exactly.
    #[test]
    fn v0_16_a_clock_survives_a_round_trip_through_json() {
        let mut clock = Clock::start(T0, 30 * MIN);
        clock.pause(T0 + 5 * MIN, Some(5 * MIN));
        clock.sleep(T0 + 7 * MIN);
        let json = serde_json::to_string(&clock).unwrap();
        let back: Clock = serde_json::from_str(&json).unwrap();
        assert_eq!(back, clock);
        assert_eq!((back.started_at(), back.planned_ms()), (T0, 30 * MIN));
        assert_eq!(
            serde_json::to_string(&Strictness::Strict).unwrap(),
            "\"strict\""
        );
    }

    /// Pause while paused keeps the first; resume while running does nothing.
    #[test]
    fn v0_16_pause_and_resume_are_idempotent() {
        let mut clock = Clock::start(T0, 30 * MIN);
        clock.resume(T0 + MIN);
        clock.pause(T0 + 10 * MIN, None);
        clock.pause(T0 + 12 * MIN, None);
        clock.resume(T0 + 20 * MIN);
        assert_eq!(clock.ends_at(T0 + 20 * MIN), T0 + 40 * MIN);
    }
}
