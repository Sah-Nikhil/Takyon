---
status: accepted
---

# `!s` speaks HTTP through WinHTTP, not through a Rust client

Web search needs two kinds of request: one keyed call to the Brave Search API, and
a parallel fan-out that reads the pages it names. Both go through **WinHTTP**, the
HTTP stack Windows already ships, called through the `windows` crate that is
already a direct dependency. TLS is Schannel, the certificate store is the user's
own, and the proxy is whatever the machine is configured for.

## Considered Options

- **`reqwest`**: already present in `Cargo.lock` as a transitive dependency, so it
  looked free. It is not. Nothing in the tree enables a TLS backend for it, so
  HTTPS would mean adding `rustls` and its cryptographic backend — roughly two
  megabytes onto a 2.6 MB installer, for a product whose first claim is that it is
  small. It also brings its own async runtime into a codebase that has deliberately
  stayed synchronous and thread-based.
- **`ureq`**: smaller and blocking, which suits how this code is called, but it is
  a new dependency tree with its own TLS backend and the same size argument
  applies with less force rather than not at all.
- **WinHTTP**: no new crates, no bundled TLS, no bundled certificate store, and the
  system proxy is honoured without configuration. The cost is a few hundred lines
  of `unsafe` FFI and a handle type that has to close itself.

## Consequences

The client is Windows-only, which matches where Takyon is. The macOS target in
`docs/plans/post-v1.md` will need `NSURLSession` or a Rust client behind the same
seam; `search::fetch` is that seam and its surface is two functions.

Behaviour Windows gives us for free is behaviour we do not control: redirects,
proxy discovery and certificate validation are the OS's policy, not ours. That is
the right default for a launcher, and the wrong one the day a test needs a fake
server — which is why `tests/web_search.rs` reaches the real internet behind
`#[ignore]` rather than standing up a local one.

Switching to a Rust client later is a one-file change. The three call sites are
`get`, `get_url` and `pages`, and the provider trait above them never sees a
socket.
