# Takyon

A local-first keyboard launcher for Windows — a Spotlight/Raycast alternative built
on Tauri, designed so that a Bangless query never touches the network and the
Palette appears in tens of milliseconds. Bangs (`!e`, `!s`, `!c`) are the only way
anything leaves the machine.

**Status: v0.1 through v0.10 are built.** The Palette is warm, the hotkey works,
and it finds and launches applications, files, clipboard history and
calculations, with Frecency, settings and a `Ctrl+K` action menu. v0.8 adds
**Agents**: `!c` drives Claude Code, Codex or opencode as a subprocess, answers
inline with tools off, and promotes into a Chat Surface on a follow-up. v0.9 adds
**web search**: `!s` retrieves through DuckDuckGo, or Exa when a key is stored,
reads the pages over WinHTTP and has an Agent answer from them, streaming, with
numbered sources. v0.10 is **appearance**: five theme families each carrying a
light and a dark half, Compact and Expanded window modes, and the Windows key as
an optional second binding.

**CI exists but has never run.** `.github/workflows/ci.yml` (typecheck, lint,
every test layer) and `release.yml` (tag → Windows installer → GitHub Release)
are written and their YAML parses, and that is all that can be said: this repo
has no GitHub remote yet, so neither workflow has executed once. Two of the
three CI jobs are on `windows-latest` deliberately — the crate is Windows-only,
and the screenshot baselines were rasterised by Windows.

**Two verification scripts are unrun, and they are the two newest.**
`docs/verify/v0.10.md` section E has never been executed by anyone — the
Windows-key hook is entirely reasoned, and nothing can automate a low-level
keyboard hook. Section A's light-wallpaper steps are also unrun, which matters
because that is the exact case light mode was broken in from v0.6 to v0.9.

**Retrieval is proven, the script is not.** ADR-0021 moved `!s` off Brave, whose
free tier now wants a card, so a real keyless search runs here and the live test
passes. `docs/verify/v0.9.md` still has not been executed by hand — section 7b in
particular. Check `ROADMAP.md` and `docs/tbd/v0.9.md` before assuming anything.

Agents and web search traded phase numbers when v0.8 shipped. Agents was built
first while `!s` kept being deferred, so the shipped work took the next release
number and web search moved down to v0.9. Anything dated before that release
naming "v0.8" means web search; the renumber is why `docs/plans/v0.4-calculator.md`
defers currency to a phase that is now v0.9.

Two things are outstanding rather than done: a real code-signing certificate for
the UIAccess helper (a v1.0 blocker), and v0.2's manual verification pass, whose
Steam steps are blocked because this machine's library holds no game.

**Windows only, and further from macOS than the workspace layout suggests.**
`apps/` and `packages/shared` were split ahead of need so the seams would exist,
and the frontend genuinely is portable — but roughly 7,400 lines across 20 Rust
files name the `windows` crate. `ClipboardStore` and `Hotkey` are now real traits
(ADR-0025) and `identity.rs` knows the macOS data directory, which is the whole
of what exists: no implementation of anything that touches the OS, and no way to
check the `cfg(target_os = "macos")` arms from here, because `cargo check
--target aarch64-apple-darwin` dies in `libsqlite3-sys` for want of a cross `cc`.
The two open questions are settled in `docs/tbc/0013` (the HTTP client) and
`docs/tbc/0014` (signing). `docs/plans/macos.md` states the whole picture.

Distribution is undecided — open source vs proprietary is an open question, so
**avoid GPL dependencies** until it is settled (this already ruled out one option;
see ADR-0005).

## Communication
**Always use the `/homonid` skill in this repo.** Invoke it at the start of every
session, before the first substantive reply, and stay in it — chat prose is terse
and article-free, technical substance unchanged. The skill's own auto-clarity rules
still win for destructive-action confirmations and multi-step sequences.

**Comments and doc-strings follow homonid too.** Drop articles and filler,
fragments are fine, short synonyms over long ones. Technical substance stays
exact: identifiers, API names, error strings and numbers are never abbreviated,
and a comment explaining *why* is still required wherever the reason is not
obvious from the code. Terse is the goal, not silent.

**A comment gets a summary line plus at most three of detail** — four lines of
prose, which is the idiomatic doc-comment shape and not an essay. Ten for a module
doc-string, which orients rather than argues. Delimiters are free: `/**`, `*/` and
blank `*` lines don't count, so a JSDoc block gets the same room as a Rust `///`.

Reasoning that needs more is not a comment. It belongs in `docs/adr/` (a settled
tradeoff), `docs/tbc/` (one we expect to revisit) or `IMPLEMENTATION_PLAN.md`,
with a one-line pointer left at the code. A file where the prose outweighs the
logic is a file nobody reads either half of.

`bun run check:comments` finds every comment over the ceiling. **Not yet part of
`lint`**: v0.1's files predate the rule and still fail it. Fold it into `lint`
once they are brought across, and treat it as blocking from then on.

```rust
// No: six lines of essay for one guard.
// The obvious way to read a shortcut is Load then Resolve then GetPath, and it
// is wrong here in two separate ways. Resolve searches: given a shortcut whose
// target has moved it will hunt the volume for it, and for a UNC target it will
// go to the network and block until the connection times out. Several seconds,
// per dead shortcut, on a walk meant to take a fraction of a second.

// Yes: claim, mechanism, pointer.
// Never Resolve: hunts the volume, blocks on UNC targets, can trigger MSI
// repair. Raw path + exists check instead. Reasoning in the module doc.
```

Still written normally, as prose: commit messages, PR bodies, and everything
under `docs/`.

## Stack (locked — don't substitute without a decision)
- Package manager: **bun** — `bun install`, `bun add`, `bun run <script>`, `bunx`.
  Never npm/npx/yarn/pnpm; translate `npx` in third-party docs to `bunx`.
- Shell: **Tauri 2** (Rust core + WebView2 on Windows).
- Frontend: **React 19 + Vite 7 + TypeScript + Tailwind v4** (`@tailwindcss/vite`),
  **cmdk** for the Palette list, **Radix** primitives. Mirrors tesseract's proven
  desktop setup. No shadcn CLI scaffolding — hand-build the few components needed.
  The reasoning, and what would make us switch, is in `docs/tbc/0001`.
- Icons: **Phosphor** (`@phosphor-icons/react`, MIT) for small semantic icons,
  always at **`duotone`** weight and never `fill` — a bare stroke antialiases to
  mud at 15px on the plate. **Iconoir** (`iconoir-react`, MIT) for larger chrome.
  Two families split by role, never adjacent at the same size (**ADR-0022**).
  Both permissive, which matters while GPL is ruled out. Everything else stays
  hand-authored SVG: `Mark.tsx`, `Select.tsx`, `TitleBar.tsx`.
- Themes: **five families, each carrying a light and a dark half** (**ADR-0023**,
  closing the colour question `docs/brand.md` left open). The registry is
  `src/theme/themes.ts` and is TypeScript only — Rust stores the id without
  interpreting it. **A half states seven roles and nothing else**; every other
  token is a `color-mix(in oklab, …)` over `plate` and `fg` in `styles.css`, so
  adding a theme is seven numbers per half and touches no component.
  **No file under `apps/desktop/src` may name a colour** — the one exception is
  Windows' close-button red in `TitleBar.tsx`, and it is commented as such. That
  rule is not style: white-at-10% borders in `palette/` were invisible on a light
  plate and shipped that way for four phases.
  Values are authored in **oklch** so equal lightness across families is stated
  rather than hoped for. `--color-scrim` is the one role that is neither derived
  nor theme-owned: it must darken in *both* appearances.
- Storage: **SQLite** per concern (settings, clipboard, frecency), WAL mode, in
  `%LOCALAPPDATA%\v3sper\takyon\`; DB key protected by **Windows DPAPI** in
  `creds\`. This mirrors how Raycast for Windows lays out its own data directory.
- **Identity vs display name (ADR-0020, superseding ADR-0011).** Everything
  Windows keys off uses the slug **`com.v3sper.takyon`** — package identity, data
  directory (`%LOCALAPPDATA%\v3sper\takyon\`), registry `Run` value,
  single-instance mutex, UIAccess pipe, updater feed. "Takyon" is the display name
  and lives in UI copy and the installer. The two read alike and are still
  **separate literals**: never derive one from the other, or the next copy change
  silently rewrites the registry. Three things stay spelled `launcher` on purpose
  — both `LEGACY_ENTROPY` constants (DPAPI entropy is an input to decryption, so
  rotating it without the fallback destroys stored clips), `prefs.ts`'s
  `LEGACY_*` localStorage keys, and the NSIS hooks' cleanup deletes.
- File index: **unelevated scoped directory walk + `ReadDirectoryChangesW`
  watchers** into a memory-mapped inverted index (ADR-0007, superseding ADR-0004).
  No service, no elevation, no raw volume access. Behind a `FileIndex` trait, with
  Windows Search as the fallback for locations outside the walked roots.
- **UIAccess**: the Palette needs `uiAccess="true"` to take foreground over
  elevated windows, which requires a signed binary in a trusted location. Code
  signing is a v0.1 requirement, not a shipping-time one, and portable mode is
  impossible as a result.
- External services: **DuckDuckGo** (no key) and **Exa** (keyed) for `!s`
  retrieval only (**ADR-0021**, amending ADR-0005's choice of Brave). Exa is asked
  first when a key is stored and **any failure falls silently through to
  DuckDuckGo** — `!s` is never a dead end, at the cost of a wrong key never
  announcing itself. `ddg.rs` parses HTML, so run
  `cargo test --test web_search -- --ignored` before a release: a class rename
  there breaks `!s` and only that test notices. Plus the user's own **Agent
  CLIs** — `claude`, `codex`, `opencode` — as subprocesses for `!c`. Takyon never
  holds an LLM account or key of its own, and never runs an Agent's login
  (**ADR-0017**; the terminal path is `docs/tbc/0012`). The one key it does hold
  is DPAPI-wrapped in `creds\` and never reaches the webview.
- HTTP: **WinHTTP through the `windows` crate** (ADR-0019), never a Rust client.
  OS TLS, the user's own proxy, and nothing added to the installer. The seam is
  `search::fetch`, which is what a macOS target would reimplement.

## Commands
- dev: `bun run dev`
- check before "done": `bun run typecheck && bun run lint` (lint covers both TS and
  `cargo clippy`)
- test: `bun run test` — **every layer**: Rust unit and integration, TypeScript,
  then Playwright.
  `test:visual` was added to it at v0.3, because a suite that has to be remembered
  separately is one that gets skipped, and it was.
- visual alone: `bun run test:visual` — Playwright screenshots of the UI running in
  the plain Vite dev server, with the Tauri bridge mocked. **Mocked is the point
  and the limit**: it cannot reach ranking, Frecency or anything else in Rust.
- perf harness: `bun run bench` — the four budgets below. Treat a regression here
  as a failing test, not a nice-to-have. Add `--alt-hotkey` where something else
  already owns `Alt+Space`, which is most machines.
- release: `bun run release` — preflight (typecheck, lint, test), `tauri build`,
  then the installer into `releases/v{version}/` with its SHA-256. Same layout as
  tesseract's `releases/`, and `releases/` is gitignored. No `latest.json` or
  `.sig` yet; the updater is a v1.0 item.

## Testing
Use the **`/tdd` skill** for writing and running tests — test-first, not
tests-afterwards. Four layers, because a launcher can't be verified by one:

1. **Rust unit tests** — matching, ranking, Frecency decay, index correctness,
   watcher-overflow handling. All pure logic, no UI, no Tauri.

2. **Rust integration tests** (`src-tauri/tests/`) — everything that calls the
   OS and cannot be reached any other way: the COM walk, icon extraction, the
   `icons.bin` round trip, SQLite on disk, the Recents Source against a Recent
   folder the test writes itself. Plus the IPC contract, driven through
   `tauri::test`'s `MockRuntime` with no window. **Machine-dependent**, so assert
   shape and ordering, never which applications are installed. Anything writing
   to disk uses `common::TempDir`, which cleans up after itself.
3. **Visual regression** — the React UI runs in the ordinary Vite dev server with
   the Tauri IPC layer mocked, driven by Playwright for screenshots. This requires
   an `api.ts` seam: **no component may call `invoke()` directly**, or the UI
   can't run outside Tauri and this layer becomes impossible. (Playwright as a dev
   dependency is unrelated to ADR-0005, which only forbids *shipping* a browser
   engine in the product.)
   **Import `test` and `expect` from `./fixtures`, never from `@playwright/test`**
   — the fixture pins the page clock, and clip rows are offsets from `Date.now()`.
   The screenshot budget is `maxDiffPixels: 150`, absolute: the ratio it replaced
   passed a wrong version number and two missing sidebar rows for two releases.
4. **Manual verification script per phase** in `docs/verify/` — the global hotkey,
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
  Frecency, Stability, Agent, Agent Driver, Sign-in state, Turn, Scratch
  directory, Hit, Citation. Don't reintroduce "result", "provider", "command" — note that
  T3 Code, the reference for v0.8's surface, calls an Agent a *provider*, and
  that word is taken here twice over.
- **Nothing UI-aware in Rust Sources.** Sources return Entries; ranking and
  rendering are separate. This is what keeps the native-Palette escape hatch in
  `docs/tbc/0002` affordable.
- **An outbound request on the Bangless path is a correctness bug** (ADR-0002),
  not a performance issue.
- **Nothing running out of `target\` may register autostart.**
  `#[cfg(not(debug_assertions))]` plus `import.meta.env.DEV` is *not enough* — a
  **release** binary run from `target\release\` passes both, and `bun run bench`
  launches exactly that and then injects thirty keystrokes, one of which will
  answer the first-run dialog for you. `firstrun::should_ask` therefore also
  refuses when `TAKYON_BENCH_LOG` is set and when `current_exe()` sits under
  `target\debug\` or `target\release\`. A stray `Run` key pointing into a build
  directory survives `cargo clean`, deleting the repo, and installing the real
  product. (Learned the hard way in tesseract, then again here.)
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
- `docs/plans/` — one agent-executable build plan per version, plus `post-v1.md`
  for what is deferred past V1. Consumed once; goes stale after the phase ships.
- `docs/verify/` — one manual verification script per phase. Unlike a plan, these
  never go stale: re-run on every regression, and run as one suite at v1.0.
- `docs/tbd/` — what each phase left undone and **which phase owns it**. A gap,
  not a decision: `adr/` says why it is built this way, `tbc/` says which of those
  calls we expect to revisit, `tbd/` says what is not done. See its README.
- `IMPLEMENTATION_PLAN.md` — canonical architecture: trait boundaries, the query
  pipeline, SQLite schemas, the index format, the IPC contract. Amend it; never
  contradict it silently.
- `docs/brand.md` — the locked mark (with path data) and the colour question,
  settled at v0.10 by ADR-0023: five theme families, each carrying a light and a
  dark half.

## Gotchas
- **Never build the release binary with bare `cargo build --release`. Always
  `bun run build`.** A bare cargo build produces a `takyon.exe` that launches,
  registers the hotkey and shows the Palette — with a **completely dead frontend**:
  no first-pixel report, Escape does nothing, the window paints nothing. It fails
  in the one way that looks like a Rust bug. `tauri build` runs
  `beforeBuildCommand` and sets the `TAURI_ENV_*` the asset embedding depends on;
  cargo alone does neither. Cost an hour of chasing a phantom regression once.
- **Never create a window from the main thread.** A synchronous `#[tauri::command]`
  and a tray menu handler both run on the main thread, and
  `WebviewWindowBuilder::build()` dispatches creation to the event loop and blocks
  until it is serviced — on the main thread that is a deadlock. It does not look
  like one: the window *frame* appears, correctly sized and titled, and only its
  webview never loads, so you get a title bar over an opaque white rectangle.
  `build()` never returns, so nothing after it logs either. `settings::open`
  therefore spawns a thread and every future window must do the same. Confirmed by
  driving a v0.6 build; the call path is unchanged since v0.1, so the Settings
  window had probably never rendered in a real build. Nothing caught it because the
  visual suite reaches that route through Vite, never through Tauri —
  `scripts/verify-drive-v0.6.ps1` now samples pixels for exactly this.
- Tesseract is the reference implementation for Tauri patterns here — autostart,
  tray, single-instance, updater, per-platform `tauri.conf.json` splits. Read
  `tesseract/docs/plans/launch-at-startup.md` and its ADR-0026 before rebuilding
  any of that from scratch.
- `Alt+Space` is the default hotkey and collides with PowerToys Run's default and
  the classic window system menu. Rebinding must work from first launch.
- WebView2 is not one process — expect a browser, renderer and GPU process. Any
  memory measurement that reads only the main process is wrong.
