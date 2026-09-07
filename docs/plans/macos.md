# macOS — what a port actually needs

**Status: it compiles, and four rows of it work.** `cargo clippy --target
aarch64-apple-darwin -p takyon --all-targets -- -D warnings` is clean — the
library, its unit tests and every integration test.

What works: it finds applications through the `.app` walk, launches them and
reveals them in Finder, opens System Settings panes and web addresses, and reads
and writes the clipboard. What does not: no icons, no file index, no clipboard
history, no `!s` retrieval, no paste-back. Each of those is a stub that refuses
in words rather than a gap that fails silently.

**None of it has run on a Mac.** The compiler has checked every line and nothing
else has.

**The 7,450-line figure was misleading and this is where it gets corrected.**
Twenty files do name the `windows` crate, but only `search/fetch.rs` had a
module-level `use windows::` that no `cfg` guarded. Everywhere else the Win32
imports already sat inside `#[cfg(windows)]` functions with a non-Windows twin
beside them — the pattern `superkey.rs` and `launch.rs` had been following since
v0.1. Closing the gap took gating one transport, four dead constants, four
statics, two helpers and three tests. The 7,450 lines are still the *porting*
work; they were never the *compiling* work.

**It can be checked from Windows.** `bun run check:macos` cross-compiles through
zig, which ships the macOS libc and Objective-C headers, so no Apple SDK is
involved. `libsqlite3-sys` and `objc2-exception-helper` both build native code
and are what a bare `cargo check` fails on. Linking a real `.app` still needs a
Mac; CI's `macos` job on `macos-latest` is the second gate.

This document exists because the question "is there a macOS setup?" deserves a
number rather than a shrug. The number is **7,410 lines across 20 files** that
name the `windows` crate. Not all of it is logic that has to be rewritten —
plenty is COM ceremony that a Cocoa equivalent replaces in a tenth of the space
— but every one of those files needs a decision.

The workspace layout, `packages/shared`, and the trait boundaries were all put
in ahead of need so that this port is a matter of writing implementations rather
than restructuring. That part held up. What did not get built is the
implementations.

## What is already portable

- **The whole frontend.** React, the Palette, both Settings windows, every
  theme. It runs in a plain browser under the visual suite already, which is the
  strongest evidence available that it does not depend on Windows.
- **`packages/shared`** — the IPC contract, by construction.
- **Ranking, matching, Frecency, aliases, bangs, the calculator, the query
  pipeline.** Pure logic, no OS calls, and the bulk of the Rust test suite.
- **The Agent drivers.** `agents/probe.rs` already carries `cfg(windows)` arms
  and looks for `claude`, `codex` and `opencode` on `PATH`, which is how they
  are found on macOS too.
- **The hotkey**, via `tauri-plugin-global-shortcut`. The original v0.10 brief
  asked for Alt+Space on macOS, which is what the plugin already binds — there
  is no macOS analogue of `superkey.rs` to write, and none is wanted.
- **Autostart**, via `tauri-plugin-autostart`, which writes a LaunchAgent.
- **The icon set.** `brand/` already generates `icon.icns`.

## What has to be written, in the order it blocks things

| # | Subsystem | Windows today | macOS |
|---|---|---|---|
| 1 | ~~`identity.rs`~~ | `%LOCALAPPDATA%\v3sper\takyon` | **written**: `~/Library/Application Support/com.v3sper.takyon`, the slug rather than `<vendor>/<app>` |
| 2 | `sources/apps` (1,350) | Start Menu COM walk, `.lnk` parsing, AppsFolder | **part written**: `apps/bundles.rs` walks the three roots. Display name is the bundle stem, not `CFBundleDisplayName`; `PATH` executables need an exec-bit check `path.rs` cannot give them |
| 3 | `icons.rs` (785) | `IShellItemImageFactory` | `NSWorkspace.iconForFile`, same `icons.bin` on the other side |
| 4 | `index/` (1,110) | `ReadDirectoryChangesW`, Windows Search fallback | FSEvents; Spotlight (`NSMetadataQuery`) as the fallback |
| 5 | `search/fetch.rs` (377) | WinHTTP (ADR-0019) | **stubbed**: `send` refuses by name, TBC-0013 owns the real answer |
| 6 | `clips/` (1,100) | DPAPI key, clipboard format listener, `SendInput` paste | **part written**: `MacClipboard` reads and writes through `pbpaste`/`pbcopy`. The key wrap (Keychain), the watcher (`changeCount`) and the paste chord (`CGEvent`, Accessibility permission) all refuse in words |
| 7 | ~~`launch.rs`~~ (439) | `ShellExecuteW` | **written**: `/usr/bin/open`, which is Finder's own path — bundles, URL schemes and `-R` reveal alike. Returns no launched-image path, so v0.3's identity refinement has no macOS half |
| 8 | `window.rs` (1,040) | monitor placement, foreground checks | mostly Tauri already; the placement maths is the part to keep |
| 9 | ~~`sources/system.rs`~~ (419) | Windows settings pages | **written**: 28 `x-apple.systempreferences:` panes. The ids are unverified — Apple renamed most of them at Ventura and there is no enumeration API |
| 10 | `search/browser.rs` (163) | default browser from the registry | **works by way of row 7**: `open <url>` uses the default browser. `default_browser()` still returns `None`, so Enter on `!s` opens the provider's results page rather than the browser's own engine |
| 11 | `tray.rs` (297) | mostly Tauri, some Win32 | menu bar item |
| 12 | `version.rs` (142) | file version info | `Info.plist` |

Row 6 has a fourth part the table did not list: `clips/key.rs` (287), the DPAPI
wrap, which is now `#[cfg(windows)]` and needs a Keychain item on the other side.

Every remaining row already has a macOS arm that compiles and refuses. Replacing
one is a self-contained change with a compile gate under it, which is what makes
the rows independent of each other and of their order.

**The rows still open need a decision first, and it is one decision.** Icons,
FSEvents, Spotlight, the pasteboard watcher and the paste chord have no
command-line stand-in the way `open`, `pbcopy` and `pbpaste` did. They need
`objc2` and its Foundation/AppKit crates in the locked stack — permissively
licensed, so ADR-0005 does not rule them out, but a dependency tree is a stack
change and belongs in an ADR. The four rows already written were chosen because
they did not need one.

**Dropped rather than ported:** `uiaccess.rs` (189) and `superkey.rs` (250).
UIAccess has no macOS analogue — the equivalent problem, showing over another
app, is the Accessibility permission, which is a prompt rather than a signed
manifest. `com.rs` (47) goes with them. `superkey.rs` being dropped is what
`Hotkey::second_binding()` returning `None` states in code.

## Three decisions, now made

**ADR-0019 is revisited in `docs/tbc/0013`.** It chose WinHTTP over a Rust HTTP
client for OS TLS, the user's own proxy, and nothing added to the installer.
Every one of those arguments holds on macOS with `URLSession` and none of them
holds with `reqwest`. But `URLSession` from Rust means objc bindings for the one
subsystem that is otherwise trivially portable. TBC-0013 records the bet, the
line at which it fails (roughly 400 lines of objc FFI, which is what `fetch.rs`
costs in WinHTTP today), and the one option to refuse outright: a per-platform
split, where `!s` redirects and validates certificates differently depending on
which machine it runs on.

**The two traits CLAUDE.md named now exist (ADR-0025).** It listed `FileIndex`,
`AppSource`, `ClipboardStore`, `Hotkey` and `SearchProvider` as the seams that
matter; `ClipboardStore` and `Hotkey` had never been written. Both now are, with
the Windows implementation as the only implementor:

- **`clips::os::ClipboardStore`** gathers the four OS calls that were spread
  across three files — the read in `watch.rs`, the write in `launch.rs`, the
  chord in `paste.rs`, the watcher in `watch.rs`. `WindowsClipboard` implements
  it; `UnsupportedClipboard` refuses in words. Both clipboard writes in
  `query.rs` were routed through it, so there is no longer a second path to the
  OS clipboard.
- **`hotkey::Hotkey`** turns out to be almost entirely portable already:
  `tauri-plugin-global-shortcut` binds the accelerator on every target. The
  platform-owned part is `hotkey::SecondBinding`, which is
  `Some(&superkey::WindowsKeyTap)` on Windows and `None` elsewhere — and `None`
  is a real answer, since macOS has no wanted analogue of the Windows-key tap.

**A third seam was found and deliberately left without a trait.** `clips::key`'s
`dpapi` is now `#[cfg(windows)]` with a refusing stub. The macOS answer is a
Keychain item whose access control replaces both the account binding and the
entropy argument, so a `SecretStore` trait shaped against DPAPI today would shape
the Keychain implementation around the wrong idea.

**Distribution is decided in `docs/tbc/0014`: ad-hoc signed, un-notarised, first
launch is right-click → Open.** That is the same posture the Windows build is in
with its unsigned UIAccess helper and its Defender false positive, and the
consistency is the whole argument. TBC-0014 records what would end it — Gatekeeper
narrowing the escape hatch, the updater landing, or the Windows certificate being
bought — and the verdict: buy the Apple membership in the same decision as the
Windows certificate, never before it.

## How to work on it

```
bun run check:macos      # cross-compile check + clippy, from Windows, via zig
```

Needs a zig build unpacked into `%LOCALAPPDATA%\zig\`, on `PATH`, or pointed at
by `TAKYON_ZIG`; the script adds the Rust target itself. `scripts/zig/` holds two
wrappers it drives — `zig cc` pinned to `aarch64-macos` with cc-rs's own
`--target=arm64-apple-macosx` filtered out, which zig's clang frontend rejects.

What this does **not** do: link, bundle, or run a single test. It type-checks and
lints. Anything asserting behaviour needs a Mac, and CI's `macos` job is where
that will start.

## CI is already wired for it

`.github/workflows/release.yml` carries a `build-macos` job, skipped unless the
repository variable `MACOS_BUILD` is set to `true`, and `continue-on-error` so
it can never gate the Windows release. Nothing in this document has to happen
before that switch works — the moment the crate compiles for
`aarch64-apple-darwin`, setting the variable publishes a `.dmg` with no further
edit to the workflow.

## Which phase owns this

Still unassigned. It is comfortably a phase of its own and probably more than
one, and it competes with the two things ROADMAP already calls v1.0 blockers: the
code-signing certificate and the updater. Sequencing it is a decision for whoever
opens v0.11, and the honest reading of the table above is that this is the
largest single piece of work left in the project.

What has been done is the part that is worth doing whether or not the port ever
finishes: the seams are named, the two open questions are recorded as TBC notes,
`identity.rs` knows where a macOS install would keep its data, and the crate
compiles for the target with a check that runs on the development machine. None
of it has been executed on a Mac, and none of it does anything on one.
