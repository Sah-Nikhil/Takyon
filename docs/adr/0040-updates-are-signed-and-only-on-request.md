---
status: accepted
amends: 0010, 0019
pairs-with: 0002
---

# Updates come from GitHub Releases, signed, checked once a day unless switched off

The filename predates the daily check; see "Revised the same day" below.

Takyon updates itself through `tauri-plugin-updater`. The feed is the
`latest.json` that `release.yml` publishes beside each Windows installer, read
from `https://github.com/<repository>/releases/latest/download/latest.json`. The
installer is signed with a minisign key whose private half lives only in the
repository's Actions secrets. The app verifies that signature against the public
key compiled into it before it runs anything.

**The feed is reached from three places, and only these.**

1. **A background check, at most once a day**, on by default. It waits 90 s
   after launch, runs on its own thread, and is switched off by Settings › About
   › "Check for updates automatically" (`update.auto_check`).
2. **Settings › About › Software update**, when its button is pressed.
3. **Enter on the Palette's update row.** It installs at once, as Raycast's
   does. A row left by an earlier run has no download in memory, so Enter asks
   the feed once more before installing.

The request carries what any HTTPS GET carries and nothing of Takyon's: no
query, no version parameter, no device ID. GitHub sees the address and time.

**The Palette row never makes a request.** A check stores what it found in
`settings.db` and in memory (`update::notice`). `window::show` reads the memory
copy and sizes the window for the row before it appears; the empty line draws a
pill in the input row in Compact and a row in Expanded. After an update lands,
the same place says "See what's new" until it is opened, and Enter opens that
version's release page in the browser. That needs no check at all: launch
compares the running version with the last one that ran.

## Revised the same day: from button-only to a daily check

The first version of this ADR allowed the button and nothing else. The user
reversed that before it shipped: people who never open Settings would never
learn an update exists. Prior art agreed. Raycast, PowerToys, VS Code and t3code
all check by default; t3code polls every four minutes. Once a day with an off
switch was chosen over opt-in, because opt-in reaches nobody who would not have
pressed the button anyway.

## Amending ADR-0010

ADR-0010 said "no update pings beyond the explicit updater check". This ADR
replaces that clause: **one update check a day, on by default, with an off
switch in About.** Nothing else in ADR-0010 moves. No analytics, no crash
reports, and the check carries no identifier and no content.

## Why this shape

- **ADR-0002 is untouched.** The query handler and `window::show` read a value
  in memory; no request starts from a keystroke or a summon. The plugin's own
  `plugin:updater|*` commands are in no capability, so the webview reaches the
  plugin only through `update.rs`'s commands.
- **GitHub Releases is where the installers already are.** No server, no domain,
  no second place to keep in step. The tag → CI → release pipeline stays the only
  way a build reaches anyone.

## Where the key and the feed live

Neither is committed. `release.yml` writes a `--config` overlay
(`scripts/updater.ts config`) that adds `createUpdaterArtifacts`, the public key
from the `TAKYON_UPDATER_PUBKEY` repository variable and the feed URL derived from
`GITHUB_REPOSITORY`. The committed `tauri.conf.json` carries an empty
`plugins.updater` block, because the plugin refuses to start without one.

So `bun run build`, the bench and `bun run release --local` need no key and
produce a build whose About page says it cannot update itself, rather than a
build that fails or one that checks a feed it has no key for. The feed URL is not
in the source, so moving the repository changes a variable, not code.

The release job checks the signature with the same algorithm the app uses before
it writes `latest.json`. A secret and a public key from two different key pairs
would otherwise publish an update that every installed copy refuses, and nothing
would report it.

## Amending ADR-0019

ADR-0019 says Takyon speaks HTTP through WinHTTP and never through a Rust client.
The updater is the exception: it uses the plugin's `reqwest`. ADR-0019's two
objections are met rather than waived:

- **Its own TLS stack.** The plugin's `rustls` default is off. `native-tls` is
  Schannel on Windows, so the trust store is the user's own, as with WinHTTP, and
  no `rustls` or `ring` is compiled.
- **The user's proxy.** `reqwest`'s `system-proxy` feature is turned on through a
  direct dependency entry, so the Windows proxy setting is honoured.

What remains is size and an async runtime, and Tauri already ships `tokio`.
`!s` stays on WinHTTP; nothing else moves to `reqwest`.

## Considered options

| Option | Why not |
|---|---|
| Check on every launch, as tesseract does | A request per login, scaling with restarts rather than with time |
| Check when the Palette opens | Makes the hotkey a network trigger and puts a request on the summon path ADR-0002 keeps local |
| Opt-in background check | Reaches only people who would have pressed About's button anyway |
| Button only (this ADR's first version) | Most users never open Settings, so most installs would never update |
| Enter opens About instead of installing | Safer against a stray Enter, but slower; the user chose Raycast's behaviour |
| Own updater on WinHTTP plus `minisign-verify` | Re-implements the download, signature and NSIS hand-off the plugin already tests, for one fewer HTTP stack |
| Commit the public key and feed in `tauri.conf.json` | Every local and bench build would then need the private key to build at all, or would ship a key with no signed artefacts behind it |
| `generate_release_notes` alone for the release body | It lists pull request titles, not what changed for a user. The body leads with the `CHANGELOG.md` section and keeps the generated list under it. `.github/release.yml` excludes the owner's account while crediting outside contributors; `latest.json` carries the `CHANGELOG.md` section only |

## Consequences

- The first build that carries the updater has to be installed by hand. Every
  build after it can be installed from About or the Palette.
- A stray Enter on the empty line installs and restarts. The row is selected
  only on the empty line, and typing anything moves the selection to Entries.
- On Windows the plugin runs the installer in passive mode with `/UPDATE` and
  exits the process. Tauri's NSIS template skips its uninstall steps in that
  mode; `installer-hooks.nsh` skips its `Run` cleanup too, so autostart survives.
- Losing the private key strands every installed copy: they trust only the key
  they were built with. The key and its password need a backup outside GitHub.
- macOS builds carry no updater artefacts yet. The `build-macos` job is unchanged
  and `latest.json` has Windows entries only.
- Verification is `docs/verify/updater.md`. The end-to-end install has to be
  driven by hand against two real releases.
