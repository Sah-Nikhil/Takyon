---
status: watching
pairs-with: ADR-0003
---

# TBC-0001 — React as the Palette's frontend framework

## The bet

We chose React 19 + Vite 7 + Tailwind v4 + cmdk, matching the proven tesseract
stack, over the smaller Solid and Svelte. The assumption that makes this correct
is **ADR-0003**: because the Palette window stays warm, React's mount and
parse cost (~10–20 ms, ~60 kB gzipped) is paid once at login into a hidden window
and never on the hot path. What remains on the hot path is re-rendering roughly
ten visible rows per keystroke — about 0.5–2 ms for React against a 16.7 ms frame
budget, i.e. unobservable. React's extra ~3–5 MB of heap is noise beside
WebView2's ~100 MB floor.

This bet is therefore **downstream of ADR-0003**. If the warm-window model falls,
this decision should be re-opened at the same time, not separately.

## How we'd know we were wrong

- The post-V1 benchmark (see `docs/plans/post-v1.md`) moves us to a cold-start or
  warm-on-signal model, putting framework mount cost onto the hot path.
- Keystroke-to-repaint exceeds **8 ms** at p95 with a full result list, and
  profiling attributes it to render rather than to the Rust query.
- The result list needs to render into the hundreds of rows without
  virtualisation, where React's reconciliation cost stops being flat.
- Idle heap attributable to the frontend exceeds ~15 MB.

## Alternatives

| Option | Improvement if we switch | Added complexity | Switching cost |
|---|---|---|---|
| **Solid** | ~53 kB less JS, ~10 ms faster boot, ~0.4 ms faster per keystroke, ~3 MB less heap | Low — JSX transfers directly; the mental model is close enough that most component bodies survive. Loses cmdk and Radix, which are React-only | **4–7 days.** Rewrite every component, hand-build the list/keyboard primitives cmdk gave us. Rust side untouched |
| **Svelte 5** | Same as Solid, slightly smaller | Medium — different syntax to learn, but a larger ecosystem and better docs than Solid | **6–10 days.** As above plus the learning curve |
| **Vanilla TS** | Maximum — no framework at all, ~0 kB | High — every list update, focus rule and keyboard interaction hand-written and hand-tested. The Chat Surface becomes genuinely painful | **8–14 days**, and permanently slower to change afterwards |
| **Stay on React, optimise instead** | Recovers most of the gap for a fraction of the cost | Low | **1–2 days.** Virtualise the list, memoise rows, move filtering off the render path. Try this *first* |

## Verdict if triggered

Try the last row first — virtualisation and memoisation recover most of the
achievable gain for one or two days of work, and if they close the gap the whole
question dies. Only if measurement still indicts the framework itself, switch to
**Solid**: JSX means the port is mechanical rather than a rewrite, and the
components we'd lose (cmdk, Radix) amount to a list and a few primitives in an
application this small.
