# Changelog

Releases of Takyon, newest first. The installer and its SHA-256 live in
`releases/v{version}/`, which is gitignored — this file is the record that
survives.

Release versions identify shipped builds under SemVer; phase numbers name plans
in `ROADMAP.md`. A finished phase may advance the release minor, while partial
phase work ships in patches (see `ROADMAP.md`'s 0.2.x policy). The v0.8
renumbering below records a separate change to the phase plan.

## v0.12.4 — In-app updates (pending release)

- **In-app updates.** Takyon checks GitHub Releases once a day and shows a new
  version on the Palette's empty line: a pill in Compact, a row in Expanded.
  Enter installs it; the installer is signature-checked before it runs. After
  an update, the same place offers "See what's new". Settings › About checks on
  demand and has a switch that turns the daily check off. This release has to
  be installed by hand; later ones install from the Palette or About.
- Installer and executable name **V3SPER** as the publisher. `!s` requests
  identify as `Takyon/0.9 (V3SPER)`.
- Release notes lead with this file's section; GitHub then credits pull request
  authors, including the maintainer, in its generated list.

## v0.12.3 — Windows image clips (pending release)

- **Image clips on Windows.** Copying an image or taking a `Win+Shift+S`
  screenshot captures it in Clipboard History. Image thumbnails appear in the
  Palette and `!v`; the selected clip has a full preview. Paste and copy-back
  restore the image. Full PNGs live as encrypted blobs under `clips\`.
- **Delete one Clip** from Clipboard History with its Delete button, `Ctrl+D`,
  or the `Ctrl+K` action menu. Settings shows Clipboard History's on-disk size.
- **Image-blob crash recovery runs off the startup path.**

**Still open.** macOS image capture and paste-back, file clips, and link
classification remain in v0.14. `docs/verify/v0.14.md`'s automated driver
passes; its hand-run steps have not been run.

**Downgrading below 0.12.3:** older versions show image clips as broken text
rows and never clean up their blobs. Clear Clipboard History before downgrading.

## v0.12.2 — Agent orchestration and Codex resume

- Added the repository orchestration policy to `AGENTS.md` and `CLAUDE.md`.
- Fixed Codex argument order so `exec resume <id>` accepts `-C` and `--sandbox`
  while the prompt still travels on stdin.

## v0.12.1 — Focus bar and CI polish

- The Focus bar became a compact pill with a centered goal, controls that slide
  out, scrolling for long goals, adjustable opacity, and screen-edge placement.
  Its buttons stay inside the window during the animation.
- CI checks out the repository before generating "What's changed" on every
  event. Live and bench jobs run on pull requests when their paths change.

## v0.12.0 — Focus and time zones

- **Focus (phase v0.16).** Start Soft or Strict Focus sessions from the Palette.
  Focus categories, app and website blocking, a floating bar, tray status,
  settings, and session statistics shipped. The manual verification script is
  unrun.
- **Time zones (phase v0.17).** Calculator queries such as `12 pm utc`,
  `3:30pm pst to ist`, and `time in tokyo` answer offline. The card names both
  zones. The manual verification script is unrun.
- Vela became the default theme family. DuckDuckGo parsing no longer panics on
  a multibyte sequence after `%`.

## v0.11.1 — Agents that npm installed

Two defects behind one symptom: an Agent that Settings called Ready failed every
Turn, and a cancelled Turn kept spending tokens. Both appear only when the Agent
was installed with npm, which is how the test laptop had opencode.

- **An Agent's prompt travels on stdin** (ADR-0032). npm installs a batch file,
  not an executable, and since CVE-2024-24576 Rust refuses to pass any argument
  holding a carriage return or line feed through `cmd.exe`. `styled_prompt`
  always holds a blank line, so every first Turn to Codex or opencode died with
  `batch file arguments are invalid`, and every `!s` with it, because `!s` writes
  its answer through the same path. Settings still read Ready, because the probes
  are all one-liners. The prompt now goes on stdin, and stdin is closed
  afterwards, or the Agent waits for EOF forever.
- **Every Agent process dies with the Palette** (ADR-0033). `Child::kill` calls
  `TerminateProcess` on the one process Takyon started, which for an npm Agent is
  `cmd.exe` — leaving `node` and the real CLI running, reading nothing and still
  billing. Each spawn now gets its own Job Object with `KILL_ON_JOB_CLOSE`, and
  the child is created suspended and resumed only once it has been assigned, so
  there is no window it could escape through.

**Unverified.** Neither fix has run against a genuine `npm i -g` install. The
`.cmd` path is covered here by fake Agents (`echo.cmd`, `hang.cmd`, `nonode.cmd`)
and by an ignored live test that wraps each real CLI in a forwarding `.cmd`, but
the real shape, with a `node` hop in the middle, exists only on the laptop that
reported it. `docs/verify/v0.11.1.md` section A is the script.

## v0.11 — PATH hydration

A GUI process on Windows inherits the `PATH` that existed when Explorer started,
and on macOS inherits almost nothing. An Agent CLI installed since the last login
was therefore invisible to Takyon while being perfectly findable in any terminal
— the kind of failure that makes a launcher look broken for a reason the user
cannot see.

- **Takyon asks the system for the real `PATH`.** `agents/shellenv.rs` reads
  `HKCU\Environment` and the machine `Path` on Windows, and asks the login shell
  on unix. The result is cached off the startup path and handed to
  `probe::resolve`, to every spawned Agent, and to the `PATH` walk that finds
  command-line applications. Settings, Agents states what it found: *PATH read
  from the registry: 42 folders, 3 more than Takyon was launched with.*
- **A second, deeper pass for per-session version managers.** `fnm` sets `PATH`
  from a PowerShell profile into a per-session directory the registry cannot see
  by construction. `hydrate_deep` loads the profile and captures it, but only
  when the first probe found nothing at all, once per process, because it costs
  1.46 s against the registry read's 1.2 ms. nvm-for-Windows, Volta and scoop do
  not need it: each adds one fixed shim directory at install, so the registry
  read already sees them.
- **A sole installed Agent leads on a first run** (ADR-0031). Someone with only
  Codex installed used to open Settings and find it ranked second, under a Claude
  Code row reading "Not found". `!c` reached it anyway, but the page made Takyon
  look as though it had chosen wrong. When the probe finds exactly one Agent and
  no choice is stored, that Agent is ranked first and switched on, once.
- **CI ran for the first time.** v0.10.1 shipped the workflows and recorded that
  neither had ever executed, because there was no remote. There is one now:
  typecheck, lint and every test layer on each push, the live DuckDuckGo test and
  the bench harness on a schedule, with per-job timeouts and pinned toolchains.
  `lint` also gained the comment, colour and licence checks, so all three block.
- **`bun run release`**, in `--local` and `--git` shapes: preflight, build, then
  the installer and its SHA-256 into `releases/`; `--git` adds the version bump,
  the tag and the push that makes `release.yml` publish. Bare `bun run release`
  refuses.

**What it does not do.** The whole unix arm compiles for `aarch64-apple-darwin`
and has never executed — every claim about it is reasoning rather than
observation, and `docs/verify/v0.11.md` section B is unrun. The deep pass has
never been watched actually recovering a folder, because this machine has no
`fnm`. A `Restricted` execution policy stops the profile loading at all,
silently, and is deliberately not worked around: forcing a profile the machine's
own policy refuses is not Takyon's call. And a CLI installed after launch still
needs a restart before it appears as an Entry, though `!c` finds it immediately;
v0.13 owns that.

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
macOS build to make: `docs/plans/v0.12-macos.md` says what that would take.

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
