# Changelog

Releases of Takyon, newest first. The installer and its SHA-256 live in
`releases/v{version}/`, which is gitignored — this file is the record that
survives.

Phase numbers in `ROADMAP.md` and release versions are the same number. The one
exception is recorded under v0.8 below.

## v0.10.1 — What driving the release found

Four defects in v0.10.0, every one of them found by using the installed build
rather than by the test suite. Three were outside what the visual layer can see:
it runs the interface in an ordinary browser tab, and none of a transparent
window, a native window border, or an HTTP cache header exists there.

- **The Palette no longer ghosts the window behind it.** The panel was 95%
  opaque with a backdrop blur that could never work — the window is transparent,
  so there is no backdrop to blur — and the remaining 5% let whatever was
  underneath read through as legible text. Opaque now.
- **The stray rectangle around the Palette is gone.** Windows was drawing its own
  shadow and border on a window that already draws both itself, and because the
  panel insets 8px for its shadow, that border landed as a second outline with a
  gap. Takyon's own shadow was doing the work all along.
- **The hotkey dropdown stopped hiding its last options.** The list was drawn
  inside a card that clips its contents, so anything past the card's edge was cut
  off with nothing to say so. It now escapes the card entirely.
- **Favicons appear.** Sources go on screen before their icons have been fetched,
  and a row that asked too early never asked again — so the letter tile was
  permanent for any site you had not already visited. The rows are now told when
  the icons land, and a miss is no longer cached for a day.

Also: **CI and release workflows** for GitHub, in the same shape as tesseract's.
Typecheck, lint and all four test layers on every push; a tag builds the
installer and publishes it with its SHA-256. Neither has ever run — this repo has
no remote yet. A macOS job is wired in and switched off, because there is no
macOS build to make: `docs/plans/macos.md` says what that would take.

## v0.10 — Appearance

Takyon gets a look you choose, and a second shape. The light theme in particular
is new in the only sense that matters: it did not work before.

### Themes

- **Five themes, and each one is a pair.** Graphite, Vela, Cherenkov, Aurora and
  Halide. A theme carries a light half *and* a dark half, so Dark theme and Light
  theme are two independent choices over the same list and Follow system
  appearance decides which is live. Pick a dark theme at noon; it is there at
  midnight.
- **Light mode was broken and now is not.** The Settings window was built from
  tokens at v0.6; the Palette never was. Its panel edge, its selected row and its
  keycaps were all white at 10% opacity — a hairline on a near-black plate and
  *nothing at all* on a near-white one. Light mode has shipped since v0.6 with an
  invisible border and an invisible selection. No file in the interface names a
  colour any more.
- **The colour question is closed** (ADR-0023). `docs/brand.md` deliberately left
  it open "until v0.6" and it stayed open two phases longer. A theme is seven
  values per half and everything else is derived from two of them, so adding one
  touches no component.
- **Cherenkov is no longer the default.** It is still there, and it is still the
  mark's own hue, but a launcher opens over whatever wallpaper you have and only
  a neutral plate never argues with it.
- **Warnings and outbound states are different colours now.** Both were the same
  amber through v0.9, so "this left your machine" and "that write was refused"
  looked identical and no theme could move either.
- **Secondary text got readable.** Descriptions, subtitles and placeholders were
  under 3:1 contrast on a light plate. Nothing text-bearing sits below 46% now.

### Two window modes

- **Compact** is what you already had: one line that grows a row at a time.
- **Expanded** opens at a fixed height and stays there — no resizing as you type.
  It has room for two things Compact cannot afford: **results grouped by kind**
  under headings, and a **first view** of what you open most, so an empty Palette
  is a starting point rather than a blank.
- Chosen from a pair of drawn previews in Settings, in your live theme. Compact
  remains the default.

### The Windows key

- **Tap the Windows key to open Takyon**, if you want it — off by default. It
  cannot be a normal shortcut: the Windows key is a modifier, and the Start menu
  opens when you *release* it. Takyon slips an unused key in behind your press so
  the tap stops looking like a tap.
- **Holding it is untouched.** `Win+R`, `Win+E`, `Win+L` and the rest all work,
  because the press is never swallowed — only the meaning of a bare tap changes.
- Off by default for three honest reasons, in ADR-0024. The short one: it needs a
  hook on every keystroke in the system, and replacing the Start menu is a large
  thing to do to a machine without being asked.
- The shortcut list became a dropdown when the switch landed above it.

### File Search

- Rebuilt to read as a list: search scopes and ignore patterns each get rows you
  can remove, an add field, and an empty state that names the consequence rather
  than the absence.
- **Reset to defaults** forgets your choices instead of writing today's answer
  back — so a machine that gains a code directory next month still picks it up.

### Faster than v0.9, not slower

Re-reading preferences on every summon repainted the whole document in the colour
it was already painted. Fixed, and the result is better than before the phase
started: **hotkey to first pixel 25.9 ms** at p95 (from 30.4 at v0.7), first Entry
19.3 ms, login to responsive 260 ms. All four budgets pass in both window modes.

**Not verified by hand:** the Windows-key binding. Every claim about it is
reasoned rather than observed — a low-level keyboard hook cannot be driven by a
test — so it ships off by default and `docs/verify/v0.10.md` section E is still
open. It also cannot reach a window running as administrator, the same limitation
the ordinary shortcut has, for the same unsigned-helper reason.

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

### 0.9.5

`!s` answers the way Arc Search does.

- **An answer looks like an answer.** An accent headline, findings that each
  carry an icon naming their kind and a label that opens the source it came from,
  optional `##` sections, and a strip of source cards under the first group. The
  numbered list is still at the bottom, at reference weight rather than
  outweighing the answer above it.
- **Names in the prose open their source.** `[Lando Norris](1)` is a link, not
  punctuation. The target is always a source number and never a URL, so a model
  that invents an address cannot put one on screen.
- **Real favicons**, fetched from the hosts themselves — the icon a page declares,
  falling back to `/favicon.ico`. One extra request per host, only to hosts the
  search already contacted, cached on disk by host. **Not** a favicon service:
  that would hand one company the list of every host you read. Sites that serve
  nothing get a letter tile.
- **The view no longer follows the stream.** The headline is the answer's title,
  and following the tail scrolled it off the top before it could be read.

Two icon sets arrive with this: Phosphor at duotone weight for the finding
gutter, Iconoir for larger chrome. Both MIT. ADR-0022 has the reasoning,
including why `og:image` thumbnails were considered and left out.

### 0.9.4

`!s` no longer needs anyone to sign up for anything.

- **DuckDuckGo answers by default.** No key, no account, no card. A fresh install
  can search the web the moment it is installed. `html.duckduckgo.com` renders
  without JavaScript, so results come back over the same WinHTTP stack the page
  reads already use and no browser engine is involved (ADR-0005 still stands).
- **Exa replaces Brave as the keyed provider.** Brave's free tier now wants a card
  on file, which is a barrier for one person and a worse one for anybody `!s` ever
  ships to, since every user needs their own key. Exa is built for this kind of
  retrieval and returns page text with each result. Brave is kept behind the trait
  but nothing selects it.
- **A failing Exa falls through to DuckDuckGo, silently.** A wrong key, a spent
  quota or an outage becomes slightly worse answers rather than a red row, so `!s`
  is never a dead end. The cost is real and deliberate: **a broken key does not
  announce itself.** The answer header names whichever service actually replied,
  and Settings → Web search states the rule in words.
- **The no-key state stopped being an error.** The `!s` row used to read "No key.
  Add one in Settings" and Enter did nothing. It now names the provider that will
  answer, and Enter works.
- The stored key moved from `credsrave.key.dpapi` to `creds\web.key.dpapi`,
  since it is no longer Brave's and should not be named for whoever is current. A
  key stored under the old name is not migrated; paste it again.

ADR-0021 has the reasoning, including why driving a headless browser at Google was
considered and rejected a second time. TBC-0004 is resolved.

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
