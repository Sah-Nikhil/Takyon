# Takyon

One key for everything on your machine, and everything past it.

Press `Alt+Space` anywhere in Windows. Type. Press Enter. Applications, files,
clipboard history, calculations, Windows settings pages and games all answer in
the same box. The Palette paints in about 20 milliseconds and the first result
lands inside 30. Add one
character to the front of the line and the same box reaches further: `!s` answers
a question from the live web with every source cited, and `!c` hands it to the AI
CLI you are already signed in to.

Windows only, built on Tauri 2. Current release is **0.9.3**.

---

## Contents

- [What it does](#what-it-does)
- [Install](#install)
- [Using it](#using-it)
- [Bangs](#bangs)
- [Connecting your AI](#connecting-your-ai)
- [Connecting Brave Search](#connecting-brave-search)
- [Settings](#settings)
- [How it works](#how-it-works)
- [Performance](#performance)
- [Development](#development)
- [Testing](#testing)
- [Documentation map](#documentation-map)
- [Status and known limits](#status-and-known-limits)

---

## What it does

**Launches things.** Start menu shortcuts, Store and UWP apps, executables on
`PATH`, Steam and Epic games, desktop shortcuts, and the 35 Windows settings
pages plus 198 control panel tasks that normally take three clicks each. Around
1,000 applications are discovered in roughly half a second at login, with their
real icons.

**Learns what you open.** Ranking is Frecency, decayed frequency and recency per
entry, applied over match quality. One or two letters is usually enough after a
few days. The top result freezes 100 milliseconds after you stop typing, so a
slow source cannot swap the thing you were about to press Enter on.

**Finds files.** `!e` searches a scoped index of your own directories. 26,844
entries, 2.5 MB on disk, a worst case query of 568 microseconds, kept true by
`ReadDirectoryChangesW` watchers rather than a rescan.

**Remembers what you copied.** `!v` is searchable clipboard history, filterable
by type, pasted back into wherever you were.

**Does math.** Type an expression and the answer is the first row. No app opens.

**Answers questions.** `!c` asks an agent CLI and streams the answer into the
Palette. A follow up promotes the same window into a chat, without opening a
second window.

**Searches the web.** `!s` asks Brave, reads the pages it gets back, and gives
you a headline plus a few labelled findings, each one ending in the sources
behind it. Every citation is a chip that opens the real page.

---

## Install

Download `Takyon_{version}_x64-setup.exe` from the release you want and run it.
Verify it first if you like: the SHA-256 sits beside it in `SHA256SUMS.txt`.

```powershell
Get-FileHash .\Takyon_0.9.3_x64-setup.exe -Algorithm SHA256
```

The installer is about 2.5 MB. Takyon starts with Windows by default, which you
can turn off in Settings, General. It lives in the tray while it is not on
screen.

Requirements: Windows 11 or 10, and the WebView2 runtime, which recent Windows
already has.

---

## Using it

| Key | What it does |
| --- | --- |
| `Alt+Space` | Show the Palette. Rebindable in Settings, Keyboard |
| Type | Search everything at once |
| `Up` / `Down` | Move through results |
| `Enter` | Open, launch, paste or ask |
| `Ctrl+K` | Action menu for the selected result |
| `Esc` | Go back one step, then dismiss |

The action menu is where the rest of the verbs live: reveal in Explorer, copy
path, run as administrator, and whatever else the selected kind supports. Common
actions also have modifier accelerators, listed inside the menu so you do not
have to learn them from folklore.

The Palette always opens empty, and dismisses when it loses focus.

---

## Bangs

A bang is one character at the start of the line. It has to be the first thing
you type, and it takes the whole rest of the line. `note !v thing` is a search
for "note !v thing", not a clipboard query. Case does not matter, `!V` works.

| Bang | Reaches | Leaves your machine |
| --- | --- | --- |
| *(none)* | Apps, files you recently opened, calculations, settings pages, games | No |
| `!e` | Your file index | No |
| `!v` | Clipboard history | No |
| `!c` | An AI agent CLI you already have | Yes, on Enter |
| `!s` | Brave Search, then the pages it returns | Yes, on Enter |

Examples:

```
chrome                          launch Chrome
2+2*3                           14, in the first row
bluetooth                       the Windows Bluetooth page
!e invoice                      find a file called invoice
!v                              browse everything you have copied
!c when is the next total solar eclipse
!s who won the last f1 race
```

Two things worth knowing about the outbound bangs. Nothing is sent while you
type, only when you press Enter, so there is no debounce because there is no
request to debounce. And the header of an outbound answer turns amber and says
so, which is the only place colour appears in the Palette.

On `!s`, pressing Enter on the query row opens your own browser with the query
instead, using that browser's default search engine, for when you would rather
read the results yourself.

---

## Connecting your AI

Takyon has no AI account, no API key of its own and no proxy. It drives the CLI
you already installed and signed in to, as a subprocess, and reads its output.
Nothing new to pay for.

Supported: **Claude Code** (`claude`), **Codex** (`codex`), **opencode**
(`opencode`).

1. Install and sign in to at least one of them, in its own terminal, the way its
   docs say. Takyon never runs a sign in.
2. Open Settings, Agents. Each installed agent gets a row showing whether it is
   signed in.
3. Turn on the ones you want with the switch, and drag the rows into the order
   you want them asked in.
4. Pick a model and an effort for each. That locked pair is the only one a
   question can use.

`!c` asks the first agent that is switched on. A signed out one is stepped over
rather than becoming a dead end. The first answer runs with tools disabled and in
a scratch directory; tools turn on only if you follow up and promote the question
into a chat.

The same ranking is what `!s` uses to write its answer, so web search works on a
machine that only has Codex installed.

---

## Connecting Brave Search

`!s` needs a Brave Search API key. The free tier covers personal use.

1. Get a key from [brave.com/search/api](https://brave.com/search/api/). Settings,
   Web Search has a button that opens it.
2. Paste it into Settings, Web Search and save.
3. Type `!s` and a question, then Enter.

The key is wrapped with Windows DPAPI for your account and stored in
`%LOCALAPPDATA%\v3sper\takyon\creds\`, not in the settings database, and it is
never handed back to the interface. Settings shows the last four characters so
you can tell which key is stored. "Remove the key" deletes it.

Without a key, `!s` explains what it needs instead of searching.

---

## Settings

Open it from the tray, or search for any control by name in the settings window's
own search box.

| Page | What is there |
| --- | --- |
| General | Start at login, appearance, interface size, turn off animations |
| Launcher | Which monitor the Palette opens on, the tray icon, whether recent files are included |
| Keyboard | The global hotkey |
| Applications | The application walk, its status, and your aliases |
| File Search | Indexed roots, the Windows Search fallback, the owned recents list |
| Clipboard History | How long history is kept, the `!v` bang, excluded applications |
| Calculator | Whether a plain expression answers in the list |
| Agents | Ranked agents, one switch each, sign in state, model and effort |
| Web Search | Your Brave key |
| Advanced | Crash logs, hotkey status, package identity and the data folder |
| About | Version |

An alias makes a name reach an app it would not otherwise match, and it applies
without waiting for the next walk.

---

## How it works

### Shape

```
takyon/
├── apps/desktop/
│   ├── src/                  React 19 + Vite 7 + Tailwind v4
│   │   ├── api.ts            the only file that calls invoke()
│   │   ├── palette/          the Palette, the ask and search views
│   │   ├── settings/         the settings window and its pages
│   │   ├── search/           !s answer parsing and state
│   │   └── components/       the shared controls
│   └── src-tauri/src/
│       ├── lib.rs            builder chain, commands, plugins
│       ├── window.rs         warm window, trim on hide, placement, sizing
│       ├── hotkey.rs         global shortcut, rebinding, conflict reporting
│       ├── tray.rs           tray icon and tooltip
│       ├── bang.rs           the bang parser
│       ├── query.rs          the pipeline
│       ├── rank.rs           matching, Frecency, the stability rule
│       ├── sources/          apps, recents, calculator, games, system
│       ├── index/            walker, watcher, memory mapped store
│       ├── clips/            clipboard history and its encryption
│       ├── agents/           the driver trait and three drivers
│       ├── search/           Brave, WinHTTP fetch, extraction, synthesis
│       └── icons.rs          icon extraction into one mapped blob
└── packages/shared/          TypeScript types mirroring the IPC contract
```

### The query pipeline

Every keystroke goes to Rust and comes back as one response.

1. **Parse the bang.** A leading `!x` selects a mode and takes the rest of the
   line. Anything else is a Bangless query.
2. **Ask the sources.** Each source returns entries, never anything UI aware. An
   entry is an id, a title, an optional second line, a kind, an icon reference and
   the actions it supports. The id is stable across restarts, because it is also
   the Frecency key.
3. **Rank.** Match quality first, then a saturating Frecency lift, then kind
   tiers: apps and settings pages above documents, recents below both, a
   calculation above everything. Sources hand up 64 candidates so a much used
   entry is not cut one step before its lift.
4. **Freeze.** 100 milliseconds after the last keystroke the top entry is fixed
   for that exact query string. Late sources may append below it, never above.
   A new keystroke is a new question and clears it.
5. **Return 12.** The list is not virtualised because it is capped.

### The IPC seam

`src/api.ts` is the only file that calls `invoke`. Every component talks to it
instead. That is what lets the whole interface run in a plain Vite dev server
with the Tauri bridge mocked, which is what the screenshot tests drive. A
contract test compares the Rust response shape against the TypeScript types, so
a rename on one side fails a test rather than a build.

### Why it feels instant

The window is built once at login and then never destroyed. Hiding it trims the
working set across the whole WebView2 process tree, showing it allocates nothing
and creates nothing. Startup defers everything except the hotkey, which is live
within about 50 milliseconds of launch, so the application walk and the index map
happen behind an already usable Palette.

### Where things live

`%LOCALAPPDATA%\v3sper\takyon\`

| Path | Holds |
| --- | --- |
| `settings.db` | Preferences and aliases, SQLite in WAL mode |
| `frecency.db` | Per entry decayed frequency and recency |
| `clips.db` | Clipboard history, with fields encrypted |
| `icons.bin` | Every extracted icon, one memory mapped blob |
| `index/` | The file index |
| `creds/` | DPAPI wrapped keys, one per concern |

The directory, the registry `Run` value, the single instance mutex and the
package identity all key off the slug `com.v3sper.takyon`. It is a literal, not
the display name lowercased, so a change to UI copy never reaches the registry.

### `!c`, end to end

Enter starts a Turn. Takyon spawns the ranked agent's CLI with the locked model
and effort, tools off, in a scratch directory, and renders its streaming JSON as
it arrives. A missing or signed out agent is explained in the row rather than
failing quietly. A follow up promotes the same window into a chat surface, which
is where tools turn on. Escape goes back a step rather than destroying the
conversation.

### `!s`, end to end

Enter sends the query to Brave. The hits come back, and their pages are fetched
in parallel with a 12 second deadline, a 6 second per request timeout and a 512
KB body cap. Each page is reduced to its prose, the lot is packed into a 24,000
character prompt with every source numbered, and the agent `!c` would have asked
writes the answer against it. The Palette names the hosts while it reads, then
renders the headline, the labelled findings and the numbered sources.

HTTP goes through **WinHTTP**, the stack Windows already ships, rather than a
Rust client. That is TLS, the certificate store and your proxy settings for zero
added binary size, on a product where a Rust HTTPS client would have added
roughly 2 MB to 2.5 MB.

### Rules the code holds itself to

- A line without a bang never touches the network. Not a suggestion request, not
  telemetry, not a prefetch. Breaking this is a correctness bug, not a
  performance one.
- No browser engine is bundled or driven for web search.
- Takyon never authenticates an agent, and never holds an LLM account.
- Nothing running out of a build directory may register autostart.
- Sources know nothing about the interface. Ranking and rendering are separate.

Each of those, and the tradeoff behind it, is written up in `docs/adr/`.

---

## Performance

Measured on a release build by `bun run bench`, against the budgets the product
is defined by.

| Metric | Budget | Measured |
| --- | --- | --- |
| Hotkey to first pixel | < 50 ms | **22.6 ms** p95 |
| First show after 35 minutes idle | < 50 ms | **22.8 ms** |
| Login to hotkey responsive | < 500 ms | **311.6 ms** |
| Idle memory, warm and trimmed | < 150 MB | **~107 MB** |
| Installer size | none | **2.5 MB** |

A regression here is treated as a failing test.

---

## Development

Package manager is **bun**. Never npm, npx, yarn or pnpm.

```bash
bun install
bun run dev          # Tauri dev, hot reloading frontend
bun run build        # release build, always this, never bare cargo build
bun run typecheck
bun run lint         # TypeScript and cargo clippy
bun run test         # every layer
bun run bench        # the four budgets above
bun run release      # preflight, build, installer plus SHA-256 into releases/
```

Prerequisites: bun, a Rust toolchain, and the Tauri prerequisites for Windows.

Two traps worth knowing before you touch the Rust:

**Never build with bare `cargo build --release`.** You get a binary that
launches, registers the hotkey and shows a completely dead frontend. `tauri
build` runs the frontend build and sets the environment the asset embedding
depends on. `cargo` alone does neither.

**Never create a window from the main thread.** A synchronous command and a tray
handler both run there, and window creation dispatches to the event loop and
blocks until serviced, which is a deadlock that looks like a rendering bug: the
frame appears, correctly sized and titled, and the webview never loads. Spawn a
thread.

---

## Testing

Four layers, because a launcher cannot be checked by one.

| Layer | What it covers | Command |
| --- | --- | --- |
| Rust unit | Matching, ranking, Frecency, parsing, index correctness | `bun run test:rust` |
| Rust integration | The COM walk, icon extraction, SQLite on disk, the IPC contract | `bun run test:rust` |
| Visual regression | The React interface in a plain Vite server with the bridge mocked, driven by Playwright | `bun run test:visual` |
| Manual scripts | Global hotkey, focus loss, tray, multi monitor, elevated windows | `docs/verify/` |

Current counts: 565 Rust unit, 53 Rust integration across 7 binaries, 60
TypeScript, 108 Playwright screenshots.

The screenshot budget is a flat 150 pixels, not a percentage. The percentage it
replaced allowed 5,702 pixels on this window, which was enough to pass a wrong
version number and two missing rows for two releases.

---

## Documentation map

| Path | What it is |
| --- | --- |
| `CONTEXT.md` | The glossary. Domain language, no implementation |
| `ROADMAP.md` | Phased checkboxes with exit criteria. The tracker |
| `IMPLEMENTATION_PLAN.md` | Canonical architecture: traits, pipeline, schemas, IPC |
| `CHANGELOG.md` | Releases, newest first |
| `docs/adr/` | Settled tradeoffs, numbered |
| `docs/tbc/` | Decisions we expect to revisit, with switching costs |
| `docs/tbd/` | What each phase left undone, and which phase owns it |
| `docs/plans/` | One build plan per version |
| `docs/verify/` | One manual verification script per phase |
| `docs/prior-art/` | What Raycast, PowerToys Run and others actually did |
| `docs/brand.md` | The mark, with path data |

---

## Status and known limits

Nine phases are built and shipped: the warm shell, applications, ranking,
calculator, clipboard history, settings, file search, agents, web search.

- **The Palette will not appear over elevated windows.** That needs a signed
  UIAccess helper. The helper, its manifest and the protocol all exist and work
  against a self signed certificate. A real certificate is the remaining v1.0
  blocker.
- **No updater.** Releases carry no `latest.json` or signature yet.
- **Windows only.** The workspace has macOS seams in place, but no macOS target.
- **No telemetry**, and none planned before v1.

---

## Licence

Undecided. Open source against proprietary is still an open question, which is
why the project avoids GPL dependencies.
