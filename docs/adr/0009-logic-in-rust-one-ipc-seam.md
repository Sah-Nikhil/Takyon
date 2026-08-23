---
status: accepted
---

# All logic lives in Rust; the UI reaches it through a single IPC seam

Matching, ranking, Frecency, index queries, clipboard access and Bang parsing are
implemented in Rust. The React UI receives a finished, ordered list of roughly ten
Entries and renders them — it does no filtering, no sorting and no scoring. One
`invoke` per keystroke, never one per Source: the Rust side fans out to Sources,
merges, ranks and applies the Stability rule internally, and returns once.

Every call into Rust goes through a single `api.ts` module. **No component calls
`invoke()` directly.**

## Why the seam, given it does nothing at runtime

`api.ts` is a file of one-line wrappers (`export const search = (q: string) =>
invoke<Entry[]>("search", { q })`). It adds one JIT-inlined function call and
costs nothing measurable — the expensive part, JSON serialisation across the
webview↔Rust boundary, happens identically either way. It is recorded here
because it *looks* like a needless abstraction, and a reasonable reader will want
to delete it.

It buys two things:

1. **The UI can run outside Tauri.** With the seam mocked, the React app runs in
   the plain Vite dev server in an ordinary browser, which is what makes
   deterministic visual-regression testing possible at all. The moment one
   component calls `invoke()` directly, that component becomes untestable by that
   route and the layer starts rotting.
2. **The IPC contract becomes one reviewable file.** Every command the frontend
   can issue is visible in a single place, which matters for keeping the ADR-0002
   guarantee checkable.

## Considered Options

- **Write the UI in Rust**: not available. Tauri renders its UI in a WebView2
  instance; there is no Rust-UI mode. A genuinely native Palette means dropping
  the webview entirely — that is TBC-0002's escape hatch, costing 15–25 days, and
  is warranted only if measurement indicts WebView2 itself.
- **Call `invoke()` directly from components**: identical performance, marginally
  less code, and forfeits both benefits above.
