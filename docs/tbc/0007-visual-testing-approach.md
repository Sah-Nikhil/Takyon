---
status: watching
pairs-with: ADR-0009
---

# TBC-0007 — Visual testing via a mocked browser build

## The bet

Visual regressions are caught by running the React UI in the ordinary Vite dev
server with the `api.ts` seam mocked, screenshotted by Playwright against fixture
data. The real binary is verified by a written manual script per phase.

The assumption: **most visual regressions are UI regressions**, so testing the UI
in isolation catches them at a fraction of the cost — deterministic, fast, no
Tauri, no hotkey, no focus rules to fight — while the genuinely OS-level
behaviours (global hotkey, focus-loss dismissal, tray, multi-monitor, UIAccess
overlay on elevated windows) are cheap to check by hand and expensive to automate.

The exposure is honest and worth stating: this layer **cannot** catch a bug where
the UI is correct and the Rust behind it is wrong. Fixture data always renders
beautifully.

## How we'd know we were wrong

- Bugs reach you that the mock layer structurally could not see — wrong ranking,
  wrong Entry contents, IPC contract drift between Rust and `api.ts`.
- Fixtures drift from what Rust actually returns, so tests pass against a reality
  that no longer exists. This is the classic failure of mock-based testing and it
  is silent.
- The manual script becomes long enough that it stops being run honestly.

## Alternatives

| Option | Improvement if we switch | Added complexity | Switching cost |
|---|---|---|---|
| **`tauri-driver` (WebDriver against the real app)** | Tests the actual binary: real Rust, real IPC, real window. Catches everything the mock layer cannot | Medium-high — Windows-only, awkward in CI, and screenshot diffs are unreliable because GPU and font rendering vary between machines | **3–5 days** to stand up, plus ongoing flakiness maintenance |
| **Contract tests on the IPC boundary** | Kills the fixture-drift failure specifically, which is the most dangerous one here | Low — assert that Rust's serialised output matches the TypeScript types `api.ts` declares | **1–2 days.** High value for the cost; likely worth doing regardless |
| **Screenshot the real window via Win32 capture** | Verifies the actual rendered window including WebView2 quirks | Medium — needs the debug no-steal-focus flag, and diffs are machine-dependent | **2–3 days.** Better as a manual aid than a CI gate |
| **Playwright over CDP against the real binary** | Same as `tauri-driver` — real Rust, real IPC — but keeps the framework already in use. WebView2 is Chromium, so `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` can open a debug port and `connectOverCDP` attaches to it | Medium — a second Playwright project, and the debug port must never be opened in a shipped build | **1–2 days**, and strictly cheaper than `tauri-driver` for the same reach |
| **Rust integration tests over `Pipeline`** | Reaches everything the mock layer cannot — ranking, Frecency, persistence — with no browser, no window and no flakiness | Low — ordinary `#[test]`s against a temp data directory | **Hours.** Not a visual layer at all, which is exactly why it was missed |

## Half-triggered at v0.3, and the answer was none of the above

The question arrived as coverage rather than as a bug: v0.3 added Frecency, the
Stability lock, aliases and Recents, and the reasonable challenge was *"did the
visual suite test any of that?"* It could not — none of it is UI, and the mock
layer's stated exposure is exactly this.

Two things came out of it, neither of them a switch:

- **`test:visual` joined `bun run test`.** The suite was passing and being
  skipped, because it had to be remembered separately. A layer that needs
  discipline to run is one that eventually does not run. That also puts it in
  `release`'s preflight, which the 0.2.0–0.2.2 builds never had.
- **The gap was closed in Rust, not in the browser.** "Does usage survive a
  restart?" is a `Pipeline` question, and a second `Pipeline` over the same
  directory answers it in fifty milliseconds. Reaching for an end-to-end harness
  there would have been a day of work to test something a unit test already
  reaches.

The lesson worth keeping: when this layer cannot see something, check whether the
thing is *UI* before assuming the answer is a bigger browser harness. Twice now it
has not been.

## Costed properly, 2026-08-28

The two candidates were examined against this repository rather than in general,
because the answer turns on things that are specific to it.

### What is actually uncovered

194 Rust tests cover pure logic well. What they do not touch is everything that
calls the OS: `lnk::com::read`, `appsfolder::com::discover`, `icons::extract`,
`recents::discover`, `path::discover`, and `ShellExecuteW` itself. Above that sit
ten `#[tauri::command]` handlers, the window lifecycle, the hotkey and the tray.

Only the last group genuinely needs a real window. **Everything below it is
reachable from `cargo test` today**, which is the finding that decides this.

### `tauri::test` exists and is unused

Tauri 2.11.5 ships a `test` feature with `mock_builder`, `mock_app`,
`get_ipc_response` and a `MockRuntime`. The ten command handlers can therefore be
driven with real state, real `Pipeline`, real SQLite — and no browser.

One caveat worth writing down: it goes in `[dev-dependencies]`, and must stay
there. Cargo excludes dev-dependencies from `cargo build`, so `tauri build` is
unaffected — but a `test` feature leaking into a shipped binary is exactly the
kind of thing nobody notices until it ships.

### Why `tauri-driver` is the wrong buy *here*

Not because it is bad, but because three things about this project blunt it:

- **`scripts/verify-drive.ps1` already exists.** It launches the release build,
  injects real keystrokes, refuses to type unless Takyon has foreground, captures
  the screen and prints the window size each step produced. The marginal gain of
  a WebDriver harness over that is DOM assertions instead of screenshots — real,
  but narrow.
- **msedgedriver must match the WebView2 runtime**, which is evergreen. This
  machine is on `151.0.4129.107` and Microsoft moves it roughly monthly. The
  harness therefore breaks on their schedule, not ours, and with no CI the way
  you find out is a red suite on an unrelated day.
- **It cannot live in `bun run test`.** Every run would launch a real window and
  steal focus, which is hostile in a dev loop — so it would sit outside the
  default suite. That is precisely the failure this note already recorded: a
  suite that must be remembered is one that gets skipped, and `test:visual` was
  skipped for exactly that reason until it was folded in.

### The order that follows

1. **Rust integration tests** — hours each, run in the default suite, reach every
   COM path. Best value by a distance.
2. **`tauri::test` command tests** — half a day to stand up, closes the IPC layer,
   still no browser.
3. **Playwright over CDP** — only if a bug escapes both, and preferred over
   `tauri-driver` because it reuses the framework already here.
4. **`tauri-driver`** — not now. Revisit if the product ever grows a second
   window whose interaction cannot be checked by hand.

## Verdict if triggered

Add **contract tests first** — they're a day or two and they eliminate fixture
drift, which is the failure mode most likely to bite and least likely to be
noticed. Reach for `tauri-driver` only when a bug escapes that both layers should
have caught, and scope it to a handful of smoke tests rather than a suite; a flaky
end-to-end suite that people learn to re-run until green is worse than no suite.
