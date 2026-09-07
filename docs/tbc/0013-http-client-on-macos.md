---
status: watching
pairs-with: ADR-0019
---

# TBC-0013 — The HTTP client on a second platform

## The bet

ADR-0019 chose WinHTTP over a Rust HTTP client for three reasons: TLS is the OS's,
the proxy is the user's, and nothing is added to the installer. The bet this note
records is that **those three reasons hold platform by platform, so the answer on
macOS is `URLSession` rather than a Rust client** — the same argument, reached
independently, giving the same shape of answer twice.

`search::fetch` is the seam. Its surface is two functions, and ADR-0019 already
said a macOS target would reimplement it.

## How we'd know we were wrong

The argument is about cost, so the trigger is a cost that turns out to be wrong:

- **Objective-C bindings for `URLSession` exceed roughly 400 lines**, which is what
  `fetch.rs` costs today in WinHTTP FFI. Above that, the "no new crates" saving is
  being paid for in `unsafe` twice over, in a subsystem that is otherwise the most
  portable thing in the Rust core.
- **The `objc2` dependency tree turns out to be larger than a TLS backend.** The
  installer-size argument is the strongest one in ADR-0019 (roughly 2 MB of
  `rustls` onto a 2.6 MB installer). If `objc2` plus `objc2-foundation` costs
  comparably on macOS, the argument has been reused where it does not apply.
- **A second non-Windows target appears** — Linux, or a headless test harness that
  wants a fake server. Two OS-specific HTTP clients is a pattern; three is a
  mistake, and the third would arrive with no `URLSession` to inherit.
- **Any GPL-licensed dependency in the objc binding tree**, which is disqualifying
  outright while ADR-0005's licensing question is open.

## Alternatives

| Option | Improvement if we switch | Added complexity | Switching cost |
|---|---|---|---|
| `URLSession` via `objc2` | ADR-0019's three arguments hold on macOS as stated; no TLS, no cert store, no proxy config shipped | Objective-C FFI in the one subsystem that is otherwise pure logic; a second `unsafe` surface to review | 3–5 dev-days |
| `reqwest` + `rustls`, both platforms | One implementation, testable against a local server, no `unsafe` at all | ~2 MB on the Windows installer, an async runtime in a synchronous codebase, and ADR-0019 reversed | 2–3 dev-days, plus re-opening ADR-0019 |
| `ureq` + `rustls`, both platforms | Blocking, which is how `fetch` is called; smaller than `reqwest` | Same TLS-backend size argument with less force; still reverses ADR-0019 | 2 dev-days |
| `reqwest` on macOS only, WinHTTP on Windows | Fastest route to a working `!s` on the port | Two clients with different redirect, proxy and certificate policy — `!s` behaves differently per platform for reasons no one can see | 1 dev-day, and a bug class forever |

The last row is the one to be careful of, because it is the cheapest and it is how
this decision gets made by accident during the port. That is precisely what
`docs/plans/macos.md` warned against: this belongs in a TBC before the port, not
in a commit message during it.

## Verdict if triggered

Take the whole thing to `ureq` + `rustls` on both platforms and retire ADR-0019
rather than amending it. If the OS-stack argument fails on the second platform it
was never really an argument about TLS — it was an argument about Windows, and one
client with one behaviour is worth 2 MB.

Do **not** take the per-platform split. If `URLSession` proves too expensive, that
is evidence for one Rust client everywhere, not for two native ones.
