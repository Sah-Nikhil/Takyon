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

## Built, 2026-08-29

The order above was followed, and the first two steps are done. Both were
cheaper than the estimate; one of them was blocked by something no amount of
planning would have found.

### What now exists

`apps/desktop/src-tauri/tests/`, three binaries, sixteen tests, ~2.5 seconds
inside `bun run test`:

- **`integration.rs`** — the real COM walk, real icon extraction through
  `IShellItemImageFactory`, the `icons.bin` round trip, Frecency across two
  `Pipeline`s over one directory, kind ordering with both Sources competing for
  one list, the Stability lock against the real clock, and the alias round trip
  applied to the real application list.
- **`recents_shell.rs`** — the Recents Source, which had never executed here at
  all (see below).
- **`ipc.rs`** — the contract test, via `tauri::test`.

A shared `common/mod.rs` holds two things worth naming. The application walk is
taken once per binary behind a `OnceLock`, because it costs ~450 ms and every
test wants the same one. And every directory a test writes to is a `TempDir`
that removes itself on drop, including when the test panics.

### The blocker: a test binary has no application manifest

`tauri::test` did not work by adding the dev-dependency. `mock_app()` alone
died before `main`:

```
process didn't exit successfully: probe_min.exe (exit code: 0xc0000139,
STATUS_ENTRYPOINT_NOT_FOUND)
```

The import table explains it. The binary imports `TaskDialogIndirect`, which
only **comctl32 v6** exports. A cargo test binary carries no application
manifest, so the loader binds comctl32 v5 out of `system32` and the process
never starts. `tauri-build` gives the real `takyon.exe` a manifest; it gives
test targets nothing.

Two lines in `build.rs` fix it, scoped to test targets:

```rust
println!(
    "cargo:rustc-link-arg-tests=/MANIFESTDEPENDENCY:type='win32' \
     name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
     processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
);
```

Worth recording because the symptom points nowhere near the cause: an exit code
before `main`, on a test that does nothing but build a mock app.

### Where `MockRuntime` stops

Measured rather than assumed. `scale_factor`, `set_size`, `show`, `set_focus`,
`emit` and `hwnd` all work. `inner_size` returns 0×0, so any assertion about
measured window size is meaningless there — `content_height` is pure and is the
right seam for that anyway. And `monitor_from_point` and `primary_monitor` are
`unimplemented!()`, so they panic: `window::show`, `window::toggle` and
`place_on_cursor_monitor` cannot run under the mock at all.

The remaining cost of driving the *real* handlers is that `window.rs` and its
neighbours take a concrete `&AppHandle`, which is `&AppHandle<Wry>`. Making them
generic over `R: Runtime` is about 40 signatures across six files, plus lifting
the `invoke_handler!` list out of `run()`. Mechanical, no behaviour change, half
a day — and deferred, because the handler bodies are two or three lines over
`Pipeline` and `Pipeline` is now covered directly. The contract test drives a
copy of the handler instead, which is enough for the failure this note called
most dangerous: the serialised shape drifting from `packages/shared/src/ipc.ts`.

### Two things the tests found on the way

**The Recents Source had never run.** Not under-tested — never executed. Its
only exercise was an `#[ignore]`d measurement, and `Start_TrackDocs = 0` means
the real folder is empty (`docs/tbd/v0.3.md` §1). `recents_shell.rs` now points
`%APPDATA%` at a temp tree and writes real shortcuts through `IShellLinkW`, so
the shell's own writer feeds the shell's own reader. That closes the logic half
of verification steps N1 and N4, and turns tbd §2 — a recently-opened folder can
never arrive — from something deduced by reading `lnk::read` into a test that
will go red the day it is fixed.

**`lnk::discover` depends on ambient COM.** Called on a thread with no
apartment it returns an empty `Vec`, silently: 154 `.lnk` files on disk, zero
read. Production is safe because `discover_all` opens a `ComScope` first, so
this is a testability hazard rather than a live bug — but the failure is an
empty list, which is indistinguishable from a machine with no Start Menu. It is
recorded in `docs/tbd/v0.3.md`. There is no test for it: the trap only springs
on a thread that has not already run something else, which no test in a shared
binary can guarantee.

### `tauri-driver`, re-examined and still not adopted

One argument from 2026-08-28 has genuinely weakened. `@wdio/tauri-service` now
reads Edge's version from the registry, downloads the matching `msedgedriver`
and caches it, so the manual version-pinning chore is mostly gone. It also
offers an embedded W3C server (`tauri-plugin-wdio-webdriver`) as an alternative
to `tauri-driver` itself.

The rest holds, and one part is sharper than before:

- The service matches against the **Edge browser** registry key, not the
  WebView2 Runtime key. Both read `151.0.4129.107` here today, and they are
  updated on separate cadences; when they diverge the driver is matched to the
  wrong thing. For reference, `msedgedriver` LATEST_STABLE is already
  `152.0.4191.53`, a major version ahead of the runtime installed here.
- The embedded route compiles a WebDriver server into the application. Nothing
  documents whether it is gated out of release builds, and this is a launcher
  that takes foreground over elevated windows.
- It still cannot live inside `bun run test`, because every run launches a real
  window and steals focus. That is the failure this note already recorded once.
- `scripts/verify-drive.ps1` already drives the release binary with real
  keystrokes and a foreground check. The marginal gain is DOM assertions.
- Two frictions specific to Takyon: the Palette starts `visible: false` and is
  summoned by a global hotkey WebDriver cannot send, and `activate` launches
  real applications.

## Verdict if triggered

**Contract tests came first and are done**, which was the recommendation and
took an afternoon rather than a day or two. What remains open, in order:
Playwright over CDP if a bug escapes both existing layers, and the
generic-over-`R` refactor if the command handlers ever grow bodies worth
testing — v0.6's Settings window is the likely trigger for both.

`tauri-driver` stays closed. Reach for it only when a bug escapes every layer
above, and scope it to a handful of smoke tests rather than a suite; a flaky
end-to-end suite that people learn to re-run until green is worse than no suite.
