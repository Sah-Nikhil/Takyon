# macOS — the build plan

**Goal: 1:1 with Windows.** Three things are permanently excepted and every one
of them is excepted because the platform has no such problem, not because the work
was skipped. Everything else is parity.

**Status: the crate compiles for `aarch64-apple-darwin` and four rows work.**
`bun run check:macos` is clean with `-D warnings` across the library, its unit
tests and every integration test. What works today: the `.app` walk, launch and
reveal, System Settings panes, and clipboard read/write. What does not: icons,
files, clipboard history, `!s` retrieval, paste-back. Each is a stub that refuses
in words rather than a gap that fails silently.

**Nothing has run on a Mac.** The compiler has checked every line and nothing else
has. The first thing to run on hardware is `bun run bench`, not a feature.

## The decisions, all of them

Settled in a design review; each links to where the reasoning lives.

| | decision | where |
|---|---|---|
| Cocoa bindings | `objc2` family as direct macOS-only dependencies. Zero new crates — they are already in `Cargo.lock` via Tauri | [ADR-0026](../adr/0026-objc2-is-the-macos-binding.md) |
| File index | Spotlight via `MDQuery` + `kMDQuerySynchronous`. No walk, no watcher | [ADR-0027](../adr/0027-spotlight-is-the-file-index-on-macos.md) |
| App shape | Agent app (`LSUIElement`) + non-activating `NSPanel` over all Spaces | [ADR-0028](../adr/0028-the-palette-is-an-agent-app-and-a-panel.md) |
| HTTP | `URLSession`, amending ADR-0019 | [ADR-0029](../adr/0029-urlsession-is-the-macos-http-client.md) |
| Clipboard | `NSPasteboard`, 500 ms poll, nspasteboard.org markers, Keychain key | [ADR-0030](../adr/0030-the-macos-clipboard.md) |
| Signing | Ad-hoc, un-notarised, right-click → Open | [TBC-0014](../tbc/0014-macos-distribution-and-signing.md) |
| Agent CLIs | Login-shell PATH hydration, t3code's method, both platforms | [path-hydration.md](./path-hydration.md) |
| OS index on Windows | A settings toggle, after this port | [os-index.md](./os-index.md) |
| Image + file clips | Its own phase, after this port | [clipboard-kinds.md](./clipboard-kinds.md) |

**Platform: macOS 13.0 Ventura, Apple Silicon only.** Declared in a new
`tauri.macos.conf.json`, not left to Tauri's 10.13 default. Ventura rather than
Big Sur — which is the oldest an M1 can run — because the System Settings pane ids
are the Ventura `com.apple.X-Settings.extension` form and the pre-Ventura table
could not be tested. Every Apple Silicon Mac can run 13 or later for free, and 11
and 12 are out of Apple's security support. Universal2 is a `--target
universal-apple-darwin` flag in `release.yml` the day an Intel Mac needs one; that
is a written deferral, not an oversight.

## The three permanent exceptions

- **UIAccess.** No macOS analogue and no macOS problem: nothing runs with a
  separate elevated input desktop to lose a foreground race against. `uiaccess.rs`
  and `com.rs` are dropped.
- **The Windows-key tap.** `superkey.rs` is dropped. `Hotkey::second_binding()`
  returning `None` is that decision stated in code (ADR-0025).
- **Nothing else.** Steam, paste-back, launched-image identity, the blocklist and
  clipboard history are all parity — several of them were on this list at the
  start of the review and came off it once the mechanism was found.

## What was already portable

- **The whole frontend.** React, the Palette, both Settings windows, every theme.
- **`packages/shared`** — the IPC contract, by construction.
- **Ranking, matching, Frecency, aliases, bangs, the calculator, the query
  pipeline.** Pure logic, and the bulk of the Rust test suite.
- **The hotkey**, via `tauri-plugin-global-shortcut`, which uses Carbon
  `RegisterEventHotKey` — the one public macOS global-hotkey API that needs **no
  Accessibility permission**. The hotkey works before any prompt.
- **Autostart**, via `tauri-plugin-autostart`, which writes a LaunchAgent.
- **The icon set.** `brand/` already generates `icon.icns`.
- **`steam.rs`'s parser.** Only `steam_path()` is Windows-specific.

## The rows

| # | Subsystem | mechanism | state |
|---|---|---|---|
| 1 | `identity.rs` | `~/Library/Application Support/com.v3sper.takyon` | **done** |
| 2 | `sources/apps` | `.app` bundle walk; `NSBundle` for display name; exec-bit `PATH` scan | walk done, rest open |
| 3 | `icons.rs` | `NSWorkspace.icon(forFile:)` → PNG at 128 px into the same `icons.bin` | open |
| 4 | `index/` | `MDQuery` + `kMDQuerySynchronous` behind `FileIndex` | open |
| 5 | `search/fetch.rs` | `NSURLSession` + `block2` | stub |
| 6 | `clips/` | `NSPasteboard`, 500 ms poll, markers, Keychain, `CGEventPost` | read/write done |
| 7 | `launch.rs` | `NSWorkspace.openApplication` + `activateFileViewerSelecting` | **rewrite** |
| 8 | `window.rs` | agent app, non-activating panel, global mouse monitor | open |
| 9 | `sources/system.rs` | 28 `x-apple.systempreferences:` panes | **done**, ids unverified |
| 10 | `search/browser.rs` | `NSWorkspace.open` for the URL; default browser via `LSCopyDefaultApplicationURLForURL` | partial |
| 11 | `tray.rs` | `NSStatusItem` — `tray-icon` already produces one | near-free |
| 12 | `version.rs` | `NSBundle.infoDictionary` → `CFBundleShortVersionString` | open |

**Row 7 is a rewrite, not a gap.** The `/usr/bin/open` implementation shipped as a
stopgap before ADR-0026 and is superseded. `NSWorkspace.openApplication(at:
configuration:completionHandler:)` hands back the launched `NSRunningApplication`
in its completion handler — no diffing, no notification observing, no race — so
**Frecency keeps launched-image identity, and gets a better one than Windows**:
`bundleIdentifier` survives an app being moved or updated where a path does not.

**Row 8 is bigger than "mostly Tauri already" implied.** `place_on_cursor_monitor`
ports unchanged, including its two-pass re-centring for mixed scale factors. What
is new is the panel configuration and, because a non-activating panel never
becomes key, a dismiss-on-click-away rebuilt on
`NSEvent.addGlobalMonitorForEvents`. ADR-0028 has the exact flags.

**Row 3 raises `ICON_PX` from 64 to 128 on both platforms.** Every Apple Silicon
Mac is 2× and a 64 px bitmap in a 32 pt slot is visibly soft; Windows has 200 %
displays too. One number, both platforms, no per-platform icon cache to reason
about. `icons.bin` carries a `FORMAT_VERSION` and a magic, so bumping it
invalidates old caches cleanly — which is what that field is for.

**Row 6's fourth part** is `clips/key.rs`, the DPAPI wrap, now `#[cfg(windows)]`
and needing a Keychain item through `security-framework` (ADR-0030).

## Onboarding: taking Cmd+Space

**Decision: force the takeover, mimicking Raycast, with alternatives in
Settings.**

Cmd+Space cannot be taken programmatically by anyone. It is a system-reserved
shortcut, and `RegisterEventHotKey` does not fire while Spotlight holds it.
Raycast's answer is to walk the user to System Settings → Keyboard → Keyboard
Shortcuts → Spotlight to uncheck "Show Spotlight search", and there is no version
of this where the user does not visit System Settings once.

The step, precisely:

1. Explain what is about to happen and why, in one screen.
2. Deep-link straight to the Keyboard Shortcuts pane.
3. **Poll the registration** — attempt `Cmd+Space` on a timer while the step is on
   screen. The moment the user unchecks the box, registration succeeds and the
   step advances by itself. No "I've done it" button to lie to, no re-check, no
   dead end where the user did it right and Takyon still says no.
4. Onboarding does not complete until it registers. This is a deliberate block and
   its cost is understood: an installed launcher that cannot be opened until a
   System Settings trip is finished is one some people abandon at first run. It is
   accepted because the alternative — a launcher on a chord nobody uses — is the
   product being quietly worse forever.

Settings → Keyboard keeps the full chord list, so anyone who wants Ctrl+Space can
have it.

**A parity gap to record rather than fix.** Carbon hotkeys are known to fail
silently inside self-drawn terminals such as Zed and VS Code, so the hotkey may not
fire while focus is in one. Windows' `RegisterHotKey` has no such hole. Upgrading
to an `NSEvent` global monitor would close it and would cost the Accessibility
permission on the hotkey path — not worth it, and worth writing down.

## Permissions

Two, and only one is required.

- **Accessibility**, for `CGEventPost` — paste-back only. Everything else works
  without it.
  `AXIsProcessTrustedWithOptions` prompts **once per binary**; after a denial
  macOS never shows that prompt again and the call simply returns false. So the
  agreed behaviour — "keep asking until granted" — is *not* a prompt: one prompt
  during onboarding, then a **banner in the Palette** on every paste-back
  attempt, with a button that opens
  `x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility`.
- **Nothing else.** Not Full Disk Access: Spotlight metadata queries return paths
  without one, which is what keeps ADR-0007's no-prompt posture intact on macOS
  (ADR-0027).

## Autostart and reachability

- **Registered at first run, default on.** A moved binary is repointed by the
  existing `self_heal_autostart`; a login item the user deliberately removed is
  **never** resurrected. That is ADR-0015's rule — OS state lives in the OS —
  ported unchanged, and it is what stops Takyon fighting the user through System
  Settings → General → Login Items.
- **The menu bar item is always created.** No "hide the icon" setting on macOS,
  unlike Windows' tray toggle: with no Dock icon and no app menu there is nothing
  else to click.
- **Relaunching Takyon shows the Palette.** `tauri-plugin-single-instance` is
  already a dependency; its handler shows the window rather than starting a second
  copy. This is the escape hatch nothing in the environment can take away — the
  user can auto-hide the menu bar, a notched MacBook drops overflow items, and
  Bartender or Ice hide them deliberately, so "always visible" is something we
  intend rather than control.

## Uninstall

Dragging the app to the Trash runs nothing. The login item, the data directory,
the Keychain item and the blob directory all survive.

- **Settings → a "Remove all Takyon data" button** that does the cleanup while the
  app can still run. `identity::data_dir()` already knows every path.
- **A note in the DMG.** Not a substitute for the button — a reminder that one
  exists.

Accepting the leftovers silently is what most Mac apps do and is wrong for *this*
app's data: a Keychain entry and an encrypted clipboard database on a machine
whose owner believes they removed the app.

## Build order

The rows are independent — each has a stub that compiles and refuses — so from
step 3 onward this is a recommended order rather than a dependency graph. The
first three are not optional and are in this order for a reason.

**Start here.**

0. **Get it onto the Mac and run it.** `git clone`, `bun install`,
   `bun run dev`. Nothing in this repository has ever executed on macOS, so this
   is the first real information anyone has: does it launch, does the hotkey
   register, does the Palette paint, does the `.app` walk find applications.
   Expect a Dock icon and an ordinary window — `LSUIElement` and the panel are
   step 2. Half an hour, and it can only be done once.

   `bun run build` for a release bundle is worth trying in the same sitting, and
   is a different risk: it links, which `check:macos` never does.

1. **The benchmark harness, ported to Rust.** `bun run bench` is four PowerShell
   scripts and cannot run here. Until it does, nothing about performance on macOS
   is knowable, and ADR-0028 removed ADR-0003's working-set trim, so the **150 MB
   idle-RSS figure is unverified and must not be quoted as if it held.** Whatever
   the harness reports is written into ADR-0003 as an amendment. The three
   latency budgets port unchanged and are not up for renegotiation. See
   *Testing* for why Rust rather than shell or Swift.

2. **`tauri.macos.conf.json`** — `minimumSystemVersion: "13.0"`, bundle
   identifier and product name from ADR-0020's two literals, `LSUIElement`.
**Then the rows.**

3. **Row 8, the window.** Everything else is judged by eye through it, and a
   Palette that cannot appear over a full-screen app is not testable.
4. **Row 7, launch through `NSWorkspace`.** Replaces the stopgap and restores
   Frecency identity.
5. **Row 3, icons.** The list looks broken without them, which makes every later
   row harder to evaluate.
6. **Row 2's remainder** — `NSBundle` display names, exec-bit `PATH` scan (which
   consumes `path-hydration.md`'s work).
7. **Row 4, Spotlight.**
8. **Row 5, `URLSession`** — unblocks `!s` and favicons together.
9. **Row 6, the clipboard** — poll, markers, Keychain, paste chord.
10. **Rows 10, 11, 12** — small, and pleasant to finish on.

The Cmd+Space onboarding is not in this list on purpose: it is the last thing
built, because until the Palette is worth summoning there is nothing to take
Spotlight's shortcut *for*.

`steam_path()` is a one-function change to
`~/Library/Application Support/Steam` and can be done at any point.

## How to work on it

```
bun run check:macos      # cross-compile check + clippy, from Windows, via zig
```

Needs a zig build unpacked into `%LOCALAPPDATA%\zig\`, on `PATH`, or pointed at by
`TAKYON_ZIG`; the script adds the Rust target itself. `scripts/zig/` holds the two
wrappers it drives — `zig cc` pinned to `aarch64-macos`, with cc-rs's own
`--target=arm64-apple-macosx` filtered out because zig's clang frontend rejects
`arm64` as an architecture name.

It type-checks and lints, and fails on a duplicated `objc2` (ADR-0026). It does
not link, bundle, or run a test.

## Testing

**Rust unit and integration tests** — written per row as the row lands, not
batched at the end. A bundle walk against a temp `/Applications`-shaped tree, an
`MDQuery` against files the test writes itself, an `NSWorkspace.openApplication`
round trip. Machine-dependent, so assert shape and ordering, never which
applications are installed — the same rule the Windows integration tests follow.
CI's `macos` job grows from `clippy` to `clippy + cargo test -p takyon` the first
time one exists.

**Visual regression** — a **`webkit`** Playwright project with its own baselines,
run **locally on the development Mac only, never in CI**. Two reasons for each
half of that:

- `webkit`, not `chromium`, because Takyon ships WebView2 on Windows and
  **WKWebView on macOS**. Chromium-on-macOS baselines would assert that an engine
  which never ships renders our React the same as last week — and that React tree
  is shared, so the Windows run already covers it. The macOS-specific risk is
  WebKit, and there is a real one here: `styles.css` derives every theme token
  through `color-mix(in oklab, …)` (ADR-0023), which is exactly the class of CSS
  where the engines have diverged. Caveat: Playwright's WebKit is Playwright's own
  build, not WKWebView — closer engine family, not the shipping engine.
- Local, because `macos-latest` runners bill at ten times the Linux rate.

Mechanically this is: one added Playwright project, and `{platform}` added to
`snapshotPathTemplate`, which is currently
`{testDir}/__screenshots__/{arg}{ext}` and would otherwise collide all platforms
onto the same 47 PNGs. Baselines must be generated on the Mac, and after this a UI
change needs both sets regenerated.

**The benchmark harness is ported to Rust**, not shell and not Swift. Shell is
wrong for the half that matters: `bench-input` measures hotkey → first pixel
against a 50 ms budget, and driving that through `osascript … keystroke` puts an
AppleScript round trip of tens of milliseconds *inside* the measurement. Swift is
right on precision and adds a fourth language for one helper. A small Rust binary
in the workspace, driven by `scripts/bench.ts` exactly as the PowerShell helpers
are today, gets `CGEventPost` from `objc2-core-graphics` (already in the lock) and
timing from `Instant` with nothing in between. Shell is fine for the memory
sampling, where there is no precision requirement. This is also where the Windows
PowerShell helpers should eventually live.

**Manual verification** — `docs/verify/macos.md`, written as the phase is built.
The permission prompts, the Spaces and full-screen behaviour, the Cmd+Space
takeover and the menu bar item genuinely cannot be automated, and neither
Playwright layer touches the real webview on either platform.

## Traps

- **`AXIsProcessTrustedWithOptions` prompts once per binary.** After a denial it
  returns false and never prompts again. The banner is not a nicety.
- **A non-activating panel never becomes key**, so Tauri's `Focused(false)` never
  fires and `should_hide_on_focus_loss` has nothing to hang on. This is the one
  place where the Windows behaviour has to be rebuilt rather than ported.
- **`LSUIElement` only takes effect on a clean launch.** Quit and relaunch; a
  hot-reloaded dev build will not show the change.
- **`open`, `pbcopy` and `pbpaste` are stopgaps**, not the design. ADR-0026
  supersedes them. Do not add more.
- **The System Settings pane ids are unverified.** Apple renamed most panes at
  Ventura and there is no enumeration API. A wrong id opens System Settings at its
  front page rather than erroring — the quiet kind of wrong. Verifying all 28 by
  hand is a `docs/verify/macos.md` task.
- **`cargo check --target aarch64-apple-darwin` fails before it reaches our
  code** without a cross C compiler: `libsqlite3-sys` and `objc2-exception-helper`
  both build native code. The error names `cc` and reads like a missing toolchain,
  because it is one. `bun run check:macos` supplies zig.

## CI

`.github/workflows/ci.yml` has a `macos` job — clippy today, clippy plus
`cargo test -p takyon` once the first macOS integration test exists. Never
Playwright.

`.github/workflows/release.yml` carries `build-macos`, skipped unless the
repository variable `MACOS_BUILD` is `true` and `continue-on-error` so it can
never gate the Windows release. Setting that variable publishes a `.dmg` with no
further edit to the workflow.

## Which phase owns this

Unassigned, and it is comfortably more than one phase. It competes with the two
things ROADMAP already calls v1.0 blockers — the code-signing certificate and the
updater — and it remains the largest single piece of work left in the project.
