# Takyon

A local-first keyboard launcher for Windows — a Spotlight/Raycast alternative built
on Tauri, designed so that a Bangless query never touches the network and the
Palette appears in tens of milliseconds. Bangs (`!e`, `!s`, `!c`) are the only way
anything leaves the machine.

**Status: design complete, no code yet.** There is no `apps/` directory, no Rust,
no tests, no CI. What exists is `CONTEXT.md`, `ROADMAP.md`,
`IMPLEMENTATION_PLAN.md`, `docs/adr/`, `docs/tbc/`, `docs/plans/` and
`docs/brand.md`. Do not assume any implementation exists.

Distribution is undecided — open source vs proprietary is an open question, so
**avoid GPL dependencies** until it is settled (this already ruled out one option;
see ADR-0005).

## Communication
**Always use the `/homonid` skill in this repo.** Invoke it at the start of every
session, before the first substantive reply, and stay in it — chat prose is terse
and article-free, technical substance unchanged. It does not apply to written
artifacts: code, comments, doc-strings, commit messages, PR bodies and everything
under `docs/` are written normally. The skill's own auto-clarity rules still win
for destructive-action confirmations and multi-step sequences.

## Stack (locked — don't substitute without a decision)
- Package manager: **bun** — `bun install`, `bun add`, `bun run <script>`, `bunx`.
  Never npm/npx/yarn/pnpm; translate `npx` in third-party docs to `bunx`.
- Shell: **Tauri 2** (Rust core + WebView2 on Windows).
- Frontend: **React 19 + Vite 7 + TypeScript + Tailwind v4** (`@tailwindcss/vite`),
  **cmdk** for the Palette list, **Radix** primitives. Mirrors tesseract's proven
  desktop setup. No shadcn CLI scaffolding — hand-build the few components needed.
  The reasoning, and what would make us switch, is in `docs/tbc/0001`.
- Storage: **SQLite** per concern (settings, clipboard, frecency), WAL mode, in
  `%LOCALAPPDATA%\v3sper\launcher\`; DB key protected by **Windows DPAPI** in
  `creds\`. This mirrors how Raycast for Windows lays out its own data directory.
- **Identity vs display name (ADR-0011).** "Takyon" is the display name and lives
  only in UI copy and the installer. Everything Windows keys off uses the fixed
  slug **`com.v3sper.launcher`** — package identity, data directory,
  registry `Run` value, single-instance mutex, updater feed. Never derive any of
  those from the display name; renaming the product must stay a copy change.
- File index: **unelevated scoped directory walk + `ReadDirectoryChangesW`
  watchers** into a memory-mapped inverted index (ADR-0007, superseding ADR-0004).
  No service, no elevation, no raw volume access. Behind a `FileIndex` trait, with
  Windows Search as the fallback for locations outside the walked roots.
- **UIAccess**: the Palette needs `uiAccess="true"` to take foreground over
  elevated windows, which requires a signed binary in a trusted location. Code
  signing is a v0.1 requirement, not a shipping-time one, and portable mode is
  impossible as a result.
- External services: **Brave Search API** for `!s` retrieval only (ADR-0005), and
  the user's own **Claude Code CLI** as a subprocess for `!c` — Takyon never
  holds an LLM account or key of its own.

## Commands
- dev: `bun run dev`
- check before "done": `bun run typecheck && bun run lint` (lint covers both TS and
  `cargo clippy`)
- test: `bun run test`
- visual: `bun run test:visual` — Playwright screenshots of the UI running in the
  plain Vite dev server, with the Tauri bridge mocked
- perf harness: `bun run bench` — the four budgets below. Treat a regression here
  as a failing test, not a nice-to-have.

## Testing
Use the **`/tdd` skill** for writing and running tests — test-first, not
tests-afterwards. Three layers, because a launcher can't be verified by one:

1. **Rust unit tests** — matching, ranking, Frecency decay, index correctness,
   watcher-overflow handling. All pure logic, no UI, no Tauri.
2. **Visual regression** — the React UI runs in the ordinary Vite dev server with
   the Tauri IPC layer mocked, driven by Playwright for screenshots. This requires
   an `api.ts` seam: **no component may call `invoke()` directly**, or the UI
   can't run outside Tauri and this layer becomes impossible. (Playwright as a dev
   dependency is unrelated to ADR-0005, which only forbids *shipping* a browser
   engine in the product.)
3. **Manual verification script per phase**, tesseract-style — the global hotkey,
   focus-loss dismissal, tray, multi-monitor placement and the UIAccess
   elevated-window overlay genuinely cannot be automated cheaply. Write the script
   as part of the phase, don't improvise it at the end.

A debug-only flag must let the Palette be shown **without stealing focus**, or
dismiss-on-focus-loss destroys the window every time you try to inspect it.

## Performance budgets (these are the product)
| Metric | Budget |
|---|---|
| Hotkey → first pixel | < 50 ms |
| Hotkey → first Entry for a Bangless query | < 30 ms |
| Idle RSS (warm, trimmed) | < 150 MB |
| Login → hotkey responsive | < 500 ms |

## Structure
- `apps/desktop/src/` — React UI (Palette, Chat Surface, Settings window)
- `apps/desktop/src-tauri/` — Rust core: Sources, ranker, indexes, Bang dispatch
- `packages/shared/` — TypeScript types shared with the future macOS/mobile targets
- `brand/` — the locked mark in one file, plus the script that generates every
  icon from it. Run `bun run --cwd brand build` after any change; never hand-edit
  a generated icon, and never let `tauri init`/`tauri icon` touch
  `src-tauri/icons/`. Surface map in `brand/README.md`.

The workspace layout exists ahead of need so the macOS seams are in place. The
seams that actually matter are Rust traits — `FileIndex`, `AppSource`,
`ClipboardStore`, `Hotkey`, `SearchProvider` — not directory structure.

## Conventions
- **Use the vocabulary in `CONTEXT.md` exactly**, in identifiers, comments and UI
  copy: Palette, Bang, Bangless, Mode, Promotion, Chat Surface, Entry, Source,
  Frecency, Stability. Don't reintroduce "result", "provider", "command".
- **Nothing UI-aware in Rust Sources.** Sources return Entries; ranking and
  rendering are separate. This is what keeps the native-Palette escape hatch in
  `docs/tbc/0002` affordable.
- **An outbound request on the Bangless path is a correctness bug** (ADR-0002),
  not a performance issue.
- **Dev builds must never register autostart.** Gate the Rust side with
  `#[cfg(not(debug_assertions))]` and the UI switch with `import.meta.env.DEV`.
  A debug registration writes a `Run` key pointing at `target\debug\` that
  survives uninstall of the real app. (Learned the hard way in tesseract.)
- **Autostart state lives in the OS, not in our config** — read it via
  `tauri-plugin-autostart`'s `isEnabled()` on mount, never mirror it into SQLite.
- Use `tauri-plugin-autostart` and `tauri-plugin-single-instance` (autostart is
  what makes single-instance necessary). Tauri capabilities must list
  `autostart:default` or the frontend calls fail at *runtime*, not build time.

## Git

This section overrides the global "never commit or push unless I explicitly ask".
Here the agent never commits at all.

**Commit messages are generated in chat, not applied.** Write the message into the
reply and stop. Committing is a manual step, always.

**Forbidden outright:** `git commit`, `git push` in any form (including
`git push -u origin <branch>` for a brand-new branch), `git merge`, `git rebase`,
and anything that rewrites history. Staging and inspection are fine.

**Format.** `<VERB> d<phase>.<n> <subject>` — one line, extremely short, no body
unless something genuinely needs explaining.

| Verb | Use when |
|---|---|
| `NEW` | the thing did not exist before |
| `FIX` | something was broken and now is not |
| `UPDATE` | something already worked and changed |

```
NEW d0.1.1 brand asset pipeline
FIX d0.1.2 mark particle bound to --accent, a surface token
UPDATE d0.1.3 v0.1 plan warns tauri init clobbers icons
```

**The dev version is not the release version.** `d0.1.3` is the third dev change
inside the v0.1 phase; it resets to `.1` when the next phase opens. Release
versions are the `docs/plans/` phases and only move when a phase ships. The `d`
prefix exists so the two can never be confused at a glance.

**Find the current number with `git log -1`.** Committed history is the record —
read the last `d<phase>.<n>`, increment it. Never guess; if history has no dev
number yet, the phase starts at `.1`.

**Worktrees: create freely.** No approval needed. Removing one is a deletion and
follows the usual rule — ask first.

**Branches: ask before creating.** The agent may run `git switch -c`, but only
after approval in that session. Approval to create one branch is not approval for
the next.

## Docs & context
- `CONTEXT.md` — the glossary. Pure domain language, no implementation detail.
- `ROADMAP.md` — phased checkboxes with exit criteria. The tracker.
- `docs/adr/` — settled tradeoffs. Don't re-derive these; if there's a gap, flag it.
- `docs/tbc/` — decisions we expect to revisit: the assumption under each, the
  trigger that would disprove it, and what switching costs. See `docs/tbc/README.md`.
- `docs/plans/` — one agent-executable plan per version, plus `post-v1.md` for
  everything deliberately deferred.
- `IMPLEMENTATION_PLAN.md` — canonical architecture: trait boundaries, the query
  pipeline, SQLite schemas, the index format, the IPC contract. Amend it; never
  contradict it silently.
- `docs/brand.md` — the locked mark (with path data) and the colour question,
  which is deliberately still open until v0.6.

## Gotchas
- Tesseract is the reference implementation for Tauri patterns here — autostart,
  tray, single-instance, updater, per-platform `tauri.conf.json` splits. Read
  `tesseract/docs/plans/launch-at-startup.md` and its ADR-0026 before rebuilding
  any of that from scratch.
- `Alt+Space` is the default hotkey and collides with PowerToys Run's default and
  the classic window system menu. Rebinding must work from first launch.
- WebView2 is not one process — expect a browser, renderer and GPU process. Any
  memory measurement that reads only the main process is wrong.
