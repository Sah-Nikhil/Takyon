---
status: watching
pairs-with: ADR-0038
---

# TBC-0016 — Bundled tzdb on Windows goes stale between releases

## The bet

On Windows, time zone rules come from the copy of tzdb that `jiff` bundles into
`takyon.exe` at build time. Windows itself updates its own rules through Windows
Update, so the taskbar clock can be right while Takyon is wrong, until Takyon
ships a release built against the newer tzdb.

The assumption is that this rarely matters. Rule changes affect one or two
countries at a time, a few times a year, and are usually announced months
before they take effect. A release every few months picks most of them up
before anyone notices. macOS is not affected: jiff reads the OS copy there.

## How we'd know we were wrong

- **A change lands first.** A tzdb release changes a rule that takes effect
  before the next Takyon release ships with it.
- **Someone reports a wrong time.** Any report of a time zone answer off by an
  hour against the Windows clock, for a zone whose rules changed recently.
- **Releases slow down.** More than six months between releases, which is about
  how far ahead most changes are announced.

## Alternatives

| Option | Improvement if we switch | Added complexity | Switching cost |
|---|---|---|---|
| ICU from Windows' `icu.dll` (`ucal_*`) | Rules update with Windows Update | FFI over a C API; `tz.rs` keeps jiff on macOS, so two paths | 2–3 d |
| Ship tzdb as a data file updated by the v1.0 updater | Fresh rules without a new binary | An update channel for data, and a signed file | 2 d, after `tauri-plugin-updater` exists |
| Bump `jiff-tzdb` in a patch release when tzdb changes | Nothing new to build | Watching tzdb's announcements | 0.5 d per release |

## Verdict if triggered

The patch release first, since it costs nothing to build. If it fires twice,
move Windows to ICU.
