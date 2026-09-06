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
- **The answer is a briefing, not an essay.** Arc Search's shape: while it works
  the Palette names the pages it is reading, by host. Then a headline and a few
  labelled findings, each ending in the sources behind it, and a line of its own
  wherever the sources disagree. Every citation is a chip that opens its page.
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

### 0.9.3

The identity rename, under a version number that means one thing.

0.9.2 was built twice. The first build carried a data migration that refused to
run whenever the destination directory already existed, and an empty one always
does — anything that resolves a path through `data_dir()` creates it. On a real
upgrade that abandoned the clipboard history in place and started the app on a
fresh directory, with no error anywhere. It was caught by driving the installed
build rather than by the suite, which is the reason the manual pass exists.

The fix moves the directory entry by entry, keeps whatever is already at the
destination, and removes the old directory only once it has emptied. Everything
else in this release is the 0.9.2 entry below, which describes the same code.

### 0.9.2

The app's Windows identity now matches its name, and Defender's opinion of the
installed binary is written down.

- **The identity slug is `com.v3sper.takyon`** (ADR-0020, superseding ADR-0011).
  Data moves from `%LOCALAPPDATA%3sper\launcher\` to `...3sper	akyon\`,
  the `Run` value and the UIAccess pipe are renamed with it, and the registry and
  the data directory now name the app you are looking at. ADR-0011 chose a neutral
  slug as insurance against a third rename after "Taskmaster" and "Praxis" were
  dropped; the name has settled, and the cheapest moment to fold it into the
  identity is before anything is signed or distributed.
- **Your clipboard history survives the move.** The directory is renamed in place
  on first start, and the DPAPI-wrapped clipboard key is unwrapped under the old
  entropy and rewrapped under the new one as it is read. The installer also
  deletes the pre-rename `Run` value on upgrade, so no orphan is left pointing at
  a binary that has moved.
- **Defender quarantines Takyon on a stock Windows 11 machine**, as
  `Trojan:Win32/Bearfoos.A!ml`. It is a false positive and it is not fixed here.
  It reads as an installer failure: the install completes and the binary is
  deleted seconds afterwards. Both installers and the binary scan clean on
  demand, so what fired is the behaviour monitor watching the process run, not a
  signature match. `docs/tbd/v0.9.md` §11 has the detection record, the
  comparison with tesseract, and the workaround.

### 0.9.1

Fixes found by driving the installed 0.9.0 build.

- **`!s` no longer draws two status rows.** `indexing` in the query response
  meant both "the application walk is running" and "reserve a row", so the Bang
  reserved one and inherited the other. Two rows in a window sized for one made
  the list scroll, and the scrollbar covered the message.
- **The application walk reports where it belongs.** The tray tooltip says
  "indexing applications…" until it lands, and Settings → Applications asks
  whether it is running rather than inferring it from an empty list — a walk that
  finished and found nothing now says so.
- **Answers render their markdown.** `**bold**` arrived with its asterisks.
- **The dropdowns are ours.** A native `<select>` popup is drawn by the OS, in a
  light grey that belongs to no part of this app. One hand-built listbox now
  serves both windows, with arrows, Home, End, typeahead, Escape and a flip when
  there is no room below.
- **The hotkey chips read as one control**, and the chosen chord is lifted with
  a ring rather than tinted ten percent.
- **A switched-off Agent cannot be reordered**, since `!c` steps over it.

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

### 0.9.1

Fixes found by driving the installed 0.9.0 build.

- **`!s` no longer draws two status rows.** `indexing` in the query response
  meant both "the application walk is running" and "reserve a row", so the Bang
  reserved one and inherited the other. Two rows in a window sized for one made
  the list scroll, and the scrollbar covered the message.
- **The application walk reports where it belongs.** The tray tooltip says
  "indexing applications…" until it lands, and Settings → Applications asks
  whether it is running rather than inferring it from an empty list — a walk that
  finished and found nothing now says so.
- **Answers render their markdown.** `**bold**` arrived with its asterisks.
- **The dropdowns are ours.** A native `<select>` popup is drawn by the OS, in a
  light grey that belongs to no part of this app. One hand-built listbox now
  serves both windows, with arrows, Home, End, typeahead, Escape and a flip when
  there is no room below.
- **The hotkey chips read as one control**, and the chosen chord is lifted with
  a ring rather than tinted ten percent.
- **A switched-off Agent cannot be reordered**, since `!c` steps over it.

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
