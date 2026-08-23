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

## Verdict if triggered

Add **contract tests first** — they're a day or two and they eliminate fixture
drift, which is the failure mode most likely to bite and least likely to be
noticed. Reach for `tauri-driver` only when a bug escapes that both layers should
have caught, and scope it to a handful of smoke tests rather than a suite; a flaky
end-to-end suite that people learn to re-run until green is worse than no suite.
