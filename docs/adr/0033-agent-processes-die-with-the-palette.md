---
status: accepted
---

# Agent processes die with the Palette

Every process that `!c` or `!s` starts is gone when the Palette hides. That
includes the process tree behind it. Nothing an Agent runs survives from one
summon to the next.

## The problem

v0.8 stopped a Turn with `Child::kill`, which calls `TerminateProcess` on the one
process Takyon created. For an npm-installed Agent that process is `cmd.exe`,
which starts `node`, which starts the real CLI. Windows does not kill children
along with their parent. After a cancel, `node` and the Agent kept running and
kept spending the user's tokens, with nobody reading the pipe.

Dismissal only reached a Turn through React. `Palette.tsx` cleared the Ask view,
`useTurn` unmounted, and its cleanup called `agentCancel`. Three races got past
that:

- **Hide while `agent_ask` was in flight.** `useTurn` only learns the Turn id when
  the invoke resolves, so the cleanup had nothing to cancel, and the Turn started
  after the Palette had gone.
- **A `!s` whose Turn started after the hide.** The search thread checks for
  cancellation between steps, but a hide that landed after its last check still
  started a Turn.
- **A cancel between spawn and registration.** `Turns::run` registered the child
  only after starting its stderr thread, so a cancel in that gap found nothing.

If Takyon itself crashed, every child survived.

## The decision

**One Job Object per spawn on Windows** (`agents/job.rs`). The job is created with
`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` and no breakaway allowed. The child is
created `CREATE_SUSPENDED`, assigned to the job, and only then resumed, so it
cannot start a descendant outside the job before assignment. `Child` does not
expose the thread handle, so the thread is found with Toolhelp. If assignment or
resume fails, the child is killed and the Turn fails as `spawnFailed`. Every
descendant inherits the job. Cancel calls `TerminateJobObject`. When Takyon dies,
Windows closes the job handle and kills every member without any of our code
running. Probes (`probe::run_command`) use the same path, so a probe that times
out takes its whole tree with it.

**A visibility gate on `Turns`.** `window::show` opens it. `window::hide` closes
it through `Turns::set_visible(false)`, which also cancels every Turn, and calls
`Searches::cancel_all`. All of this happens in Rust before `EVENT_HIDE` is
emitted. `window::hide` is the one function that all three dismiss routes
(hotkey, Escape, focus loss) go through, so dismissal no longer depends on the
webview running a cleanup.

- `start` refuses a Turn while the gate is closed, and the refusal emits nothing.
- `start` registers the Turn before its thread spawns anything, so `cancel` always
  finds it. A cancel that arrives before the job exists is kept as a flag, and
  the job is killed the moment it is attached.
- After attaching the job, `run` checks the gate again. A hide that landed between
  `start`'s check and the spawn terminates the Turn at that point.
- Termination never blocks. `hide` may run on the main thread, so reaping stays
  on each Turn's own thread.
- A cancelled Turn emits nothing, as in v0.8.

## Considered options

- **Kill the process tree by walking a Toolhelp snapshot.** This races with
  processes that are starting while the walk runs, and it does nothing about a
  crash.
- **Make the Agent the direct child by following the shim.** This does not cover
  what the Agent itself starts, and ADR-0032 already rejects following shims.

## Consequences

- When a Turn ends normally, the job is terminated as well, so anything the Agent
  left behind in the background dies too. That is the rule working as intended,
  not a side effect.
- Clicking a source link in `!s` opens the browser, which takes focus, which hides
  the Palette and kills the answer. A Turn only runs while someone can read it.
- **Unix has no crash cover.** There is a process group (`setsid`, then
  `kill(-pgid, SIGKILL)`) but nothing like `KILL_ON_JOB_CLOSE`, so on macOS a
  crashed Takyon still orphans its children. `docs/tbd/v0.11.md` §8 records this,
  and v0.12 owns it.
- Tests that start Turns have to open the gate first, because it defaults to
  closed. A test that forgets will see Turns refused and may read that as a
  regression.
- v0.15's live Agent processes are meant to run inside these jobs and to follow
  the same lifetime rule.
