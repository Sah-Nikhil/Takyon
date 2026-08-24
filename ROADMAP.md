# Takyon Build Plan

A phased, dependency-ordered roadmap. Each phase ends with something you can
actually use — no phase leaves you holding half-wired plumbing. Check items off as
you go; this file is the tracker.

Domain terms are defined in [`CONTEXT.md`](./CONTEXT.md). Settled tradeoffs live in
[`docs/adr/`](./docs/adr/); decisions we expect to revisit, with switching costs,
live in [`docs/tbc/`](./docs/tbc/); deferred work is in
[`docs/plans/post-v1.md`](./docs/plans/post-v1.md).

**Nothing below is built yet.** Every box is unchecked because there is no code.

---

## v0.1 — The warm shell

**Goal:** an empty Palette that appears instantly and proves the core bet.

- [x] Bun workspace: `apps/desktop` (Tauri 2 + React 19 + Vite 7 + Tailwind v4) and `packages/shared`
- [x] Global hotkey (`Alt+Space`) via `tauri-plugin-global-shortcut`; a failed registration is reported in a dialog and in the Palette, never swallowed. **Rebinding is v0.6** — it needs the settings UI
- [ ] **UIAccess helper + code signing** — helper crate, `uiAccess="true"` manifest, named-pipe protocol and `scripts/dev-sign-uiaccess.ps1` all exist and work against a self-signed certificate. **A real certificate is still outstanding and is a v1.0 blocker** — see `docs/plans/uiaccess-signing.md`
- [x] One Palette window created at startup, hidden — never destroyed (ADR-0003)
- [x] Working-set trim on hide; show path does no allocation or window creation. The trim walks the **whole process tree**, because essentially all the resident memory is in WebView2's descendants rather than in the Rust host
- [x] Dismiss on Escape and on focus loss; always opens empty
- [x] Tray icon: settings, quit — with both glyph polarities and a runtime swap when the system theme changes
- [x] Autostart via `tauri-plugin-autostart` + `tauri-plugin-single-instance`, on by default via first-run prompt, **never registered in dev builds**. The `Run` value is named `com.v3sper.launcher`, not "Takyon" (ADR-0011)
- [ ] `bun run bench` — harness built; release numbers measured and written into `docs/tbc/0002` (first pixel p95 **22.6 ms** / 50, start-to-hotkey **311.6 ms** / 500, idle RSS **27.5 MB** / 150 across 7 processes). **The 30+ minute idle run is still outstanding**, and it is the one that decides ADR-0003
- [x] Deferred init: hotkey live within ~50 ms of launch; everything else after
- [x] The idle beat: the mark animates while the Palette is open and empty, stops on the first keystroke and while hidden. **Settings → Turn off animations** kills it, as does Windows' own reduce-motion setting. Spec in `docs/brand.md`; storage is `localStorage` behind `src/prefs.ts` until `settings.db` exists

**Exit criteria:** you press `Alt+Space` anywhere in Windows, an empty Palette
appears in under 50 ms, Escape dismisses it, and the benchmark numbers are written
into `docs/tbc/0002` as the first real evidence for or against the warm model.

---

## v0.2 — Launching applications

**Goal:** it replaces the Start menu for opening things.

- [ ] `AppSource`: Start Menu `.lnk` (user + machine), `shell:AppsFolder` (UWP/Store), bare `.exe` on PATH, Steam games
- [ ] Matching: word-boundary prefix + executable basename + acronym (`vsc` → Visual Studio Code)
- [ ] Icon extraction into a single memory-mapped `icons.bin`, lazy fill, off the UI thread, placeholder while missing
- [ ] Launch on Enter; Palette dismisses immediately, not after the app appears
- [ ] **`Ctrl+K` action menu** as a shared primitive — every Source and Mode contributes actions to it (open, reveal in Explorer, copy path, run as administrator, open with…)
- [ ] Modifier accelerators for the common actions, **user-rebindable**, and listed inside the action menu so they're discoverable rather than folklore
- [ ] Result list virtualised, keyboard-only navigation

**Exit criteria:** you use it instead of the Start menu for a full day and don't
reach for the Start menu once.

---

## v0.3 — Ranking that learns

**Goal:** it starts guessing right before you finish typing.

- [ ] `frecency.db` — per-Entry decayed frequency + recency, updated on every launch
- [ ] Ranking: Frecency over raw match quality; Apps always sort above documents
- [ ] **Stability rule**: the top Entry freezes ~100 ms after the last keystroke; late Sources may only append below
- [ ] User-defined aliases (settings-editable) resolved before matching
- [ ] Recently-opened files as a cheap Bangless Source (shell recent items, no index), always below apps

**Exit criteria:** after a week of use, your ten most-used applications are all
reachable in one or two keystrokes.

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

- [ ] NSIS installer, code signing, `tauri-plugin-updater`
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
