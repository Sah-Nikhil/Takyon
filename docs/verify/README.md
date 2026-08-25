# Manual verification

One script per phase. The third testing layer from `IMPLEMENTATION_PLAN.md` §11 —
the things a test suite structurally cannot see.

## Why these are not in `docs/plans/`

Different lifecycle. A build plan is consumed once: nobody reads
`v0.1-warm-shell.md` again after v0.1 ships. **A verification script never goes
stale.** It is re-run on every regression, and at v1.0 the whole folder is run as
one cumulative suite. Build plans are never cumulative.

`ls docs/verify/` answers "what do I run to prove this product works?", which is a
question with an audience — before a release, after a risky refactor, on a new
machine. Interleaved with build plans it is not answerable at all.

## What belongs here

Only what automation genuinely cannot reach, or cannot reach cheaply:

- global hotkeys, focus rules, dismissal, the tray, multi-monitor placement
- anything requiring elevation, or the UIAccess overlay over an elevated window
- what the shell actually returns for *this* machine's applications
- what extracted icons look like
- whether the **native window** is the right shape — the browser-based visual
  layer has no window to be clipped by, and three separate v0.2 bugs lived exactly
  there

If a step could be a Rust unit test or a Playwright assertion, it should be one.
A script long enough to skim is a script nobody runs honestly.

## Running one

Against a **release** build unless a step says otherwise — debug deliberately
behaves differently in several places. Record the date, the machine, and anything
surprising. **A step that "sort of worked" is a failure.**

Steps blocked by the environment are marked blocked, never quietly skipped, and
the reason is written up in [`../tbd/`](../tbd/) with the phase that owns closing
it.

## Files

| Script | Phase | Plan |
|---|---|---|
| [`v0.1.md`](./v0.1.md) | v0.1 warm shell | [`../plans/v0.1-warm-shell.md`](../plans/v0.1-warm-shell.md) |
| [`v0.2.md`](./v0.2.md) | v0.2 applications | [`../plans/v0.2-applications.md`](../plans/v0.2-applications.md) |
