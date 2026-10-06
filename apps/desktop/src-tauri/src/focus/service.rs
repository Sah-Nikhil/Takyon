//! The running Focus session, behind one lock: what IPC commands, the
//! enforcer loop and the Palette all go through.
//!
//! Every change is written to `focus.db` before it is acknowledged, so a
//! crash loses nothing a user saw happen (ADR-0036).

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;

use super::categories::builtins;
use super::duration::DurationError;
use super::identity::Process;
use super::procs::Running;
use super::rules::{Machine as RulesMachine, Refusal, Rules};
use super::session::{Clock, Friction, Snoozes, Strictness};
use super::store::{BlockList, EventKind, Outcome, Session, Store, StoreError};

/// What the service needs from the machine. Faked in tests.
pub trait Machine: Send + Sync {
    /// Installed games as `EntryId`s (`steam:730`) with their install dirs.
    fn games(&self) -> Vec<(String, Option<PathBuf>)>;
    /// Takyon's install directory; never blocked.
    fn takyon_dir(&self) -> PathBuf;
    /// Processes running now, for the start form's refusals.
    fn running(&self) -> Vec<Running>;
    /// False for a process Takyon cannot close: run as administrator.
    fn can_quit(&self, pid: u32) -> bool;
    /// Registered browsers' images and display names.
    fn browsers(&self) -> Vec<(PathBuf, String)>;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum FocusError {
    AlreadyRunning,
    NotRunning,
    /// Nothing on the Block list resolves to anything on this machine.
    NothingToBlock,
    /// Strict: retype the goal and wait out the countdown first.
    NeedsFriction,
    /// Strict sessions have no Snooze.
    NotAllowed,
    /// Nothing enforces a session on this platform yet (macOS until v0.12).
    Unsupported,
    Duration {
        reason: String,
    },
    Storage {
        reason: String,
    },
}

impl From<StoreError> for FocusError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::AlreadyRunning => FocusError::AlreadyRunning,
            StoreError::Db(e) => FocusError::Storage {
                reason: e.to_string(),
            },
        }
    }
}

impl From<rusqlite::Error> for FocusError {
    fn from(e: rusqlite::Error) -> Self {
        FocusError::Storage {
            reason: e.to_string(),
        }
    }
}

impl From<DurationError> for FocusError {
    fn from(e: DurationError) -> Self {
        FocusError::Duration {
            reason: format!("{e:?}"),
        }
    }
}

/// The running session as the frontend draws it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub running: bool,
    pub goal: String,
    pub strict: bool,
    pub remaining_ms: u64,
    pub ends_at: u64,
    pub paused: bool,
    pub break_ends_at: Option<u64>,
    /// Keys on the Block list that Focus will not touch, in words on the form.
    pub refused: Vec<String>,
    /// Strict countdown left, once the goal has been retyped.
    pub friction_ms: Option<u64>,
}

/// A running session plus what only lives in memory.
pub struct Active {
    pub session: Session,
    pub rules: Rules,
    pub refused: Vec<String>,
    pub snoozes: Snoozes,
    pub friction: Friction,
    /// Apps Focus quit, deduped by image, for Restore Terminated Apps.
    pub quit: Vec<Process>,
}

pub struct Focus {
    store: Store,
    machine: Box<dyn Machine>,
    live: Mutex<Option<Active>>,
    /// `quit` of the session that last ended, until the runtime takes it.
    ended_quit: Mutex<Vec<Process>>,
}

impl Focus {
    /// Open, resuming a session left running by a crash or reboot.
    pub fn open(store: Store, machine: Box<dyn Machine>) -> Self {
        let focus = Focus::new(store, machine);
        focus.resume_running();
        focus
    }

    /// Open without resuming: `setup` manages this, and the deferred thread
    /// calls [`Focus::resume_running`], which walks game libraries.
    pub fn new(store: Store, machine: Box<dyn Machine>) -> Self {
        Focus {
            store,
            machine,
            live: Mutex::new(None),
            ended_quit: Mutex::new(Vec::new()),
        }
    }

    /// Pick up a session left running by a crash or reboot. True if one was.
    pub fn resume_running(&self) -> bool {
        let Some(session) = self.store.running() else {
            return false;
        };
        let active = self.activate(session);
        *self.lock() = Some(active);
        true
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Active>> {
        self.live
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Rules for a session against this machine, as of now.
    fn activate(&self, session: Session) -> Active {
        let (rules, refused) = self.rules(&session.block_list);
        let friction = Friction::new(&session.goal);
        Active {
            session,
            rules,
            refused,
            snoozes: Snoozes::default(),
            friction,
            quit: Vec::new(),
        }
    }

    fn rules(&self, list: &BlockList) -> (Rules, Vec<String>) {
        let custom = self.store.custom_categories();
        let games = self.machine.games();
        let ids: Vec<String> = games.iter().map(|(id, _)| id.clone()).collect();
        let game_dir = |launcher: &str, id: &str| {
            let want = format!("{launcher}:{id}");
            games
                .iter()
                .find(|(g, _)| *g == want)
                .and_then(|(_, dir)| dir.clone())
        };
        let takyon_dir = self.machine.takyon_dir();
        let machine = RulesMachine {
            builtins: builtins(),
            custom: &custom,
            games: &ids,
            game_dir: &game_dir,
            takyon_dir: &takyon_dir,
        };
        Rules::build(list, &machine)
    }

    /// What the start form should list as refused, before starting: keys
    /// Focus never touches, then what is running beyond its reach.
    pub fn preview(&self, list: &BlockList) -> Vec<Refusal> {
        let (rules, protected) = self.rules(list);
        let mut out: Vec<Refusal> = protected
            .into_iter()
            .map(|key| Refusal::Protected { key })
            .collect();
        if !rules.is_empty() {
            let browsers = self.machine.browsers();
            let can_quit = |pid| self.machine.can_quit(pid);
            out.extend(rules.refusals(&self.machine.running(), &can_quit, &browsers));
        }
        out
    }

    pub fn start(
        &self,
        goal: &str,
        length_ms: u64,
        strictness: Strictness,
        list: &BlockList,
        now: u64,
    ) -> Result<View, FocusError> {
        let mut live = self.lock();
        if live.is_some() {
            return Err(FocusError::AlreadyRunning);
        }
        let (rules, _) = self.rules(list);
        if rules.is_empty() {
            return Err(FocusError::NothingToBlock);
        }
        let clock = Clock::start(now, length_ms);
        let id = self.store.start(goal, strictness, list, &clock)?;
        let session = Session {
            id,
            goal: goal.to_string(),
            strictness,
            block_list: list.clone(),
            clock,
        };
        *live = Some(self.activate(session));
        Ok(view_of(live.as_ref(), now))
    }

    pub fn view(&self, now: u64) -> View {
        view_of(self.lock().as_ref(), now)
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Change the running session and persist its clock. NotRunning if none.
    fn change(
        &self,
        now: u64,
        f: impl FnOnce(&mut Active) -> Result<(), FocusError>,
    ) -> Result<View, FocusError> {
        let mut live = self.lock();
        let active = live.as_mut().ok_or(FocusError::NotRunning)?;
        f(active)?;
        self.store
            .save_clock(active.session.id, &active.session.clock)?;
        Ok(view_of(live.as_ref(), now))
    }

    pub fn pause(&self, break_ms: Option<u64>, now: u64) -> Result<View, FocusError> {
        self.change(now, |a| {
            gate(a, now)?;
            a.session.clock.pause(now, break_ms);
            self.store
                .record(a.session.id, now, EventKind::Paused, None)?;
            Ok(())
        })
    }

    pub fn resume(&self, now: u64) -> Result<View, FocusError> {
        self.change(now, |a| {
            a.session.clock.resume(now);
            self.store
                .record(a.session.id, now, EventKind::Resumed, None)?;
            Ok(())
        })
    }

    /// Edit: more time. Never gated; adding time is not an escape.
    pub fn extend(&self, ms: u64, now: u64) -> Result<View, FocusError> {
        self.change(now, |a| Ok(a.session.clock.extend(ms)?))
    }

    pub fn complete(&self, now: u64) -> Result<View, FocusError> {
        let mut live = self.lock();
        let active = live.as_mut().ok_or(FocusError::NotRunning)?;
        gate(active, now)?;
        self.store
            .finish(active.session.id, now, Outcome::Completed)?;
        self.end(&mut live);
        Ok(View::default())
    }

    pub fn snooze(&self, key: &str, ms: u64, now: u64) -> Result<(), FocusError> {
        let mut live = self.lock();
        let active = live.as_mut().ok_or(FocusError::NotRunning)?;
        if !active.session.strictness.allows_snooze() {
            return Err(FocusError::NotAllowed);
        }
        active.snoozes.snooze(key, now, ms);
        self.store
            .record(active.session.id, now, EventKind::Snoozed, Some(key))?;
        Ok(())
    }

    /// Strict: the retyped goal. True starts the countdown.
    pub fn type_goal(&self, typed: &str, now: u64) -> Result<bool, FocusError> {
        self.with_active(|a| a.friction.type_goal(typed, now))
            .ok_or(FocusError::NotRunning)
    }

    /// Palette dismissed mid-countdown: start over.
    pub fn reset_friction(&self) {
        self.with_active(|a| a.friction.reset());
    }

    /// Finish a session whose clock has run out. Some(outcome) when it did.
    pub fn tick(&self, now: u64) -> Option<Outcome> {
        let mut live = self.lock();
        let active = live.as_ref()?;
        if !active.session.clock.is_over(now) {
            return None;
        }
        let _ = self.store.finish(active.session.id, now, Outcome::Expired);
        self.end(&mut live);
        Some(Outcome::Expired)
    }

    /// Machine suspending: the clock stops (`PBT_APMSUSPEND`).
    pub fn sleep(&self, now: u64) {
        let _ = self.change(now, |a| {
            a.session.clock.sleep(now);
            Ok(())
        });
    }

    pub fn wake(&self, now: u64) {
        let _ = self.change(now, |a| {
            a.session.clock.wake(now);
            Ok(())
        });
    }

    /// Remember an app Focus quit, for Restore Terminated Apps.
    pub fn note_quit(&self, process: &Process) {
        self.with_active(|a| {
            if !a.quit.iter().any(|q| q.image == process.image) {
                a.quit.push(process.clone());
            }
        });
    }

    /// Apps quit during the session that just ended. Empty while one runs.
    pub fn take_quit(&self) -> Vec<Process> {
        std::mem::take(&mut *self.ended_quit.lock().unwrap_or_else(|p| p.into_inner()))
    }

    /// Drop the running session, keeping what it quit for the runtime.
    fn end(&self, live: &mut Option<Active>) {
        if let Some(active) = live.take() {
            *self.ended_quit.lock().unwrap_or_else(|p| p.into_inner()) = active.quit;
        }
    }

    /// Run `f` against the running session, if there is one. The enforcer's door.
    pub fn with_active<R>(&self, f: impl FnOnce(&mut Active) -> R) -> Option<R> {
        self.lock().as_mut().map(f)
    }
}

/// Strict's hurdle before Pause or early Complete; spent once passed.
fn gate(active: &mut Active, now: u64) -> Result<(), FocusError> {
    if !active.session.strictness.needs_friction() {
        return Ok(());
    }
    if !active.friction.is_ready(now) {
        return Err(FocusError::NeedsFriction);
    }
    active.friction.reset();
    Ok(())
}

fn view_of(active: Option<&Active>, now: u64) -> View {
    let Some(a) = active else {
        return View::default();
    };
    let clock = &a.session.clock;
    View {
        running: true,
        goal: a.session.goal.clone(),
        strict: a.session.strictness == Strictness::Strict,
        remaining_ms: clock.remaining(now),
        ends_at: clock.ends_at(now),
        paused: clock.is_paused(now),
        break_ends_at: clock.break_ends_at().filter(|&b| b > now),
        refused: a.refused.clone(),
        friction_ms: a.friction.remaining(now),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: u64 = 60_000;
    const T0: u64 = 1_760_000_000_000;

    struct Fake;

    impl Machine for Fake {
        fn games(&self) -> Vec<(String, Option<PathBuf>)> {
            vec![(
                "steam:730".into(),
                Some(PathBuf::from(r"D:\Steam\common\CS")),
            )]
        }
        fn takyon_dir(&self) -> PathBuf {
            PathBuf::from(r"D:\Takyon")
        }
        fn running(&self) -> Vec<Running> {
            let elevated = |image: &str| Running {
                pid: 7,
                process: Process {
                    image: image.into(),
                    session_id: 1,
                    ..Default::default()
                },
            };
            vec![elevated(r"D:\Discord\Discord.exe")]
        }
        fn can_quit(&self, pid: u32) -> bool {
            pid != 7
        }
        fn browsers(&self) -> Vec<(PathBuf, String)> {
            Vec::new()
        }
    }

    fn focus() -> Focus {
        Focus::open(Store::open(None).unwrap(), Box::new(Fake))
    }

    fn list(items: &[&str]) -> BlockList {
        BlockList {
            categories: vec![],
            items: items.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// `interface FocusView` in `packages/shared/src/ipc.ts`, copied by hand.
    #[test]
    fn v0_16_the_view_matches_ipc_ts() {
        let v = serde_json::to_value(View::default()).unwrap();
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "breakEndsAt",
                "endsAt",
                "frictionMs",
                "goal",
                "paused",
                "refused",
                "remainingMs",
                "running",
                "strict"
            ]
        );
        let e = serde_json::to_value(FocusError::NeedsFriction).unwrap();
        assert_eq!(e["kind"], "needsFriction");
    }

    #[test]
    fn v0_16_starting_a_session_makes_it_the_running_one() {
        let focus = focus();
        let view = focus
            .start(
                "Write",
                30 * MIN,
                Strictness::Soft,
                &list(&["exe:discord.exe"]),
                T0,
            )
            .unwrap();
        assert!(view.running);
        assert_eq!(
            (view.goal.as_str(), view.remaining_ms, view.ends_at),
            ("Write", 30 * MIN, T0 + 30 * MIN)
        );
        assert_eq!(focus.view(T0 + 10 * MIN).remaining_ms, 20 * MIN);
    }

    #[test]
    fn v0_16_a_session_with_nothing_to_block_or_a_second_session_is_refused() {
        let focus = focus();
        assert_eq!(
            focus.start("x", MIN, Strictness::Soft, &list(&[]), T0),
            Err(FocusError::NothingToBlock)
        );
        assert_eq!(
            focus.start("x", MIN, Strictness::Soft, &list(&["exe:explorer.exe"]), T0),
            Err(FocusError::NothingToBlock)
        );
        focus
            .start("x", MIN, Strictness::Soft, &list(&["site:x.com"]), T0)
            .unwrap();
        assert_eq!(
            focus.start("y", MIN, Strictness::Soft, &list(&["site:x.com"]), T0),
            Err(FocusError::AlreadyRunning)
        );
    }

    #[test]
    fn v0_16_refused_keys_are_reported_and_the_rest_still_blocks() {
        let focus = focus();
        let view = focus
            .start(
                "x",
                MIN,
                Strictness::Soft,
                &list(&["exe:explorer.exe", "exe:slack.exe"]),
                T0,
            )
            .unwrap();
        assert_eq!(view.refused, ["exe:explorer.exe"]);
        let slack = Running {
            pid: 9,
            process: Process {
                image: r"D:\Slack\slack.exe".into(),
                session_id: 1,
                ..Default::default()
            },
        };
        let quit = focus.with_active(|a| {
            a.rules.to_quit(
                &[slack],
                &mut a.snoozes,
                None,
                T0,
                std::path::Path::new(r"D:\Takyon"),
            )
        });
        assert_eq!(quit.unwrap().len(), 1);
    }

    fn running(focus: &Focus, strictness: Strictness) {
        focus
            .start(
                "Write report",
                30 * MIN,
                strictness,
                &list(&["exe:discord.exe"]),
                T0,
            )
            .unwrap();
    }

    #[test]
    fn v0_16_the_preview_names_protected_keys_then_what_runs_elevated() {
        let focus = focus();
        assert_eq!(
            focus.preview(&list(&["exe:explorer.exe", "exe:discord.exe"])),
            [
                Refusal::Protected {
                    key: "exe:explorer.exe".into()
                },
                Refusal::Elevated {
                    key: "exe:discord.exe".into(),
                    name: "Discord".into()
                }
            ]
        );
        assert!(focus.preview(&list(&["exe:slack.exe"])).is_empty());
    }

    #[test]
    fn v0_16_soft_pauses_resumes_extends_and_completes_freely() {
        let focus = focus();
        running(&focus, Strictness::Soft);
        let paused = focus.pause(Some(5 * MIN), T0 + MIN).unwrap();
        assert!(paused.paused);
        assert_eq!(paused.break_ends_at, Some(T0 + 6 * MIN));
        focus.resume(T0 + 3 * MIN).unwrap();
        assert_eq!(focus.view(T0 + 3 * MIN).ends_at, T0 + 32 * MIN);
        assert_eq!(
            focus.extend(10 * MIN, T0 + 3 * MIN).unwrap().ends_at,
            T0 + 42 * MIN
        );
        assert!(!focus.complete(T0 + 4 * MIN).unwrap().running);
        assert_eq!(focus.store().history(0)[0].outcome, Outcome::Completed);
        assert_eq!(focus.pause(None, T0 + 5 * MIN), Err(FocusError::NotRunning));
    }

    #[test]
    fn v0_16_soft_snoozes_and_strict_does_not() {
        let focus = focus();
        running(&focus, Strictness::Soft);
        focus.snooze("exe:discord.exe", 3 * MIN, T0 + MIN).unwrap();
        let spared =
            focus.with_active(|a| a.snoozes.allows("exe:discord.exe", T0 + 2 * MIN, false));
        assert_eq!(spared, Some(true));

        let strict = self::focus();
        running(&strict, Strictness::Strict);
        assert_eq!(
            strict.snooze("exe:discord.exe", 3 * MIN, T0 + MIN),
            Err(FocusError::NotAllowed)
        );
    }

    /// Strict: Pause and Complete wait for the retyped goal plus 30 s.
    #[test]
    fn v0_16_strict_puts_friction_before_pause_and_complete() {
        let focus = focus();
        running(&focus, Strictness::Strict);
        assert_eq!(focus.pause(None, T0 + MIN), Err(FocusError::NeedsFriction));
        assert_eq!(focus.complete(T0 + MIN), Err(FocusError::NeedsFriction));
        assert!(!focus.type_goal("write report", T0 + MIN).unwrap());
        assert!(focus.type_goal("Write report", T0 + MIN).unwrap());
        assert_eq!(focus.view(T0 + MIN + 10_000).friction_ms, Some(20_000));
        assert_eq!(
            focus.complete(T0 + MIN + 10_000),
            Err(FocusError::NeedsFriction)
        );
        assert!(focus.pause(None, T0 + MIN + 30_000).unwrap().paused);
        // Spent: the next early exit starts over.
        focus.resume(T0 + 2 * MIN).unwrap();
        assert_eq!(focus.complete(T0 + 3 * MIN), Err(FocusError::NeedsFriction));
        focus.reset_friction();
        assert_eq!(focus.view(T0 + 3 * MIN).friction_ms, None);
    }

    #[test]
    fn v0_16_a_session_expires_on_tick_and_is_recorded() {
        let focus = focus();
        running(&focus, Strictness::Strict);
        assert_eq!(focus.tick(T0 + 29 * MIN), None);
        assert_eq!(focus.tick(T0 + 30 * MIN), Some(Outcome::Expired));
        assert!(!focus.view(T0 + 30 * MIN).running);
        assert_eq!(focus.store().history(0).len(), 1);
    }

    #[test]
    fn v0_16_sleep_pauses_the_session_until_wake() {
        let focus = focus();
        running(&focus, Strictness::Strict);
        focus.sleep(T0 + 5 * MIN);
        focus.wake(T0 + 65 * MIN);
        assert_eq!(focus.view(T0 + 65 * MIN).remaining_ms, 25 * MIN);
    }

    /// Restore Terminated Apps reopens what Focus quit, once each, after the end.
    #[test]
    fn v0_16_apps_quit_during_a_session_are_handed_back_when_it_ends() {
        let focus = focus();
        running(&focus, Strictness::Soft);
        let discord = Process {
            image: r"D:\Discord\Discord.exe".into(),
            session_id: 1,
            ..Default::default()
        };
        focus.note_quit(&discord);
        focus.note_quit(&discord);
        assert!(
            focus.take_quit().is_empty(),
            "nothing handed back mid-session"
        );
        focus.complete(T0 + MIN).unwrap();
        let back = focus.take_quit();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].image, discord.image);
        assert!(focus.take_quit().is_empty());
    }

    /// The Strict promise: a new process picks up where the old one died.
    #[test]
    fn v0_16_a_running_session_resumes_in_a_new_process() {
        let dir = std::env::temp_dir().join(format!("takyon-focus-resume-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        {
            let focus = Focus::open(Store::open(Some(dir.clone())).unwrap(), Box::new(Fake));
            running(&focus, Strictness::Strict);
        }
        let focus = Focus::open(Store::open(Some(dir.clone())).unwrap(), Box::new(Fake));
        let view = focus.view(T0 + 5 * MIN);
        assert!(view.running && view.strict);
        assert_eq!(view.remaining_ms, 25 * MIN);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
