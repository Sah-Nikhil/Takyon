# Takyon Build Plan

A phased, dependency-ordered roadmap. Each phase ends with something you can
actually use — no phase leaves you holding half-wired plumbing. Check items off as
you go; this file is the tracker.

Domain terms are defined in [`CONTEXT.md`](./CONTEXT.md). Settled tradeoffs live in
[`docs/adr/`](./docs/adr/); decisions we expect to revisit, with switching costs,
live in [`docs/tbc/`](./docs/tbc/); **what each phase left undone, and which phase
owns it, is in [`docs/tbd/`](./docs/tbd/)**; work deferred past V1 is in
[`docs/plans/post-v1.md`](./docs/plans/post-v1.md).

**v0.1 and v0.2 are built.** v0.1's outstanding item is a real code-signing
certificate for the UIAccess helper, which is a v1.0 blocker rather than a v0.1
one. v0.2's is its manual verification pass.

---

## v0.1 — The warm shell

**Goal:** an empty Palette that appears instantly and proves the core bet.

- [x] Bun workspace: `apps/desktop` (Tauri 2 + React 19 + Vite 7 + Tailwind v4) and `packages/shared`
- [x] Global hotkey (`Alt+Space`) via `tauri-plugin-global-shortcut`; a failed registration is reported in a dialog and in the Palette, never swallowed. **Rebinding is v0.6** — it needs the settings UI
- [ ] **UIAccess helper + code signing** — helper crate, `uiAccess="true"` manifest, named-pipe protocol and `scripts/dev-sign-uiaccess.ps1` all exist and work against a self-signed certificate. **A real certificate is still outstanding and is a v1.0 blocker** — [`docs/tbd/v0.1.md`](./docs/tbd/v0.1.md), detail in `docs/plans/uiaccess-signing.md`
- [x] One Palette window created at startup, hidden — never destroyed (ADR-0003)
- [x] Working-set trim on hide; show path does no allocation or window creation. The trim walks the **whole process tree**, because essentially all the resident memory is in WebView2's descendants rather than in the Rust host
- [x] Dismiss on Escape and on focus loss; always opens empty
- [x] Tray icon: settings, quit — with both glyph polarities and a runtime swap when the system theme changes
- [x] Autostart via `tauri-plugin-autostart` + `tauri-plugin-single-instance`, on by default via first-run prompt, **never registered in dev builds**. The `Run` value is named `com.v3sper.launcher`, not "Takyon" (ADR-0011), the OS owns the answer and Takyon re-reads it rather than caching it (ADR-0015), `self_heal_autostart` repoints it after an update moves the binary, and the NSIS uninstall hook deletes both the `Run` value and its `StartupApproved` flag. Two gaps carried forward: a refused write is not reported to the user (v0.6) and the value is written unquoted, which starts mattering when v1.0 installs into `C:\Program Files\Takyon` (v1.0) — [`docs/tbd/v0.1.md`](./docs/tbd/v0.1.md) §3 and §4
- [x] `bun run bench` — all four budgets measured on a release build and written into `docs/tbc/0002`: first pixel p95 **22.6 ms** / 50, **first show after 35 min idle 22.8 ms** / 50, start-to-hotkey **311.6 ms** / 500, idle RSS **~107 MB** / 150. The post-idle show is the one that decided ADR-0003 and it shows no cold-start penalty at all
- [x] Deferred init: hotkey live within ~50 ms of launch; everything else after
- [x] The idle beat: the mark animates while the Palette is open and empty, stops on the first keystroke and while hidden. **Settings → Turn off animations** kills it, as does Windows' own reduce-motion setting. Spec in `docs/brand.md`; storage is `localStorage` behind `src/prefs.ts` until `settings.db` exists

**Exit criteria:** you press `Alt+Space` anywhere in Windows, an empty Palette
appears in under 50 ms, Escape dismisses it, and the benchmark numbers are written
into `docs/tbc/0002` as the first real evidence for or against the warm model.

---

## v0.2 — Launching applications

**Goal:** it replaces the Start menu for opening things.

- [x] `AppSource`: Start Menu `.lnk` (user + machine), `shell:AppsFolder` (UWP/Store), bare `.exe` on PATH, Steam games. **1032 applications in ~480 ms** on the dev machine, release build, on the deferred-init thread. No disk cache — PowerToys Run deleted theirs for three bug classes it could not fix (microsoft/PowerToys#6048), and the expensive half is already persisted in `icons.bin`. Revisit with a number if that walk ever exceeds ~1 s. **`.lnk` files are never `Resolve`d**: raw path plus an existence check, because `Resolve` hunts the network for moved UNC targets and can trigger MSI repair
- [x] Matching: the six-rung ladder from §3, which was **amended** — its 900/800 rungs described the same set, so one was unreachable
- [x] Icon extraction into a single memory-mapped `icons.bin`, lazy fill, off the UI thread, placeholder while missing. Reaches the webview through a **`takyon-icon://` URI scheme**, not as bytes in the query response; §6 records why
- [x] Launch on Enter; Palette dismisses immediately, not after the app appears. Everything goes through `ShellExecuteW`, including plain executables, so nothing inherits our handles
- [x] **`Ctrl+K` action menu** as a shared primitive — every Source and Mode contributes actions to it (open, reveal in Explorer, copy path, run as administrator). A packaged app is offered only Open, because it has no file
- [x] Modifier accelerators for the common actions, table-driven in `actions.rs` and listed inside the menu so they're discoverable rather than folklore. **The rebinding UI is v0.6** — it needs `settings.db`; the mechanism is data now so that phase is a change of values, not of code
- [x] Keyboard-only navigation. **The list is not virtualised**, deliberately: §3 caps it at twelve Entries and eight are on screen, so a windowing library would add a dependency and a measurement pass to avoid rendering four rows. Revisit if a Source ever returns an unbounded set
- [ ] **Run the manual verification script** ([`docs/verify/v0.2.md`](./docs/verify/v0.2.md)). What is open and who owns it: [`docs/tbd/v0.2.md`](./docs/tbd/v0.2.md). Short version — **26 of 41 steps confirmed, 3 partial, 2 blocked, 1 failed, 9 never run** after two scripted passes on 2026-08-27, repeatable with `scripts/verify-drive.ps1`. All nine remaining need a person: an uninstall, the UAC prompt, a Steam game, or an application actually starting. That pass ran entirely at 150% scaling, which is what the dev machine has always been, so `D7` is nearly closed. **C5 fails**: `icons.bin` is 12 bytes and has never held an icon. Steam stays blocked because this machine's library holds no game

**Exit criteria:** you use it instead of the Start menu for a full day and don't
reach for the Start menu once. *Not yet claimed — that is a day of use, not a
test run.*

**Packaged as 0.2.0 on 2026-08-28**, the first release artifact for this phase —
`releases/` held nothing newer than v0.1.3, dated before five of v0.2's own
commits. Cut deliberately before the exit criterion is met, because both the
criterion and the unfinished verification script need an installed build to run
against.

**That installer also carries v0.3 task 0**, which is why `code` reaches the
right application and `explorer` is no longer called a Store app in a build
labelled 0.2.0. `C5` above is fixed in it: `icons.bin` holds icons.

**0.2.2 carries v0.3 tasks 1 to 3** — the ranker learns. Three patch releases now
carry v0.3 work, which is awkward and deliberate: release versions move when a
phase ships, and v0.3 ships as 0.3.0 once its Sources land. The alternative was
claiming a phase that is a quarter done.

**0.2.1 follows the same day** with the scrollbar the Palette had been leaving to
Windows — white track, stepper arrows, inside a near-black panel. Now a 4px inset
thumb drawn from the palette's own token ([ADR-0016 is the sibling
rule](./docs/adr/0016-the-second-line-is-disambiguation.md); the scrollbar
reasoning is a comment in `styles.css`, including why `color-scheme: dark` is the
wrong fix for a `transparent: true` window).

---

## v0.3 — Ranking that learns, and the Sources it ranks

**Goal:** it starts guessing right before you finish typing, and it knows about
the things v0.2 could not see.

- [x] **Fix `EntryId` first.** Launch arguments are excluded from identity, so nine Start Menu shortcuts collapse onto `cmd.exe`, three onto `javacpl.exe`, three onto x86 `powershell.exe` — **15 distinctly-named applications** dropped, then returned by `AppsFolder` mislabelled `Store app` with a truncated action menu and past `is_noise`. Frecency keys on `EntryId`, so this lands before task 1 or the usage database is wrong from the first launch ([`docs/tbd/v0.2.md`](./docs/tbd/v0.2.md) §9). **Done:** arguments join the id, `Store app` is detected rather than asserted (38 packaged of 112 AUMIDs), `is_noise` moved to `sources/apps/noise.rs` and now covers both discovery paths
- [x] **Make `icons.bin` actually persist.** `flush()` runs once per launch, right after the walk, before a single icon has been extracted — the file has always been 12 bytes ([`docs/tbd/v0.2.md`](./docs/tbd/v0.2.md) §10). **Done:** written on a 750 ms debounce after extraction instead; 12 bytes → 492 KB in one driven session
- [x] **The second line only when it disambiguates** ([ADR-0016](./docs/adr/0016-the-second-line-is-disambiguation.md)) — landed with task 0. Zero of 1036 applications currently share a title, so the applications list shows none at all; the rule earns its place as this phase's Sources start competing in one list
- [x] **Duplicate handling — settled without the learned-collapse feature.** Built learned collapse (icon + process evidence, [TBC-0008](./docs/tbc/0008-learned-identity-aliases.md)), then **retired it**: it needed two learned launches before it could hide a duplicate, so a fresh machine showed both rows — counterintuitive, and neither Raycast nor PowerToys does it ([prior art](./docs/prior-art/ranking-and-dedup.md)). Replaced by two static rules correct from first launch: a Windows-dir binary the shell already lists as an app is dropped at discovery (`explorer` joins `calc`/`notepad` in `WINDOWS_DIR_APP_DUPLICATES`), and two genuinely different same-named executables stay two rows disambiguated by **version** (`node` 24.14.1 beside `Node.js` 26.7). Exe-stem matches also yield to name matches, so `chrome` no longer surfaces a fork's `chrome.exe`
- [x] `frecency.db` — per-Entry decayed frequency + recency, updated on every launch. `rusqlite` with SQLite bundled in; WAL; the `usage` table from IMPLEMENTATION_PLAN §4. Written only for Open and Run as administrator, never for reveal or copy path
- [x] Ranking: Frecency over raw match quality; Apps always sort above documents, and **System entries carry a 0.8 weight applied after the Frecency lift** — `dis` matched Discord and the Display page at 796.5 apiece and a 0.3% Frecency gap picked the winner, which is a coin flip rather than a ranking ([`docs/tbd/v0.3.md`](./docs/tbd/v0.3.md) §10). Saturating lift, `1 + 0.6·w/(w+1)`, applied in the pipeline rather than in a Source — so a Source now hands up 64 candidates against the 12 the Palette shows, or a much-used Entry would be cut one step before its lift
- [x] **Stability rule**: the top Entry freezes 100 ms after the last keystroke; late Sources may only append below. Keyed on the exact query string, so a new keystroke is a new question and clears it. Inside the delay the list still reorders — the guarantee is about a *stopped* query, not a frozen first answer
- [x] User-defined aliases resolved before matching, from the `aliases` table in `settings.db`. Applied to the app list **in place**, so a new alias is live without a re-walk — discovery runs once at login, and an alias needing the next one would look broken all session. **The editor is v0.6**; until then an alias is one `INSERT` by hand
- [x] Recently-opened files as a cheap Bangless Source — `%APPDATA%\Microsoft\Windows\Recent`, no index, no watcher. Always below apps by kind tier. Rebuilt on a 20-second timer rather than per keystroke, because a few hundred shortcuts through COM would blow the 20 ms budget many times over. No **Run as administrator**: a document cannot be elevated
- [x] **A version beside the title where two same-named executables disagree** ([ADR-0016](./docs/adr/0016-the-second-line-is-disambiguation.md), extended) — `node` 24.14.1 beside `Node.js` 26.7, two R installs, Chrome beside Helium. Nothing else on those rows told them apart, because ADR-0016's second line triggers on a shared *title* and these do not share one. **Only the colliding filenames are read**: all 1233 executables costs 13.3 s against a 450 ms walk, the 16 that collide cost 3 ms
- [x] **System entries Source** — control panel tasks via the All Tasks shell folder (`{ED7BA470-…}`, **198 tasks** walked with the `IEnumShellItems` path `appsfolder.rs` uses) plus a curated `ms-settings:` table (35 pages). Two Kinds, split once the 198 tasks were seen competing: `System` (the 35 curated pages) **shares the App rank tier**, `SystemTask` (the control-panel tasks) sits below every app because they are long sentences that only ever match by word prefix — a settings page is a destination like an app, so it competes on match quality and Frecency rather than sitting below apps (corrected after `display` surfaced `DisplaySwitch` above the Display page; IMPLEMENTATION_PLAN §3). Launch is by the item's captured **PIDL** through `SEE_MASK_IDLIST`, not a parsing name — an All Tasks item is positional (`::{ControlPanel}\0\…`) and has no reparseable name. `ComScope` extracted to `com.rs`, now shared by three callers. All 198 PIDLs bind for launch in a test; the actual window-open is manual (verify §SY). Closed the largest v0.2 coverage gap — Raycast surfaces ~1187 here and Takyon surfaced none
- [x] **Game launcher Sources** — built as a seam rather than one more special case. `GameLibrary` (`sources/apps/games.rs`) with **Steam and Epic** behind it, and one `LaunchTarget::Game { launcher, id }` replacing the per-store variant that was starting to multiply. Steam's EntryIds are untouched by the move: `steam:440` is what v0.2 wrote and what `{slug}:{id}` still produces, so nothing learned was invalidated. Epic reads the `.item` JSON manifests and **existence-checks the executable** — all seven on the dev machine are stale, so the Source correctly contributes **nothing here**, while Raycast lists all seven as launchable. DLC needs no rule: its manifest names no executable. Games launch through the launcher's own URI, never their exe (`com.epicgames.launcher://apps/<AppName>?action=launch&silent=true`), so DRM, cloud saves and playtime all still work. **Xbox and Game Pass need no Source at all** — MSIX packages with AUMIDs, already listed by `appsfolder.rs`. GOG and Battle.net are neither installed here nor built: both would be written blind against docs and verified against nothing ([`docs/tbd/v0.3.md`](./docs/tbd/v0.3.md) §9). EA stays deferred — its install path exists only in a log, not a manifest
- [x] **Desktop shortcuts**, reusing the `.lnk` walk, **kept only when no other Source already found the app**. Desktop loses every collision so `EntryId` stays on the Start Menu copy. Top level only — a folder on the Desktop is the user's filing, not an application menu. **The roots come from `SHGetKnownFolderPath`, not `%USERPROFILE%\Desktop`**, and that is not pedantry: this machine's Desktop is OneDrive-redirected, so the env-var path holds 9 stale shortcuts while the real one holds 15. Measured: **15 shortcuts, 0 new applications** — 13 already found at the same target, and both Roblox entries point at an old `C:\Program Files` install that the Start Menu copy supersedes. The rule earns nothing here and costs one `read_dir` of two directories; it pays on a machine where a portable executable's only shortcut is on the Desktop
- [x] **winget Source — measured, and declined.** Not 4 packages on the dev machine but **115**, and the measurement was decisive against building it twice over. Every launchable one is already reachable by the name a person would type — `terminal`, `visual studio code`, `onedrive`, `ollama`, `rustup`, `zen`, `docker` — while the apparent misses are winget's own naming ("Roblox Player for sahni") and runtimes nobody launches (VC++ redistributables, `WindowsAppRuntime.*`, SDKs, a printer driver). More decisively: **`winget list` carries no launch target** — a package id and a version, no path, no AUMID, and no "winget run" — so the Source could only list rows it cannot open. It is an installer inventory, not an application list ([`docs/tbd/v0.3.md`](./docs/tbd/v0.3.md) §12; listing checked in at `tests/winget-list.txt`)

**Exit criteria:** after a week of use, your ten most-used applications are all
reachable in one or two keystrokes — and the new Sources have pushed none of them
down. `bluetooth` reaches the Bluetooth settings page without the Start menu.

---

## v0.4 — Calculator and converter

**Goal:** the second reflex after launching.

- [ ] Inline expression evaluation, result as the top Entry, Enter copies it
- [ ] Unit and currency conversion — currency needs a rate source, so it is **Bang-gated or explicitly opt-in** (ADR-0002)

**Exit criteria:** you stop opening a calculator app.

---

## v0.5 — Clipboard history

**Goal:** useful without ever becoming a liability.

- [ ] Clipboard watcher; `clipboard.db` (SQLite, WAL), key wrapped with DPAPI in `creds\`
- [ ] Retention chosen from a fixed list: forever / 6 months / 1 month / 1 week / 1 day; expiry deletes rather than hides
- [ ] Honour `ExcludeClipboardContentFromMonitorProcessing`; user-editable foreground-app blocklist
- [ ] Clipboard Entries **never appear in Bangless results** (ADR-0006) — own view only
- [ ] Paste-back preserving format where sensible

**Exit criteria:** copying a password out of a password manager leaves no trace in
the history, verified by inspecting the database.

---

## v0.6 — Settings

**Goal:** every decision so far becomes the user's.

- [ ] Separate settings window, `Ctrl+,` from the Palette and from the tray
- [ ] `settings.db`; UI is the only editor (no hand-edited config file)
- [ ] **Two-tier navigation, Raycast-style**: a short fixed set of app-level pages (General, Launcher, Keyboard, Advanced, About) above a divider, then one alphabetical page per feature (Applications, Calculator, Clipboard History, File Search, AI…). Every future Source or Mode adds a tier-two page without touching the navigation
- [ ] Settings search box — tier two becomes unbrowsable once it passes ~15 entries
- [ ] Migrate the v0.1 "Turn off animations" switch from `localStorage` into `settings.db` — `src/prefs.ts` is the only reader, and it belongs on the Appearance page
- [ ] Hotkey rebinding, autostart, tray visibility, retention, blocklist, aliases, monitor placement, recents toggle, Bangless-file-search toggle (default off), index roots + exclusions with a live entry count
- [ ] **Autostart moves to General and gains nothing else.** The registration shipped in v0.1; v0.6 owes it a home, a reported error when the registry write is refused, and the discipline not to mirror it into `settings.db` (ADR-0015). No "close to tray" and no "start hidden" — tesseract has both, the Palette has no ✕ and always starts hidden
- [ ] Appearance: follow system by default, plus a manual light/dark override and pinned interface-size options (full theming is post-V1)
- [ ] Local crash logs written to disk with a settings button to open the folder — **nothing is ever sent** (ADR-0010)
- [ ] Every control offers **pinned, explicit options** — chips rather than free-text or sliders where the option set is small (Raycast's hotkey and interface-size controls are the reference)
- [ ] Apply-on-change with a ~1 s debounce and a brief "Applied" confirmation; **destructive settings get a confirmation dialog naming the consequence** ("Setting retention to 1 day will permanently delete 4,312 clipboard items")

**Exit criteria:** nothing in the app requires editing a file or a registry key.

---

## v0.7 — File search (`!e`)

**Goal:** fast, scoped file search with no elevation and no service (ADR-0007).

- [ ] `FileIndex` trait; unelevated parallel directory walk over curated roots
- [ ] Memory-mapped inverted index on disk; mmap at boot, never re-walk at startup
- [ ] `ReadDirectoryChangesW` watchers per root for live updates
- [ ] **Watcher-overflow detection** (`ERROR_NOTIFY_ENUM_DIR`) triggering a scoped rescan of the affected subtree, with an index generation counter so a stale index is never silently served
- [ ] Default roots (Desktop, Documents, Downloads, code dirs) + user-editable roots and exclusions in settings; skip `node_modules`, `.git`, `AppData` by default
- [ ] Windows Search fallback for locations outside the walked roots
- [ ] `!e` Mode: filenames and folder names, actions for open / reveal in Explorer / copy path
- [ ] Optional setting: surface file Entries Bangless too (default off, always below apps)

**Exit criteria:** `!e` returns in under 20 ms at p95, the initial walk completes
in under 60 s in the background without competing with login, the index survives a
reboot without re-walking, and a file created one second ago is findable.

---

## v0.8 — Web search (`!s`)

**Goal:** an answer in the Palette, not a browser tab.

- [ ] `SearchProvider` trait; Brave Search API behind it (ADR-0005)
- [ ] Parallel page fetch + Readability-style extraction — no browser (ADR-0005)
- [ ] Synthesised answer rendered inline with its sources
- [ ] Enter opens the query in the default browser and default search engine
- [ ] Selecting a source Entry opens that URL

**Exit criteria:** a question like "Ferrari in F1" returns a readable synthesised
answer with working source links, without opening a browser.

---

## v0.9 — Claude Code (`!c`)

**Goal:** ask a question from the hotkey; get an answer in place.

- [ ] `claude` CLI subprocess, streaming JSON output rendered as it arrives
- [ ] Detect missing or logged-out CLI and explain it, rather than failing silently
- [ ] Inline answer in the Palette; **tools disabled** on this path
- [ ] Promotion to the Chat Surface on the first follow-up (ADR-0001); full agent mode lives only there
- [ ] Chat Surface: its own window, own lifecycle, survives Palette dismissal

**Exit criteria:** a question answers inline in the Palette, a follow-up promotes
into a Chat Surface, and Escape on the Palette never destroys a conversation.

*Design not finalised — session model, working directory, and tool policy are
still open. Write `docs/plans/v0.9-claude-code.md` before starting.*

---

## v1.0 — Ship

- [ ] NSIS installer into `C:\Program Files\Takyon` (UIAccess needs a trusted location), code signing, `tauri-plugin-updater`
- [ ] **Quote the `Run` value.** `auto-launch` writes it unquoted; harmless from `%LOCALAPPDATA%`, a fragility once the path contains a space
- [ ] First-run experience: hotkey introduction, autostart prompt, permissions
- [ ] Full benchmark pass against all four budgets on a cold machine
- [ ] Revisit `docs/tbc/0002` with real numbers and resolve or retire it
- [ ] Decide open source vs proprietary, and license accordingly

**Exit criteria:** someone who is not you installs it and uses it for a week.

---

## Still undecided

These block nothing today but should be settled before they become expensive:

- **The name.** "Takyon" collides with `claude-task-master`, a widely-used
  Claude Code task-management tool — same audience, immediate confusion.
  **No longer urgent**: ADR-0011 separates the app's Windows identity from its
  display name, so renaming stays a UI-copy change rather than a data migration.
  Still worth settling before anything is published under it.
- **Open source vs proprietary.** Constrains dependency licensing; already ruled
  out one option (ADR-0005).
- **Portable / no-installer mode** — in scope or not.
- **macOS**, deliberately post-V1 (`docs/plans/post-v1.md`).
