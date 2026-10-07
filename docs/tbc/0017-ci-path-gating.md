---
status: watching
pairs-with: TBC-0015
---

# TBC-0017 — On a pull request, the slow live jobs run only when their paths change

## The bet

`ci.yml`'s two slowest jobs are `web-search`, which does live DuckDuckGo
searches, drives real browsers for Focus and fetches site icons, and `bench`, a
release build plus the performance harness. On a pull request they now run
their tests only when the PR touches something they check. A `changes` job lists
the PR's files (`scripts/ci-changes.sh`) and sets three flags:

| Flag | Set when the PR touches | Runs |
|---|---|---|
| `search` | `src-tauri/src/search/`, `tests/web_search.rs` | the live `!s` test, the site-icon test |
| `focus` | `src-tauri/src/focus/{browser,site,runtime,icons}.rs`, `focus/blocked.html`, `tests/focus.rs`, `tests/focus_browser.rs` | the live browser test, the site-icon test |
| `app` | app source on either side, Tauri config and capabilities, `packages/shared/src/`, `package.json`, `bun.lock`, the bench scripts | the bench |

A change to `ci.yml`, `ci-changes.sh`, `rust-toolchain.toml`, `Cargo.toml`,
`Cargo.lock` or `build.rs` sets all three. Pushes to `main`, the daily schedule
and manual runs ignore the flags and run everything.

Both jobs are required checks in the "Main Branch Protection" ruleset, and a
required check that is skipped reads as pending and blocks the merge. So the
jobs always start; with their flag off, every step but one `echo` is skipped
and the job passes in seconds.

The assumption is that these path lists are complete: nothing outside them can
break a live search, the browser driver or a budget. The live tests reach a long
way into the crate, so the lists are the files that carry the behaviour, not
every file compiled in.

## What would prove it wrong

A pull request merges with its gated job skipped, and `main`'s own run, or the
next daily run, fails that job on the merged commit. That is the cost of being
wrong: one red run on `main`, caught within a day, never a silent break. The
daily run is the backstop the gating leans on.

## What switching back costs

Delete the `changes` job, the `needs: changes` lines and the step-level `if`s.
Nothing else depends on them.

## Why not a merge queue

A merge queue would test the exact merge result and let a release trust it,
removing the second run on `main` altogether. GitHub offers merge queues only to
organization-owned repositories, and private ones only on Enterprise Cloud.
Takyon is a private repository on a personal account. The ruleset already
requires every check to pass on a branch that is up to date with `main`, which is
the part of a merge queue that matters at one pull request at a time.
