---
status: watching
pairs-with: ADR-0005
---

# TBC-0014 — What a macOS build is allowed to cost

## The bet

**A macOS `.dmg` ships ad-hoc signed and un-notarised, and first launch is a
right-click → Open.** No Apple Developer Program membership, no $99/yr, no
notarisation step in `release.yml`.

The bet is that this is *consistent* rather than merely cheap. The Windows build
is in the same posture already: the UIAccess helper is unsigned, a real
code-signing certificate is an open v1.0 blocker, and Defender currently
quarantines the installed binary as a behavioural false positive. Asking macOS to
be the polished one while Windows is not would be paying for gloss on the platform
with no users yet.

## How we'd know we were wrong

- **Gatekeeper hardens further.** The right-click → Open escape hatch has been
  narrowed once per release for several macOS versions. If it stops working — or
  moves behind System Settings → Privacy & Security with no obvious affordance —
  the first-run experience becomes "the app is damaged and can't be opened", which
  is indistinguishable from a corrupt download.
- **Anyone who is not the author installs it.** That is v1.0's exit criterion, and
  it is also the moment "right-click the first time" stops being a note in a README
  and starts being support.
- **The updater lands.** `tauri-plugin-updater` is a v1.0 item; an unsigned update
  on macOS is a worse proposition than an unsigned first install, because the user
  is not there to right-click it.
- **The Windows certificate gets bought.** The consistency argument is the load
  bearing half of this bet. The day Windows is signed, "macOS is unsigned too" stops
  being a posture and starts being an omission.
- **Distribution settles as open source** (ADR-0005's open question). A Homebrew
  cask is the expected route for an unsigned open-source app and it changes the
  first-run story: `brew install --cask` does the quarantine dance itself.

## Alternatives

| Option | Improvement if we switch | Added complexity | Switching cost |
|---|---|---|---|
| Ad-hoc signed `.dmg` | Nothing to buy, nothing to renew; `release.yml`'s `build-macos` job already produces it | First launch is right-click → Open, and the failure text blames the download | — (this is the bet) |
| Apple Developer Program, signed + notarised | First launch is a double-click; the updater works without ceremony | $99/yr, an Apple ID tied to a real identity, a notarisation step and its credentials in CI | $99/yr + 1–2 dev-days |
| Homebrew cask, unsigned | Quarantine handled by the package manager; no fee | Only reaches users who have Homebrew; needs the source public first (ADR-0005) | 1 dev-day, gated on ADR-0005 |
| No macOS distribution — build from source only | Honest about the state of things; costs nothing | The `build-macos` CI job produces an artifact nobody can install | — |

## Verdict if triggered

Buy the Apple membership **in the same decision as the Windows certificate, not
before it**. Both are the same class of purchase, both are annual, and buying one
platform's trust while the other is quarantined by Defender is the worst spend of
the three options.

Until then the `MACOS_BUILD` repository variable stays unset, which is what
`.github/workflows/release.yml` already assumes.
