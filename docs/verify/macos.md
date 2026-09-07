# macOS — manual verification

The v0.12 port, checked on hardware. **Nothing in this file has ever been run**,
and neither has the code it checks: the whole port was written on Windows and
gated by `bun run check:macos`, which type-checks and lints and never links,
bundles or executes a line.

This file grows as rows land. Sections are written when the row they cover is
built, not batched at the end, so an unwritten section means an unbuilt row
rather than an untested one. Sections A to D exist today; the row-by-row
sections after them arrive with their rows.

**Target: macOS 13 Ventura or later, Apple Silicon.** A machine on 11 or 12 is
out of scope and the bundle will refuse to launch on one.

## What automation already covers

Do not re-do these by hand:

- Everything platform-independent: matching, ranking, Frecency, aliases, bangs,
  the calculator, the query pipeline. `cargo test -p takyon` on the Mac.
- The bench helper's pure logic — process-tree walking, `ps` row parsing, the
  key table: `cargo test -p takyon-bench`.
- The React UI, through Playwright's `webkit` project. **The baselines do not
  exist yet** — they have to be generated on the Mac, and until they are, that
  suite has nothing to compare against. Windows' `chromium` baselines are not a
  substitute and must not be copied across.

Everything below needs a person, a window server and a login session.

## A. First run

The half hour that produces more information than every check that follows. Do
it before touching any other section.

### 1. It builds and starts

1. `git clone`, then `bun install`.
2. `bun run dev`.
3. The app launches. Expect a **Dock icon and an ordinary window** — section B is
   what removes them, and a dev build has no bundle to read `LSUIElement` from.
4. Nothing panics in the terminal within the first ten seconds.

### 2. The hotkey registers

1. With the app running, press **Option+Space**.

   `DEFAULT_ACCELERATOR` is still the literal `Alt+Space` with no macOS arm, and
   on this platform that means Option+Space, not Cmd+Space. The Cmd+Space
   takeover is unit 12 and is deliberately last — [`docs/tbd/v0.12.md`](../tbd/v0.12.md) §7.
2. The Palette appears.
3. **If it does not**: check the Palette's own banner and the terminal for a
   failed registration. Carbon's `RegisterEventHotKey` needs no Accessibility
   permission, so a failure here is a taken chord, not a permission prompt.
4. Escape dismisses it.

### 3. The `.app` walk found something

1. Open the Palette and type a few letters of an installed application.
2. Entries appear, and they are applications rather than nothing.
3. Icons are **expected to be missing** — row 3 is unbuilt and its stub refuses in
   words. A placeholder is correct here; a crash is not.

### 4. A release bundle links

`check:macos` never links, so this is the first time the linker sees the crate.

1. `bun run build`. **Never a bare `cargo build --release`** — that produces a
   binary with a completely dead frontend and no sign of why.
2. It produces a `.dmg` under
   `apps/desktop/src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/`.
3. Open the `.dmg`, drag the app to `/Applications`.
4. First launch is **right-click → Open**, not a double-click: the build is
   ad-hoc signed and un-notarised, and Gatekeeper's own message blames the
   download rather than the signature (TBC-0014).

## B. The agent app

`LSUIElement` **only takes effect on a clean launch.** Quit fully and relaunch;
a hot-reloaded dev build will not show the change, and a half-quit app will show
the old state and look like a bug.

1. Launch the **installed** build from `/Applications` (not `bun run dev`).
2. **No Dock icon appears.**
3. There is **no app menu** in the menu bar — no "Takyon" next to the Apple menu.
4. The hotkey still summons the Palette.
5. Cmd+Tab does **not** list Takyon.
6. Confirm the plist actually carries it:
   `defaults read /Applications/Takyon.app/Contents/Info LSUIElement` prints `1`.
7. `defaults read /Applications/Takyon.app/Contents/Info LSMinimumSystemVersion`
   prints `13.0`.
8. The bundle identifier is the slug, not the display name:
   `defaults read /Applications/Takyon.app/Contents/Info CFBundleIdentifier`
   prints `com.v3sper.takyon` (ADR-0020).

Steps 2 and 3 are the two halves of the same flag and both must hold. A Dock
icon with no app menu means the runtime `set_activation_policy` call ran and the
plist did not, which is worth knowing precisely because it still looks correct
at a glance.

## C. The benchmark

**Run this before any feature work.** It is the first number anyone has for
macOS, and ADR-0028 removed ADR-0003's working-set trim here, so the 150 MB
idle-RSS budget is a Windows-only claim until this section produces a figure.

### 1. It runs at all

1. Quit any running Takyon first. `single-instance` makes a second copy hand off
   and exit, and the harness then measures a process that was never alive. It
   detects this and says so; `--kill-running` ends it for you.
2. `bun run build`, then `bun run bench`.
3. Add `--alt-hotkey` if Option+Space is taken, which binds Ctrl+Alt+F9 instead
   and measures the identical code path.

### 2. `CGEventPost` reaches the hotkey

This is the one genuinely new mechanism in the harness and the thing most likely
to be wrong.

1. The run reports `n=30` shows rather than failing with "timed out waiting for
   the Palette to report a painted frame".
2. **If it times out with the Palette visibly appearing**, the events are landing
   but the log is not — a bench-log problem. **If the Palette never appears**,
   `CGEventPost` is not reaching a Carbon hotkey and the harness needs an
   `NSEvent` path instead.
3. Watch for a permission prompt. `CGEventPost` at `kCGHIDEventTap` may want
   Accessibility for the *injecting* process — that is the harness, not Takyon,
   and it does not change Takyon's own permission story.

### 3. Read `untrackedWebKitHelpers` before believing the RSS number

The single most important number in the first run.

1. Open the newest `bench/results/*.jsonl` run's memory output.
2. `untrackedWebKitHelpers` is **0**.
3. **If it is not 0, the macOS RSS figure is wrong and must not be recorded.**
   WKWebView's content, networking and GPU processes are XPC services reparented
   to launchd, so the tree walk that finds every WebView2 process on Windows
   finds none of them here. A non-zero count is that hazard confirmed, and the
   walk needs a responsible-pid lookup before any macOS memory number means
   anything — [`docs/tbd/v0.12.md`](../tbd/v0.12.md) §4.
4. Sanity-check it against Activity Monitor, which groups helpers by responsible
   process and will show the real total.

### 4. The numbers

1. Run **30 shows or more**. `stats` takes `s[floor(0.95 * n)]`, so at 20 runs
   the p95 *is* the maximum sample and one outlier fails a budget by itself.
2. The three latency budgets are unchanged and are not up for renegotiation:
   first pixel < 50 ms, first Entry < 30 ms, start to hotkey < 500 ms.
3. Write whatever comes out into `docs/tbc/0002` with the machine and the date,
   and amend ADR-0003 with the idle-RSS figure — that amendment is the point of
   the exercise, whichever way the number falls.

Windows, same day, for comparison: first pixel p95 21.9 ms, first Entry p95
24.3 ms, start to hotkey 263.9 ms, idle RSS 25.8 MB across 7 processes.

## D. The System Settings panes

28 ids, none of them verified, and **a wrong one fails quietly**: it opens System
Settings at its front page rather than erroring. Apple renamed most panes at
Ventura and there is no enumeration API, so this is a hand check with no
shortcut.

1. Open the Palette and type the name of each pane in `sources/system.rs`.
2. Enter opens System Settings **at that pane**, not at its front page.
3. Record each one pass or fail. A pane that lands on the front page has a wrong
   id; correct it in `sources/system.rs` and re-check.
4. Ventura, Sonoma and Sequoia moved panes independently, so note which macOS
   version this pass was run on — a table that passes on one may not on the next.

## E. The window (row 8)

**Do E.1 before writing any more window code.** It decides whether a hard piece
of work exists at all — [`docs/tbd/v0.12.md`](../tbd/v0.12.md) §9. The Palette is
a configured `NSWindow`, not an `NSPanel`, and the four checks below are exactly
the behaviours a panel would have bought.

### 1. The four panel behaviours, without a panel

1. Put an app into full screen (green button). Press the hotkey.
   **The Palette draws over it, and the Space does not switch.**
2. With the Palette open, switch Space with Ctrl+→.
   **The Palette is still there**, not stranded on the old desktop.
3. Open the Palette over a normal window, then click that window's title bar.
   The Palette dismisses (that is check 3 below) — now re-open it and instead
   watch whether it vanishes **on its own** the instant the app underneath
   regains focus. **It must not.**
4. Press the hotkey while a document app is frontmost and type nothing. The
   other app's title bar **stays active-looking** and its cursor keeps blinking
   — Takyon has not stolen activation.

Record each one. **All four pass** → no `NSPanel` is needed, amend ADR-0028 and
say so. **Any one fails** → that is the dynamic `NSPanel` subclass, and §9 says
what it costs.

### 2. Typing still works

The point of the non-activating configuration is that the Palette takes key
events without activating the app. Confirm it did not take the first half away:

1. Press the hotkey, type a few letters.
2. The characters appear in the Palette, not in the app underneath.
3. Escape dismisses.

If characters land in the other application, the window is not becoming key and
this is the failure mode the whole configuration risks.

### 3. Dismiss on click-away

Rebuilt on `NSEvent.addGlobalMonitorForEventsMatchingMask`, because Tauri's
`Focused(false)` is not reliable for this window.

1. Open the Palette. Click once anywhere in another application.
2. **The Palette hides**, and the click still reaches what was clicked — the
   monitor observes and cannot consume.
3. Do it with a right-click too; both buttons are watched.
4. Open the Palette and click **inside it**. It stays open. A global monitor only
   sees events delivered to other applications, so this should hold by
   construction; check it anyway, because if it fails the Palette is unusable.
5. Watch for an Accessibility prompt. There should be none — a global monitor for
   mouse-down needs no permission. **If macOS prompts here, that is new
   information** and changes the permission story in the plan's § Permissions.

### 4. Placement

`place_on_cursor_monitor` ports unchanged from Windows, including its two-pass
re-centring for mixed scale factors. That logic is not new; what is new is
whether Tauri's monitor APIs report macOS geometry the way it assumes.

1. On a single display: the Palette is horizontally centred and sits high on the
   screen, not dead centre.
2. With an external display attached, move the cursor to the second screen and
   press the hotkey. **The Palette appears on the screen holding the cursor.**
3. If the two displays have different scale factors (a Retina laptop plus a 1×
   external is the common case), check both directions. This is what the two-pass
   re-centring exists for and it has never run here.
4. The menu bar and the Dock do not overlap it on either screen.

## F. Launching (row 7)

All of this goes through `NSWorkspace` now. `/usr/bin/open` is gone, so a failure
here is a failure of the real design rather than of a stopgap.

### 1. Applications start

1. Open the Palette, type an application's name, press Enter.
2. **It starts**, and the Palette dismissed before it appeared rather than after.
3. Try one with a space in its path and one in `~/Applications` rather than
   `/Applications`.
4. Try an application that is already running: it comes forward rather than
   starting a second copy.

### 2. Ctrl+K actions

1. **Reveal in Finder** opens a Finder window with the file *selected*, not just
   the folder. This is `activateFileViewerSelectingURLs:`.
2. **Copy path** puts the path on the pasteboard.
3. **Run as administrator** is offered but refuses in words — macOS has no
   equivalent, and ADR-0007's no-elevation stance holds here. The message must be
   a sentence, not an error code.

### 3. URLs

1. A `steam://` Entry opens Steam (only if Steam is installed; the library on the
   dev machine has no game, so this may be unverifiable here as it is on Windows).
2. A System Settings Entry opens System Settings — that is section D.
3. `!s` results open in the default browser, not always Safari. Set a non-Safari
   default browser first, or this passes for the wrong reason.

### 4. What it reported

`open_application` waits up to two seconds for the completion handler.

1. Launch several applications in a row. **None of them feels slower than the
   Palette dismissing**, which is the only thing this wait could regress.
2. If a launch ever appears to hang for two seconds, that is the timeout being
   reached and it means the handler is not firing — worth knowing, though the
   application will still have started.

## Still to be written

One section per row, as the row lands:

- Row 3, icons — and `ICON_PX` at 128 on a 2× display.
- Row 4, Spotlight — including that **no** Full Disk Access prompt appears.
- Row 6, the clipboard — the poll, the nspasteboard.org markers, the Keychain
  item, and paste-back with and without Accessibility granted.
- Unit 12, the Cmd+Space takeover — the blocking onboarding step and its polling.
- Unit 13, uninstall — that "Remove all Takyon data" leaves no Keychain item and
  no data directory behind.
