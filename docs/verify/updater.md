# In-app updater — setup and manual verification

The signed updater from [ADR-0040](../adr/0040-updates-are-signed-and-only-on-request.md):
`release.yml` signs the Windows installer and publishes `latest.json`. Takyon
checks once a day unless switched off, shows a new version on the Palette's
empty line (Enter installs), and Settings › About checks and installs on demand.

**Nothing below has been run.** The automated layers cover the parts that do not
need a published release. `scripts/updater.test.ts` and `scripts/lib/updater.test.ts`
cover signature checking against a real `tauri signer` signature, the build
overlay and `latest.json`. `cargo test --lib update` and `tests/ipc.rs` cover the
wire shapes and the no-feed path. `tests/updater.rs` runs the real plugin against
a feed on `127.0.0.1`: a newer version reads as available, a correctly signed
download passes verification, and a tampered or foreign-signed one is refused
before anything runs, and a check that finds one becomes the Palette's notice.
`tests/visual/settings.spec.ts` covers every About state, the daily switch, and
that opening About makes no check. `tests/visual/update-notice.spec.ts` covers the
Palette row and pill, Enter installing, and "See what's new". What only a person
can do is below: the key setup, two real releases, the daily schedule, and the
install itself.

## Runs

| Date | Build | Windows | What ran | Result |
|---|---|---|---|---|

## S. One-time setup (before the first release that carries the updater)

The tag workflow refuses to start until all three values exist. Do this once, on
a machine you trust, from the repository root in PowerShell.

| # | Step | Expected |
|---|---|---|
| S1 | `bun --cwd=apps/desktop run tauri signer generate -w "$env:USERPROFILE\.tauri\takyon-updater.key"` and enter a strong password when asked | Two files: `takyon-updater.key` (private) and `takyon-updater.key.pub`. The private key goes nowhere inside the repository |
| S2 | Store the `.key` file and its password in a password manager or other backup outside GitHub | Recoverable. **Losing either strands every installed copy**: they trust only this key |
| S3 | `Get-Content -Raw "$env:USERPROFILE\.tauri\takyon-updater.key" \| gh secret set TAURI_SIGNING_PRIVATE_KEY` | `gh secret list` shows it. Piped, so the key never lands in shell history |
| S4 | `gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD` and type the password at the prompt | Listed by `gh secret list` |
| S5 | `$pubkey = (Get-Content -Raw "$env:USERPROFILE\.tauri\takyon-updater.key.pub").Trim(); gh variable set TAKYON_UPDATER_PUBKEY --body $pubkey` | `gh variable list` shows it. The signer already writes the `.pub` file in Tauri's required format; the public key belongs in a variable |
| S6 | `$env:TAKYON_UPDATER_PUBKEY = $pubkey; $env:GITHUB_REPOSITORY = (gh repo view --json nameWithOwner -q .nameWithOwner); bun scripts/updater.ts config "$env:TEMP\updater.conf.json"` | Prints `config written, feed https://github.com/<owner>/<repo>/releases/latest/download/latest.json`. A pasted private key or a truncated value fails here instead of in CI |

The GitHub web UI works as well: Settings › Secrets and variables › Actions, with
the full file contents pasted as each value. Use a separate key for Takyon; do
not reuse another app's updater key.

## A. When the feed is contacted

| # | Step | Expected | Status |
|---|---|---|---|
| A1 | Install a release build. Open Resource Monitor › Network, filter on `takyon.exe`. Log in and, within the first minute, open and dismiss the Palette, run Bangless queries and open Settings › About | No connection to `github.com` or `release-assets.githubusercontent.com` in that first minute: summons, queries and About make none | |
| A1b | Keep watching for two more minutes | One connection to `github.com` about 90 s after launch, then nothing. Restart Takyon: no second check the same day | |
| A1c | Settings › About › switch off "Check for updates automatically", then set `update.last_check_ms` aside by restarting a day later (or remove the row in `settings.db` with Takyon closed) | No background connection after 90 s. The button still checks when pressed | |
| A2 | Build locally with `bun run build`, run it, Settings › About › **Check for updates** | "This build can't update itself. Installers from the Takyon releases page can." No connection appears in Resource Monitor | |

## B. What a release publishes

Needs a tag pushed by the user (`bun run release --git <version>`).

| # | Step | Expected | Status |
|---|---|---|---|
| B1 | Open the GitHub release for the tag | Assets: `Takyon_<v>_x64-setup.exe`, `Takyon_<v>_x64-setup.exe.sig`, `SHA256SUMS.txt`, `latest.json`. The body is the version's `CHANGELOG.md` section, the verification note, then GitHub's generated "What's Changed" crediting pull request authors, including the maintainer. About shows the `CHANGELOG.md` section only | |
| B2 | `curl -L https://github.com/<owner>/<repo>/releases/latest/download/latest.json` | `version` is the tag without `v`; both `windows-x86_64` and `windows-x86_64-nsis` point at this release's installer | |
| B3 | Right-click the downloaded installer › Properties › Details; after installing, Settings › Apps › Installed apps | Company and publisher read **V3SPER**. No personal name anywhere on either | |
| B4 | In the workflow run's logs, search for the private key's first characters and for the password | Not present. Secrets are masked and no step echoes them | |

## C. Check and install

Install release N by hand, then publish N+1 through the tag workflow.

| # | Step | Expected | Status |
|---|---|---|---|
| C1 | On N: Settings › About › **Check for updates** | "Takyon N+1 is available" with that version's notes underneath. The button becomes **Install and restart** | |
| C2 | Press **Install and restart** | A percentage while it downloads, then "Installing". Takyon exits, NSIS shows a progress window with no questions, and Takyon starts again by itself | |
| C3 | Settings › About | Version N+1 | |
| C4 | Check the hotkey, theme, Clipboard History and Focus categories | All as they were before the update | |
| C5 | `Get-ItemProperty HKCU:\Software\Microsoft\Windows\CurrentVersion\Run` | `com.v3sper.takyon` still present if autostart was on before; Task Manager › Startup apps still lists Takyon as enabled | |
| C6 | Installed apps and the Start menu | One Takyon entry at N+1, one working Start menu shortcut | |
| C7 | Press **Check for updates** again on N+1 | "Takyon N+1 is the latest version." | |
| C9 | Install N again by hand. Wait for the daily check (A1b), then press the hotkey | Compact: an "Update available vN+1" pill at the input row's right edge. Expanded: a selected row under the empty line, with "Install Update" in the footer | |
| C10 | Type a letter, then clear it | The row goes while typing and comes back on the empty line | |
| C11 | Press Enter on the row | Progress on the row, then "Installing", Takyon exits and comes back at N+1 with no questions | |
| C12 | Press the hotkey on N+1 | "See what's new vN+1". Enter opens the N+1 release page in the browser; the next summon has no row | |
| C8 | During C2 have a `!c` Turn running | After the update, no `claude`, `codex`, `opencode` or `node` process from the old Takyon remains (ADR-0033's Job Object closes with the process) | |

## D. Failures

| # | Step | Expected | Status |
|---|---|---|---|
| D1 | Turn networking off, **Check for updates** | "Couldn't reach the update feed. Check the connection and try again." beside the button; nothing else changes | |
| D2 | Check while online, turn networking off, press **Install and restart** | The same message; the button still reads **Install and restart**. Networking back on, pressing it again installs | |
| D3 | Set a system proxy (Settings › Network › Proxy) that logs requests, then check | The request appears in the proxy's log | |
| D4 | A signature that fails | Not hand-runnable without a second feed. Covered by `scripts/lib/updater.test.ts` (release side) and `tests/updater.rs` (the plugin, app side). Mark blocked, not passed | |
