---
status: watching
pairs-with: ADR-0003
---

# TBC-0002 — The always-warm, trimmed-on-hide process model

## The bet

One Palette window stays alive for the process lifetime, hidden rather than
destroyed, with its working set released on hide. The assumption: **latency is
felt on every invocation and memory is checked once**, so trading ~100 MB of idle
RSS for tens of milliseconds of show time is the right side of the trade — and
working-set trimming recovers most of the memory anyway, at a cost of maybe
5–15 ms of page faults on show.

Nothing here has been measured. This is the single least-evidenced load-bearing
decision in the project, which is why it is scheduled for revisit with numbers.

## How we'd know we were wrong

- Idle RSS after trimming stays above **60 MB**, meaning trimming isn't buying
  what we assumed and we're paying full price for warmth.
- Post-trim show time exceeds **50 ms**, meaning we're paying the memory *and*
  losing the latency — the worst quadrant.
- Windows aggressively reclaims or swaps the trimmed process, so the first show
  after a long idle is dramatically slower than the second (the case a benchmark
  run in a tight loop will completely miss — measure after 30+ minutes idle).
- WebView2 proves unstable when kept alive for days: leaks, GPU process crashes,
  or a wake-from-sleep failure requiring a restart.

## Alternatives

| Option | Improvement if we switch | Added complexity | Switching cost |
|---|---|---|---|
| **Always cold** — create the window on hotkey | Idle RSS drops to ~15 MB (a Rust tray process only) | Low — arguably simpler | **2–4 days.** But show time goes to hundreds of ms–seconds, losing the product's core claim. Only viable if paired with a native, non-WebView2 Palette |
| **Warm on signal** — pre-warm on modifier press, keystroke burst, or during active hours | Most of the speed at a fraction of average idle RAM | Medium — a heuristic that is sometimes wrong, and being wrong is indistinguishable from being slow | **3–5 days**, plus ongoing tuning of a heuristic that never fully stops being wrong |
| **Native Palette, WebView only for the Chat Surface** | Removes WebView2 from the hot path entirely — sub-10 ms show, ~20 MB idle. Strictly better on both axes | High — the Palette UI is rewritten in a native Windows toolkit, and macOS then needs a third implementation | **15–25 days.** The real endgame if this product ever needs to be dramatically faster than Raycast rather than merely faster |

## Verdict if triggered

If trimming underperforms but warmth still wins on latency, accept the memory and
say so publicly — it's an honest trade. If both triggers fire together, the answer
is not "go cold", it's **native Palette with a WebView Chat Surface**: it's the
only option that improves both axes at once, and the Bang/Source architecture
(all of which lives in Rust) survives the change untouched. That's the reason to
keep every Source, ranker and index behind Rust traits with no UI knowledge —
it keeps this escape hatch cheap.
