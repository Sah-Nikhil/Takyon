# Changelog

Releases of Takyon, newest first. The installer and its SHA-256 live in
`releases/v{version}/`, which is gitignored — this file is the record that
survives.

Phase numbers in `ROADMAP.md` and release versions are the same number. The one
exception is recorded under v0.8 below.

## v0.9 — Web search

`!s` answers a question in the Palette instead of opening a browser tab. It is
the first thing in Takyon that reaches the internet, and it only ever does so
after you press Enter.

### Searching

- **`!s` asks the web.** Brave Search returns the pages, Takyon reads them over
  the HTTP stack Windows already ships, reduces each to its prose, and hands that
  text to an Agent to answer from. No browser engine is bundled or driven
  (ADR-0005), and no Rust HTTP client either (ADR-0019).
- **The answer streams, with numbered sources.** Citations like `[1]` match the
  list underneath, and selecting a source opens it in your browser.
- **The summariser is the Agent you already ranked.** Whichever Agent `!c` would
  ask writes the answer, with tools off, in the Scratch directory — so a machine
  with only Codex installed still has `!s`.
- **Enter opens your browser** with the query, using that browser's own default
  search engine, when you would rather read the results yourself.

### The line between contained and outbound

- **Typing sends nothing.** There is no debounce because there is no request to
  debounce: `!s ferrari` fires on Enter, never on a keystroke. A line without a
  Bang still never leaves the machine (ADR-0002).
- **The surface says so.** The `!s` row and the answer header are warm and read
  "Left this machine", which is the one place in the Palette that colour appears.
- **Your key stays yours.** The Brave key is wrapped with DPAPI for your Windows
  account, stored beside the clipboard key rather than in `settings.db`, and
  never sent back to the interface — Settings shows its last four characters.

### Known limits

- **No real search has run on this machine.** Every layer below the network is
  tested, and the live-network tests fetch real pages, but no Brave key is stored
  here so the phase's own exit criterion is unproven. `docs/tbd/v0.9.md` §1.
- Citations are numbers rather than links, cancelling does not stop a request
  already in flight, and a page that draws itself in JavaScript contributes only
  its provider snippet. Each is written down with the phase that owns it.

## v0.8 — AI

Takyon can answer a question. `!c` drives a coding-agent CLI you already have
installed and signed in to, and the answer streams into the Palette.

### Agents

- **`!c` asks an Agent.** Claude Code, Codex and opencode, each behind an
  `AgentDriver`. Takyon runs them as subprocesses and holds no account, key or
  token of its own — signing in stays in the Agent's own CLI (ADR-0017).
- **The answer arrives in place.** The Palette becomes the conversation; a
  follow-up continues in the same window rather than opening a second one, and
  Escape steps back rather than destroying what is on screen.
- **Tools are off on the first answer.** The Turn you get by reflex, one
  keystroke from the global hotkey, cannot write anything, and runs in an empty
  Scratch directory. Follow-ups are an explicit act and carry tools.
- **The model and effort are locked in Settings.** Whatever you pick per Agent is
  the only pair a Turn can use; the frontend never sends either value.

### Choosing between Agents

- **Settings ranks every Agent**, first to last, with a switch each. `!c` asks
  the first one switched on and works down the list, so a signed-out Agent is
  stepped over rather than being a dead end.
- **`!c` never waits on a probe** (ADR-0018). The order and the switches are
  stored preferences, so the Palette names its Agent on the first keystroke.
  Reading Sign-in state costs three process spawns and used to block Enter until
  they finished — on a cold machine that read as a broken launcher.

### Known limits

- Permission prompts are refused, so a follow-up asked to edit a file gets as far
  as the permission check. The approve/deny UI is v1.0 — `docs/tbd/v0.8.md` §1.
- The conversation lives in the Palette, so dismissing the window ends it.
- The UIAccess helper is still unsigned: the Palette will not appear over
  elevated windows. See `docs/plans/uiaccess-signing.md`.
- No updater. `tauri-plugin-updater` is a v1.0 item, so there is no
  `latest.json` and no signature beside the installer.

### About the version number

Agents and web search traded phase numbers with this release. Web search (`!s`)
was v0.8 and kept being deferred while Agents was built, so the work that
actually shipped took the next release number and web search moved to v0.9.
Anything written before this release that names "v0.8" means web search.

## v0.7.0 and earlier

Not written up. `ROADMAP.md` carries the phase-by-phase record: the warm shell,
launching applications, ranking with Frecency, the calculator, clipboard history,
Settings, and file search.
