---
status: accepted
amends: ADR-0019
pairs-with: TBC-0013
---

# `URLSession` is the HTTP client on macOS

`search::fetch` speaks HTTP through **`NSURLSession`** on macOS, as it speaks
WinHTTP on Windows. The seam is unchanged: two functions, `send` and the `pages`
fan-out above it, with URL parsing, percent-encoding and the deadline logic
shared.

ADR-0019 chose WinHTTP over a Rust client for three reasons — TLS is the OS's,
the proxy is the user's, and nothing is added to the installer. All three hold on
macOS with `URLSession` and none of them holds with `reqwest` or `ureq`. The
argument is the same argument, reached twice, giving the same answer twice.

## Why this overrides TBC-0013's stated verdict

TBC-0013 said that if `URLSession` proved expensive, the answer was one Rust
client on both platforms, and that a per-platform split was the option to refuse
outright. It priced `URLSession` at "objc bindings for the one subsystem that is
otherwise trivially portable" and set a trip-wire at roughly 400 lines of FFI.

**The fact it reasoned from was wrong.** `objc2-foundation` is already in
`Cargo.lock`, pulled in by Tauri (ADR-0026), so `NSURLSession` costs no
dependency at all. With `block2` for the completion handler, the implementation is
well under the trip-wire.

Its "refuse the split" verdict still deserves an answer rather than a dodge. That
verdict was aimed at a split where **one side is a Rust client with its own TLS
stack, its own certificate store and its own redirect policy** — genuinely
different behaviour, invisible to the user, varying by platform for no reason they
could discover. Two OS stacks, each honouring the machine it runs on, is the same
*policy* expressed twice: the user's proxy, the user's certificate store, the
user's TLS configuration, on both platforms. That is less divergence from what the
user configured, not more.

TBC-0013 is amended rather than obeyed, and says so.

## Consequences

**Behaviour Takyon does not control, it still does not control.** Redirects,
proxy discovery and certificate validation are the OS's policy on both platforms,
which was already ADR-0019's accepted trade. It remains the reason
`tests/web_search.rs` reaches the real internet behind `#[ignore]` rather than
standing up a local server — there is no seam to point at a fake one, by design.

**Blocking, not async.** `fetch::send` is called from a Turn's thread and returns
a value; `URLSession`'s completion handler is turned into that by a
`std::sync::mpsc` channel and a bounded wait, with the same `TIMEOUT_MS` budget
WinHTTP is given. Nothing async leaks into the codebase, which has deliberately
stayed synchronous and thread-based since v0.1.

**Favicons come along for free.** `fetch::get_icon` sits above `send`, so ADR-0022's
source-card favicons work on macOS the moment this lands, with the same
`MAX_ICON` cap.

**A third platform would settle this differently.** If Linux ever appears, there
is no third OS stack worth binding and the honest answer becomes one Rust client
everywhere — at which point ADR-0019, this ADR and TBC-0013 are all retired
together. That trigger is recorded in TBC-0013.
