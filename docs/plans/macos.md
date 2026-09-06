# macOS — what a port actually needs

**Status: nothing exists.** There is no `tauri.macos.conf.json`, no
`cfg(target_os = "macos")` anywhere under `src-tauri/src`, and no macOS
implementation behind any trait. The crate does not compile for
`aarch64-apple-darwin` today and would not get past the first module that
reaches for the `windows` crate — which is declared under
`[target.'cfg(windows)'.dependencies]`, so on any other target the import
resolves to nothing at all.

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
| 1 | `identity.rs` | `%LOCALAPPDATA%\v3sper\takyon` | `~/Library/Application Support/com.v3sper.takyon` |
| 2 | `sources/apps` (1,350 lines) | Start Menu COM walk, `.lnk` parsing, AppsFolder | bundle walk of `/Applications`, `~/Applications`, `/System/Applications`; `Info.plist` for the display name |
| 3 | `icons.rs` (785) | `IShellItemImageFactory` | `NSWorkspace.iconForFile`, same `icons.bin` on the other side |
| 4 | `index/` (1,110) | `ReadDirectoryChangesW`, Windows Search fallback | FSEvents; Spotlight (`NSMetadataQuery`) as the fallback |
| 5 | `search/fetch.rs` (377) | WinHTTP (ADR-0019) | see the ADR question below |
| 6 | `clips/` (802) | DPAPI key, clipboard format listener, `SendInput` paste | Keychain, `NSPasteboard.changeCount`, `CGEvent` paste |
| 7 | `launch.rs` (439) | `ShellExecuteW` | `NSWorkspace.open` |
| 8 | `window.rs` (1,040) | monitor placement, foreground checks | mostly Tauri already; the placement maths is the part to keep |
| 9 | `sources/system.rs` (419) | Windows settings pages | System Settings panes |
| 10 | `search/browser.rs` (163) | default browser from the registry | `LSCopyDefaultApplicationURLForURL` |
| 11 | `tray.rs` (297) | mostly Tauri, some Win32 | menu bar item |
| 12 | `version.rs` (142) | file version info | `Info.plist` |

**Dropped rather than ported:** `uiaccess.rs` (189) and `superkey.rs` (250).
UIAccess has no macOS analogue — the equivalent problem, showing over another
app, is the Accessibility permission, which is a prompt rather than a signed
manifest. `com.rs` (47) goes with them.

## Three decisions to make before writing any of it

**ADR-0019 has to be revisited, not assumed.** It chose WinHTTP over a Rust HTTP
client for OS TLS, the user's own proxy, and nothing added to the installer.
Every one of those arguments holds on macOS with `URLSession` and none of them
holds with `reqwest`. But `URLSession` from Rust means objc bindings for the one
subsystem that is otherwise trivially portable. This is a real tradeoff and it
belongs in a TBC before the port, not in a commit message during it.

**Two traits CLAUDE.md names do not exist.** It lists `FileIndex`, `AppSource`,
`ClipboardStore`, `Hotkey` and `SearchProvider` as the seams that matter.
`FileIndex`, `SearchProvider`, `Source`, `AgentDriver` and `GameLibrary` are
real. `ClipboardStore` and `Hotkey` were never written, so the clipboard and
hotkey paths are direct calls with no seam to implement against. Introducing
those two traits against the Windows implementation, while it is the only one,
is the first commit of this port and the only one that is worth doing whether or
not the port ever finishes.

**Distribution is still open.** A `.dmg` that is not signed by a $99/yr Apple
Developer account is ad-hoc signed at best, and Gatekeeper makes the first
launch a right-click → Open. That is the same posture the Windows build is in
with its unsigned UIAccess helper, so it is consistent — but it should be a
decision, not a discovery, and it interacts with the open source vs proprietary
question (ADR-0005).

## CI is already wired for it

`.github/workflows/release.yml` carries a `build-macos` job, skipped unless the
repository variable `MACOS_BUILD` is set to `true`, and `continue-on-error` so
it can never gate the Windows release. Nothing in this document has to happen
before that switch works — the moment the crate compiles for
`aarch64-apple-darwin`, setting the variable publishes a `.dmg` with no further
edit to the workflow.

## Which phase owns this

Unassigned. It is comfortably a phase of its own and probably more than one, and
it competes with the two things ROADMAP already calls v1.0 blockers: the
code-signing certificate and the updater. Sequencing it is a decision for
whoever opens v0.11, and the honest reading of the table above is that this is
the largest single piece of work left in the project.
