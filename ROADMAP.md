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

**Packaged as 0.3.0 on 2026-09-01** — the first release whose number matches its
phase. Every task is done: 0 to 10 built and green, 11 measured and declined.
`bluetooth` reaches its settings page, which is half the criterion. *The other
half is not claimed*: a week of use is a week, not a test run.

The verification script is closed except for what this machine cannot answer.
§SY1–SY5, §EP3, §DK1–DK4 and §RK1–RK7 all pass, and the 14 steps that need no
window are now **driven** by `v0_3_run_the_verify_steps_that_need_no_launch`
rather than typed. What is left is written down as blocked, with what would
unblock each: Recents needs `Start_TrackDocs` on, Epic and Steam each need a game
installed ([`docs/verify/v0.3.md`](./docs/verify/v0.3.md) § Permanently blocked).

Two things the phase learned the hard way, both from live use rather than tests.
`dis` selected the Display settings page over Discord on a **0.3%** Frecency gap —
fixed with a per-Kind weight (App 1.0, System 0.8) applied after the lift, not a
tier. And `disk` reached the Storage page above **Disk Cleanup**, because task 8
had loaded its curated keywords onto the *user alias* rung: a keyword Takyon ships
must never outrank a name the user can see. `TIER_KEYWORD` at 850 fixed it.
Both are in [`docs/tbd/v0.3.md`](./docs/tbd/v0.3.md) §10.

---

## v0.4 — Calculator and converter

**Goal:** the second reflex after launching.

- [x] Inline expression evaluation, result as the top Entry, Enter copies it
- [x] Unit conversion — length, mass, temperature, data, time, all from static tables
- [ ] Currency conversion — **deferred to v0.8**. It needs a rate source, so ADR-0002 requires it to be Bang-gated or opt-in, and at v0.4 neither gate exists: `bang.rs` is v0.8 and there is no settings store until v0.6. No currency code was written, and a test asserts no currency unit resolves. [`docs/tbd/v0.4.md`](./docs/tbd/v0.4.md) §2
- [x] **New:** `=` forces a calculation, and a Settings switch chooses whether anything else may

**Exit criteria:** you stop opening a calculator app. **Met** — `12*1.18` and
`40 kg to lb` both answer inline and offline.

The parser is hand-rolled rather than a crate; the plan said otherwise and
[TBC-0011](./docs/tbc/0011-hand-rolled-expression-parser.md) records why it
changed. Detection follows Raycast's rules, adopted after watching them work and
fail on this machine — including the one that fails: in the default Policy,
`2022` takes the top row from **Adobe Photoshop 2022**. That is a known cost with
a one-switch cure, written up in [`docs/tbd/v0.4.md`](./docs/tbd/v0.4.md) §1.
ADR-0016 gained a Calc exemption so an answer keeps its expression.

---

## v0.4.5 — Presentation

**Goal:** the Palette reads like Raycast's, which is cleaner than what v0.4
shipped.

- [x] Calculator card — expression, arrow, result, a label under each, captioned. Green-lit against the v0.4.5 installer, 2026-09-02
- [ ] Section headers — **not approved.** Grouping by category puts back the gate v0.3 removed: `dis` returned Display above Discord and `disk` returned Storage above Disk Cleanup until apps and settings were made to compete in one list. See the plan
- [x] Right-aligned kind labels — "Application", "Settings". ADR-0016 amended with why a Kind column is not the decoration its second-line rule rejects
- [x] Footer bar showing what Enter does — "Open ↵ | Actions Ctrl K", becoming "Copy answer" on a calculation. Replaces the floating `Ctrl K` hint

**Exit criteria:** a calculation reads as an answer rather than a list row; every
row says what it is; the footer says what Enter will do; and the window is exactly
as tall as its content in each combination.

Deliberately gated: task 1 shipped and was judged on screen before 3 and 4 were
built. Section headers stay unbuilt. Plan in [`docs/plans/v0.4.5-presentation.md`](./docs/plans/v0.4.5-presentation.md),
verification in [`docs/verify/v0.4.5.md`](./docs/verify/v0.4.5.md).

The card is not `ROW_HEIGHT` tall, so `window::content_height` and
`paletteHeight` both had to learn about it (TBC-0006). That arithmetic is the one
part the mocked visual layer cannot check — a card that overflows screenshots
perfectly and clips against the real window — so it is covered by Rust unit tests
instead.

---

## v0.5 — Clipboard history

**Goal:** useful without ever becoming a liability.

- [x] Clipboard watcher — `AddClipboardFormatListener` on a **message-only window** on its own thread, because the Palette's window belongs to Tauri's message loop. Polling `GetClipboardSequenceNumber` was the alternative and is worse twice over: it misses a copy-then-copy inside the interval, and it wakes a process whose premise is being idle (ADR-0003). `clips.db` (SQLite, WAL, `PRAGMA secure_delete = ON` at every open), AES-256-GCM per row with a fresh nonce, key wrapped with DPAPI in `creds\clip.key.dpapi` and bound to the user account
- [x] Retention chosen from a fixed list: forever / 6 months / 1 month / 1 week / 1 day; expiry deletes rather than hides. Swept at startup and hourly. **The default is one month**, and it lives in `settings.db`'s `settings` table rather than in the frontend — the sweep runs before any window exists, so a default held in `localStorage` would sweep over a chosen `forever` on every launch. That table was v0.6's and had to open early; [`docs/tbd/v0.5.md`](./docs/tbd/v0.5.md) §1 owns the UI that edits it
- [x] Honour `ExcludeClipboardContentFromMonitorProcessing`, **and `CanIncludeInClipboardHistory`** — an application that told Windows not to keep a copy meant it for us too. Plus the user-editable blocklist, matched on the **clipboard owner's** executable rather than the foreground window's: a copy from a context menu leaves the owner right and the foreground wrong
- [x] Clipboard Entries **never appear in Bangless results** (ADR-0006) — own view only. Structural rather than filtered: `ClipStore` does not implement `Source`, so it is not in `query.rs`'s registry and there is no rule for a later phase to forget
- [x] **A "Clipboard History" Command, found by typing** — Raycast's shape, and the correction that made this phase usable. A Bang is a shortcut for people who already know it exists; a launcher's answer to "where is my clipboard history" has to be typing `clipboard`. New `EntryKind::Command` and `sources/commands.rs`, sharing the App rank tier because a command is a destination you ask for by name. **This does not weaken ADR-0006**: what is excluded Bangless is clipboard *content*, and the command row carries none — the shoulder-surfing guarantee is untouched, which is why the Source could be added at all
- [x] **The history surface** — a full-window View the Palette navigates *into*: back arrow, filter input, type control, day-grouped list, detail pane with Information (source, type, characters, copied), and a "Paste ↵ | Actions Ctrl K" footer. **Not a second window**: another WebView2 would cost the login budget and a large share of the 150 MB ceiling, so the warm Palette resizes to `VIEW_HEIGHT` and `Escape` navigates back rather than dismissing
- [x] **`!v` becomes a toggle** (`clips.bang` in `settings.db`, default on). Turned off, `!v` is ordinary text that falls through to Bangless and matches nothing — the command is still there, so the feature never disappears with the accelerator
- [x] Paste-back — clipboard first, then a synthesised `Ctrl+V` after an 80 ms settle. Ordered that way deliberately: a failed keystroke leaves the user one manual paste away, a failed copy leaves them with nothing. Format preservation is a seam (`ClipKind`) rather than code, because only text is captured at v0.5
- [x] **New:** `bang.rs`, brought forward from v0.8 because `!v` needs it. Position 0, ident to whitespace, unknown Bang falls through to Bangless (IMPLEMENTATION_PLAN §9). The registry and the `!` picker stay parked in [`docs/plans/bang-registry.md`](./docs/plans/bang-registry.md)

**Exit criteria:** copying a password out of a password manager leaves no trace in
the history, verified by inspecting the database. *Not yet claimed* — the storage
half is proven twice over, by `tests/clips_disk.rs` against a real file (a swept
clip's ciphertext is absent from `clips.db`, its WAL and its shm) and by a driven
pass against the release build on 2026-09-02, which copied a string, found it
through `!v`, got the exact original bytes back from `Ctrl+Enter`, and got
**nothing** for the same string typed Bangless. What is missing is the password
manager itself, and a second Windows account for the DPAPI claim. **25 of the 35
steps** in [`docs/verify/v0.5.md`](./docs/verify/v0.5.md) are confirmed, driven by
[`scripts/verify-drive-v0.5.ps1`](./scripts/verify-drive-v0.5.ps1) against an
isolated `LOCALAPPDATA`; [`docs/tbd/v0.5.md`](./docs/tbd/v0.5.md) §2 lists what
each of the nine remaining ones needs, and none of them is a code change.

**Two bugs survived every automated layer and died on the first driven run**, both
in the watcher — the one file whose Win32 half no test can reach. A copy past the
4 MB cap was **stored truncated** rather than skipped: the scan stopped *at*
`MAX_CHARS`, so an oversized clip arrived exactly at the limit and looked
acceptable. A 5 MB copy became a row of exactly 4,194,304 characters, which is
half a document filed silently. And **`source_exe` was NULL on every row**:
attribution used `GetClipboardOwner` alone, and Windows reports no owner at all
for a .NET or WinRT copier, which is most of them. With no source recorded the
blocklist could never match — half of ADR-0006's exclusion story, dead, with a
green test suite above it. Owner now falls back to the foreground window, which
is what the plan asked for by name. Both are covered by unit tests on extracted
pure functions (`text_within_cap`, `attribution`) so they cannot come back.

**Packaged as 0.5.0 on 2026-09-03**, the second release whose number matches its
phase. The clipboard is reachable three ways — the **Clipboard History** command
in ordinary results, the surface it opens, and `!v` for anyone who wants the
shortcut — and the exit criterion is still the one thing not claimed, because it
needs a password manager this pass could not drive.

Two things the phase decided rather than inherited. **Retention defaults to one
month** — history stays useful, and a password copied once does not sit in the file
for years; either extreme is one setting away. And **a repeat of the newest clip
moves that row rather than adding one**, because copying the same thing twice is
routine and two identical rows are never what anyone wanted — but only the *newest*
collapses, so A, B, A is genuinely three events.

---

## v0.6 — Settings

**Goal:** every decision so far becomes the user's.

**Built in three slices**, because the phase is the largest in V1 and one
unreviewable diff was the alternative. Slice 1 is the shell and the plumbing;
slice 2 the remaining feature pages; slice 3 appearance and the colour question.

- [x] **Slice 1.** Separate settings window, `Ctrl+,` from the Palette and from the tray
- [x] `settings.db`; UI is the only editor (no hand-edited config file). Extended rather than created — `prefs.rs` and the `settings` table opened at v0.5 for `clips.retention`
- [x] **Two-tier navigation, Raycast-style**: a short fixed set of app-level pages above a divider, then one alphabetical page per feature. Every future Source or Mode adds a tier-two page without touching the navigation — `navSections` is pure and `nav.test.ts` holds the promise by appending a page and asserting where it lands. **Pages for features that do not exist yet are deliberately absent**: File Search arrives with v0.7 and AI with v0.9, and shipping them now as disabled rows would put dead controls in a window whose whole point is that every control does something
- [x] Settings search box — and it returns **individual settings, not page names**, because past ~15 pages "Clipboard History" is not what someone is hunting for, "retention" is. t3code's `searchSettings` is the reference
- [x] Migrate the v0.1 "Turn off animations" switch from `localStorage` into `settings.db`. **The calculator Policy went with it**, since `prefs.ts` held both and migrating one is half a job. Migration is idempotent — a key already stored wins, so a stale legacy value cannot undo a later choice
- [x] **Autostart moves to General and gains nothing else.** The one behavioural fix landed: a refused registry write now shows its error beside the control and refetches in a `finally`, closing [`docs/tbd/v0.1.md`](./docs/tbd/v0.1.md) §3. Still read from the OS on every mount, never mirrored (ADR-0015). No "close to tray" and no "start hidden"
- [x] Every control offers **pinned, explicit options** — chips rather than free-text or sliders. `controls.tsx` is the whole vocabulary: `Group`, `Row`, `Switch`, `Chips`, `useApplied`
- [x] Apply-on-change with a brief "Applied" confirmation, and **no save button anywhere**. The optimistic value is always replaced by what the refetch returns, never by what was clicked — ADR-0015's rule for autostart, applied to every control because it costs nothing
- [x] **A bug the phase found rather than fixed:** the calculator Policy was pushed from the frontend only, so every keystroke before the Palette mounted answered under Automatic whatever had been chosen — a restart silently reverting the setting. Rust now reads it at startup, as it already did for `clips.bang`
- [x] **And the one that mattered more: the Settings window had never rendered.** `settings::open` built the window on the main thread, where `WebviewWindowBuilder::build()` blocks waiting for the event loop it is itself blocking. The frame appeared — right size, right title — and only the webview never loaded, so it was a title bar over a white rectangle, and `build()` never returned so nothing after it logged. It now spawns. The call path is unchanged since v0.1, and nothing caught it because the visual suite reaches that route through Vite rather than through Tauri. See CLAUDE.md gotchas; `scripts/verify-drive-v0.6.ps1` now samples pixels for it
- [x] **Slice 2.** Hotkey rebinding from pinned chords with a reset — the old binding is released first, and a refused chord restores the previous one rather than leaving nothing bound. Tray visibility, **refused while the hotkey is unregistered** because the tray would be the only way in and out. Monitor placement (cursor or primary — two choices, not a monitor list, since a saved index is silently wrong once a display is unplugged). Recents toggle, aliases editor, clipboard retention, `!v` toggle and the capture blocklist. Closes [`docs/tbd/v0.3.md`](./docs/tbd/v0.3.md) §3 and [`docs/tbd/v0.5.md`](./docs/tbd/v0.5.md) §1, §6, §7
- [x] **The destructive confirmation names the real count**, asked from Rust before the change: "will permanently delete 3 clipboard items… overwritten, not moved — there is nothing to restore from"
- [x] **Slice 3.** Appearance: follows Windows by default, with an override that wins in **both** directions, plus pinned interface sizes. **A full light theme**, not just the plumbing — and only four tokens are restated, because every separation is an alpha of `--color-fg` and follows for free
- [x] Interface size is a root `zoom` **mirrored by Rust**, which scales the Palette's window by the same integer percentages. A font scale would have left the Palette's fixed pixel heights behind
- [x] Local crash logs (ADR-0010): a panic hook writing to `logs\panic.log` under the data directory, capped, with an Advanced page button that opens the folder. **Nothing is ever sent, and there is no code path that could**
- [ ] **Resolve the colour question** in `docs/brand.md`. The light palette slice 3 ships is *derived* — same Cherenkov hue darkened to survive white — and still needs the real decision. Everything is `color-mix` over `--color-plate` and `--color-fg`, so the swap remains one edit
- [ ] Bangless-file-search toggle, index roots + exclusions with a live entry count — **deferred to v0.7**, which is when an index exists to have roots
- [ ] [`docs/tbd/v0.3.md`](./docs/tbd/v0.3.md) §10's per-Kind ranking weight, which named v0.6 as its owner. **Not built** — it is a ranking constant rather than a setting, and exposing a weight as a control is a worse answer than choosing the number

**Exit criteria:** nothing in the app requires editing a file or a registry key.
*Met for everything that exists: the hotkey, autostart, appearance, interface
size, placement, tray, recents, aliases, retention, the `!v` Bang and the
clipboard blocklist are all reachable from the window. Index roots are the one
listed item still unreachable, and there is no index for them to point at
until v0.7.*

**Packaged as 0.6.0.** Cut deliberately with two things outstanding, both
recorded rather than quietly closed:

- **The colour question is still open** (`docs/brand.md`). The plan made it a ship
  gate; the light palette in this release is *derived* — the same Cherenkov hue
  darkened to survive white — not decided. Every surface is `color-mix` over
  `--color-plate` and `--color-fg`, so settling it stays one edit and redraws no
  asset.
- **`docs/verify/v0.6.md` has not been run by a person.** S1, S3 and S5 are driven
  by `scripts/verify-drive-v0.6.ps1`; sections A, M, K, L, C, A2, P2 and D need a
  real registry, a pre-v0.6 profile and a second monitor. The installer exists
  partly so that pass has something to run against.

What *is* verified: 408 Rust tests, 21 TypeScript, 67 Playwright, all four
performance budgets, and the settings window driven against the release binary
with a pixel check that it actually paints.

**Three changes landed after 0.6.0 was cut**, so they are not in that installer:

- **The Keyboard row was unusable below ~900px.** Six chips are wider than the
  content pane at every window size, and the control refused to shrink — it
  squeezed the label to one word per line and drew the first chip over it. Rows
  wrap now, and a Playwright test runs at the 680×480 minimum.
- **Autostart is on by default rather than asked.** The first-run modal existed
  because declining had to be possible and there was nowhere else to do it — and
  the Settings window that was supposed to be that place had never rendered.
  Every guard is unchanged: never from a debug build, a `target\` directory, or
  under the bench harness.
- **The Settings window draws its own title bar.** Windows' native bar is a light
  strip with square buttons on a window built from near-black surfaces, and it
  cannot be themed. Undecorated plus `settings/TitleBar.tsx`, which also means it
  follows the light theme.

**Manual verification:** [`docs/verify/v0.6.md`](./docs/verify/v0.6.md), written
for slice 1 and grown by each later slice. Almost everything slice 1 claims is
covered automatically; what is left is the real Tauri window, the real registry
and the process lifetime.

---

## v0.7 — File search (`!e`)

**Goal:** fast, scoped file search with no elevation and no service (ADR-0007).

Delivered in three slices — the index offline, then keeping it true, then the
surface. See the plan for what each covers and why.

**Slice 1 landed.** Measured on a release build against this machine's real roots:
26,844 entries, a **916 ms** walk against a 60 s budget, a **2.5 MB** index against
the ~150 MB TBC-0005 watches for, and a query worst case of **568 µs** against the
20 ms p95 target.

Queries under three characters take a **linear prefix scan**, not the recent set
this plan first specified: 646 µs over 26,846 names is 3% of the budget, and it
finds a two-letter folder the recent set has never seen. `C:\Data\0Projects\Create\HH`
is the case that raised it.

**Slice 2 landed.** All four exit criteria now met — a file created a second ago is
findable through the real `ReadDirectoryChangesW` path, asserted end to end rather
than by calling the apply function directly. The index is wired into boot: mapped
if it exists, walked in the background only if it does not, watched either way.

- [x] `FileIndex` trait; unelevated parallel directory walk over curated roots. Reparse points are indexed but never descended, or an unbounded-depth walk cycles through the first junction it meets
- [x] Memory-mapped inverted index on disk; mmap at boot, never re-walk at startup. Generation-named files, because Windows will not replace a file that is still mapped
- [x] `ReadDirectoryChangesW` watchers per root for live updates. Deltas land in an in-memory overlay rather than rewriting a 2.5 MB file per event; a query reads mapped hits minus deletions plus additions, and a rebuild folds the overlay back in
- [x] **Watcher-overflow detection** triggering a scoped rescan of the affected subtree, with an index generation counter so a stale index is never silently served. `Stale` is set **before** the rescan, so a rescan that cannot run leaves the index saying so rather than reporting Ready — there is a test for exactly that ordering
- [ ] Default roots (Desktop, Documents, Downloads, code dirs) + user-editable roots and exclusions in settings; skip `node_modules`, `.git`, `AppData` by default. **Defaults done, settings UI is slice 3.** The code root is probed rather than hardcoded, and overlapping roots are folded together — OneDrive redirection makes Documents a child of OneDrive on most machines, and both walked is every file indexed twice
- [ ] Windows Search fallback for locations outside the walked roots — **built in this phase, behind a settings toggle, default off.** Measured: it returns zero rows for `C:\Programming\SELF` on a machine whose whole C: drive is in crawl scope, and its queries run 10–72 ms against a 20 ms budget. On by default it would read as working and hide the gap
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
