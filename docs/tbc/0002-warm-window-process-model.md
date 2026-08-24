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

## Measured — v0.1, 2026-08-24

Release build (`lto = true`, `codegen-units = 1`, `panic = "abort"`), via
`bun run bench`. Machine: AMD Ryzen 7 7745HX, 15.7 GB RAM, Windows 11 26200,
2560x1600 at 150%.

| Metric | Budget | Measured | |
|---|---|---|---|
| Hotkey to first pixel | < 50 ms | **p50 20.2 ms · p95 22.6 ms · max 25.3 ms** (n=30, min 16.5) | PASS |
| Process start to hotkey responsive | < 500 ms | **311.6 ms** | PASS |
| Idle RSS, warm and trimmed | < 150 MB | **27.5 MB** across 7 processes | PASS |
| First show after 30+ min idle | — | **still not measured** | — |
| Hotkey to first Entry | < 30 ms | not applicable yet | v0.2 |

### The memory result is the interesting one

**27.5 MB resident against 184.4 MB committed.** The trim on hide is releasing
roughly 85% of the process tree's committed memory back to the OS, and the
resident figure lands at less than a fifth of the budget.

That is a much better outcome than this document assumed. The first trigger below
— "idle RSS after trimming stays above 60 MB, meaning trimming isn't buying what
we assumed" — has **not** fired, and is not close to firing. The ADR-0003 trade
looks, on this evidence, to have been priced pessimistically rather than
optimistically.

Two caveats worth keeping attached to that number:

- It counts **7 processes**, not one. WebView2 runs a browser, renderer and GPU
  process, and the renderer is a child of the browser process rather than of us.
  An earlier draft of the trim only released this process's working set, which
  would have made this measurement meaningless in the flattering direction.
- Committed memory does not fall when the working set is trimmed, which is why
  both numbers are recorded. If someone later reports "Takyon uses 184 MB", they
  are reading the commit charge and they are not wrong — the resident figure is
  the one that reflects actual RAM pressure.

### What the latency number includes

Both ends are stamped in Rust on one clock; the frontend only echoes an id back
after a double `requestAnimationFrame`. So the span is *hotkey handler entry to
the IPC call following the frame the renderer committed*: it **includes** one IPC
hop and **excludes** DWM's final composition and present.

The margin is wide enough that the excluded portion cannot plausibly close it —
DWM composition is a frame, and the budget has 27 ms of headroom at p95. Treat
these as a regression gate; the 240fps calibration capture (step I5 of the manual
script) converts them into a claim about what the eye sees.

### Still owed: the number that actually decides this

`bun run bench --idle 35`. Every measurement above was taken seconds after the
previous show, in exactly the tight loop this document warns produces flattering
results. **Windows reclaiming the trimmed working set over a long idle is
unmeasured**, and it is the third trigger below — the one that would mean paying
the memory *and* losing the latency.

Until that run exists, the honest summary is: the warm-window model is passing
every budget it has been tested against, and has not yet been tested against the
case most likely to break it.

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
