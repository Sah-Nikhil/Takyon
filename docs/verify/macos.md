# macOS — manual verification

The v0.12 port, checked on hardware. **Nothing in this file has ever been run**,
and neither has the code it checks: the whole port was written on Windows and
gated by `bun run check:macos`, which type-checks and lints and never links,
bundles or executes a line.

This file grows as rows land. Sections are written when the row they cover is
built, not batched at the end, so an unwritten section means an unbuilt row
rather than an untested one. Every row has a section now; what is left unwritten
is listed at the end.

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

1. With the app running, press **Cmd+Space**.

   It will almost certainly do nothing: Spotlight holds that chord on a stock
   Mac and `RegisterEventHotKey` fails silently while it does. That is expected
   here and section L is where it gets taken. To carry on now, open Settings ->
   Keyboard and pick **Control+Space**.
2. The Palette appears on whichever chord registered.
3. **If it does not**: check the Palette's own banner and the terminal for a
   failed registration. Carbon's `RegisterEventHotKey` needs no Accessibility
   permission, so a failure here is a taken chord, not a permission prompt.
4. Escape dismisses it.

### 3. The `.app` walk found something

1. Open the Palette and type a few letters of an installed application.
2. Entries appear, and they are applications rather than nothing.
3. Icons should be present — row 3 is built. A placeholder or two while the blob
   fills is fine; none at all after a few queries is section G's problem.

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

## G. Icons (row 3)

`ICON_PX` went 64 → 128 on **both** platforms, so this section has a Windows
half too.

1. Open the Palette and type a few letters. **Rows have real icons**, not
   placeholders, and they are crisp rather than soft on a Retina display.
2. Icons appear as rows draw rather than all at once — extraction is lazy and
   that is correct.
3. Quit and relaunch. Icons appear immediately: the blob was written and mapped.
4. `ls -l ~/Library/Application\ Support/com.v3sper.takyon/icons.bin` is
   hundreds of kilobytes, not 12 bytes. Twelve bytes means a header and no
   icons, which is what v0.2's C5 failed on for two releases.
5. **On Windows, once**: the first launch after this change re-extracts every
   icon, because `FORMAT_VERSION` went to 2 and the old blob is discarded. That
   is one slow fill, not a bug. The second launch is fast again.

## H. Files (row 4)

Spotlight through `MDQuery`, and the thing to check first is that it asks at
all.

1. Type `!e` and a filename you know exists. **Entries appear.**
2. **No Full Disk Access prompt appears.** This is ADR-0027's central claim —
   metadata queries return paths without one. A prompt here would be a finding.
3. Search for something with a space, and something with an apostrophe.
4. Search for a string containing `*`. It matches files containing a literal
   asterisk, **not** everything — the wildcard is escaped.
5. Compare against Spotlight itself (Cmd+Space, once you have it) for the same
   needle. Takyon's list should be a plausible subset, not empty and not wild.
6. **The roots scope it.** Settings -> Files, remove a root, search for
   something that only lives under it: it stops being found, with no rebuild and
   no wait — scope is a query predicate on macOS, not a walk boundary.
7. Add an exclusion (`node_modules`) and confirm files under a directory of that
   name stop appearing anywhere in the tree, not just at its top level.
8. The Files page shows **no entry count** — it reads "Searching through
   Spotlight". Spotlight reports no count and ADR-0027 accepts that rather than
   faking one.

## I. Web search (row 5)

1. `!s` a question. Results arrive with numbered sources.
2. The answer streams rather than appearing whole.
3. Favicons appear beside sources — same `URLSession` path, different cap.
4. Turn Wi-Fi off and `!s` again. It fails **in a sentence**, within about six
   seconds, rather than hanging: that is `NSURLRequest`'s timeout doing its job.
5. If the machine has a proxy configured in System Settings, `!s` still works —
   that is half the reason ADR-0029 chose `URLSession` over a Rust client.

## J. The clipboard (row 6)

Four mechanisms, and the markers matter most because they are the safety story.

### 1. Capture

1. Copy some text in another application. Open the Palette, go to clipboard
   history. **The clip is there.**
2. Copy twice within half a second. Expect **one** row, not two — the accepted
   cost of a 500 ms poll (ADR-0030).
3. Copy from several applications and confirm each row is attributed to the
   right one.

### 2. The markers, which is the part that must not be wrong

1. Install or open a password manager that implements nspasteboard.org — 1Password
   and Bitwarden both do.
2. Copy a password from it.
3. **It does not appear in clipboard history.** If it does, stop and treat it as
   a defect: this is ADR-0006's primary safety mechanism, not a nicety.
4. Copy ordinary text from the same application afterwards. That *should* appear
   — the marker is per-copy, not per-application.

### 3. Paste-back and the permission

1. With Accessibility **not** granted, choose a clip and press Enter.
   **The clip is on the pasteboard** and the message says the permission is
   needed and where to grant it. Cmd+V by hand works.
2. Grant Accessibility in System Settings → Privacy & Security.
3. Try again. **The clip pastes itself.**
4. Revoke the permission and try once more: it must return to step 1's behaviour
   rather than silently doing nothing. `AXIsProcessTrusted` asks without
   prompting, so this is checkable as many times as you like.

### 4. The Keychain

1. Open Keychain Access and search for `com.v3sper.takyon`. **One generic
   password item**, account `clip.key`.
2. Quit Takyon, relaunch, open clipboard history. **Old clips still decrypt** —
   the key came back from the Keychain rather than being regenerated. A history
   that looks empty here means a new key, which is the failure worth catching.

## K. The small rows (10, 11, 12)

1. **Default browser**: set a non-Safari default in System Settings, then press
   Enter on an `!s` result. It opens in *that* browser, and a query opens as a
   search in the browser's own engine rather than as a URL.
2. **Menu bar**: the item is present, and its glyph is legible in **both**
   appearances — switch System Settings → Appearance between Light and Dark and
   look again. Wrong polarity means a glyph that vanishes into the bar.
3. **Versions**: install two applications with the same name in different
   places. Both Entries show a version to tell them apart.
4. **Steam**: if Steam is installed, its games appear and launch through
   `steam://`. Unverifiable on a machine with no game — the same gap Windows has.

## L. Taking Cmd+Space (unit 12)

The mechanism, not the first-run screen — that surface is still unbuilt
([`docs/tbd/v0.12.md`](../tbd/v0.12.md) §15). Everything below is reachable from
Settings -> Keyboard today.

1. On a stock Mac, Spotlight holds Cmd+Space. Open Settings -> Keyboard.
2. "Open Takyon with" shows **Command + Space**, and it is **not** registered —
   the row says so and a block appears offering to open Keyboard Shortcuts.
3. Press **Open Keyboard Shortcuts**. System Settings opens **at Keyboard
   Shortcuts**, not at its front page. A front page means the pane id is wrong,
   which is section D's failure mode.
4. Uncheck "Show Spotlight search".
5. **Without touching Takyon**, watch the Settings window. Within about a second
   the block disappears and the row reads as registered — the poll took the chord
   on its own. There is no button to press, and that is the point.
6. Press Cmd+Space. The Palette opens.
7. Re-check Spotlight's box, quit and relaunch Takyon: the chord fails to
   register again and the block comes back.

Also confirm the two platform differences on that page:

8. The chord list offers macOS spellings — Command, Control, Option — not
   Alt+Space.
9. **There is no "Open Takyon with the Windows key" row.** `superkey.rs` is
   dropped on macOS (ADR-0025), so a switch there would offer a hook that can
   never install.
10. Pick a non-default chord, then press **Reset**. It returns to Command+Space,
    not Alt+Space — the page reads Rust's own list rather than a second copy of
    the default.

## M. Removing all data (unit 13)

**Destructive and irreversible. Do this last**, or every section above has to be
set up again.

1. Use Takyon enough to have clipboard history and some launch history.
2. Settings -> Advanced. A **Data** group is present with "Remove all Takyon
   data". On Windows that group does not appear at all — the NSIS uninstaller
   already does this.
3. Press **Remove**. A confirmation names what will go and says it cannot be
   undone. Cancel once, and confirm nothing was deleted.
4. Press **Remove** again and confirm.
5. The line under the button reports success and names the directory.
6. Check by hand: the data directory under `~/Library/Application Support/` named
   `com.v3sper.takyon` is gone, and Keychain Access finds no item for
   `com.v3sper.takyon`.
7. Clipboard history in the Palette is empty rather than erroring.
8. Quit and relaunch. Takyon starts cleanly on an empty directory and a fresh
   Keychain item rather than failing because something it expected is missing.
9. **If the report names a problem instead**, read it: a surviving Keychain entry
   is exactly the case this reports rather than rounding to "done", and it is the
   one worth chasing.

## Still to be written

Every row now has a section above. What is left is work that is not yet written
at all:

- The **blocking first-run screen** for the Cmd+Space takeover. §L covers the
  mechanism that exists; the screen that refuses to finish onboarding until the
  chord registers is unbuilt ([`docs/tbd/v0.12.md`](../tbd/v0.12.md) §15).
- The `webkit` Playwright baselines, which have to be generated on this machine.
