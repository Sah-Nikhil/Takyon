---
status: watching
pairs-with: ADR-0003
---

# TBC-0015 — The bench runs in CI, and a missed budget only warns

## The bet

`ci.yml`'s `bench` job builds the release binary and runs `bun run bench` on a
GitHub-hosted Windows runner, on every PR, every push to main and once a day. A
missed budget raises a warning annotation and writes the numbers to the job
summary. The job fails only when the harness itself fails: the hotkey never
registered, the Palette never reported a painted frame, or the process crashed.
`bench.ts` tells the two apart by exit code, 2 for a missed budget and 1 for
anything thrown.

The assumption is that a shared runner's timings are too noisy to gate on. It is
a 4-vCPU VM with no GPU and neighbours we cannot see, and every budget is tens of
milliseconds. A gate on those numbers would fail commits for reasons that have
nothing to do with the commit, and a gate that cries wolf is a gate people learn
to re-run until it goes green.

A second, smaller assumption: a hosted Windows runner has an interactive desktop
session, so `RegisterHotKey`, `keybd_event` and WebView2's first paint all work
there. Nobody has seen this job run yet. Its first run is the proof, and a
failure there reads as a harness failure, which is the correct outcome.

## How we'd know we were wrong

- **The numbers turn out to be stable.** After 14 daily runs, if the p95 of
  `show_to_first_pixel` and `query_to_first_entry` varies by less than 20% of its
  budget from run to run, the noise argument is gone. Gate on budgets scaled to
  the runner, and keep the local budgets as they are.
- **A regression lands under a warning.** If a commit's bench job warned, it
  merged, and a later local `bun run bench` confirms the regression was real,
  then warnings are not being read and something has to fail instead.
- **The harness cannot run on a hosted runner.** If the job fails on the hotkey
  or the first paint every time, the interactive-session assumption is wrong.
  The fallback is a self-hosted runner, below.

## Alternatives

| Option | Improvement if we switch | Added complexity | Switching cost |
|---|---|---|---|
| Gate on runner-scaled budgets | A regression fails the PR | A second set of budgets to keep honest | Change the exit-2 branch in the bench step to fail |
| Self-hosted runner on a dedicated Windows machine | Stable numbers, and budgets that match the product's | A machine to keep patched, and it must not run Raycast, PowerToys or an installed Takyon | New `runs-on` label, and a machine |
| Bench off CI, by hand before a release | No runner minutes | Relies on someone remembering, which is why `test:visual` was folded into `bun run test` | Delete the job |
