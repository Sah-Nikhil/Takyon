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

Release build (`lto`, `codegen-units = 1`, `panic = "abort"`). Machine: Ryzen 7
7745HX, 15.7 GB, Windows 11 26200, 2560x1600 @ 150%.

| Metric | Budget | Measured | |
|---|---|---|---|
| Hotkey to first pixel | < 50 ms | p50 **20.2** · p95 **22.6** · max 25.3 (n=30) | PASS |
| **First show after 35 min idle** | < 50 ms | **22.8 ms** | PASS |
| Process start to hotkey responsive | < 500 ms | **311.6 ms** | PASS |
| Idle RSS, warm and trimmed | < 150 MB | **~107 MB** steady, 36–110 observed | PASS |
| Hotkey to first Entry | < 30 ms | n/a until v0.2 | — |

## The bet holds. One assumption inside it did not.

### Latency after a long idle: no penalty at all

This was the trigger this document was most worried about, and it did not fire.

| | ms |
|---|---|
| First show after **35 minutes** idle | **22.8** |
| The very next show, seconds later | **30.8** |

The cold show was *faster* than the warm one that followed it. That is noise, not
a real ordering — but noise dominating means the cold-start penalty is smaller
than run-to-run variance, i.e. indistinguishable from zero. Both sit inside a
50 ms budget.

So: **trimming does not cost a slow first summon.** The 5–15 ms of page faults
this document budgeted for is not visible.

### Memory: the 27 MB figure was wrong, and I recorded it before checking

An earlier revision of this section reported **27.5 MB** idle RSS and concluded
the trade had been priced pessimistically. That number was taken three seconds
after a hide. It caught the trough, not the resting state.

Sampling every two minutes across the idle shows what actually happens:

```
minute   0   12.1 MB   <- immediately after the trim
minute   2   97.2 MB   <- already back
minute   4   97.7
minute   6  108.7
minute  10  107.9
minute  16  110.2
minute  20  110.1
minute  22   36.1      <- Windows reclaims
minute  26   40.5
minute  28   40.4
minute  30  106.2      <- and it returns
minute  34  109.0
```

The trim releases almost everything — 12 MB is a real number, and the process
tree does briefly hold only that. But WebView2 faults its pages back within two
minutes and settles around **107 MB**, with occasional reclaim dips to ~40 MB
that refill again.

Committed memory barely moves throughout (169–184 MB), which is the expected
counterpart: trimming unmaps, it does not free.

**On the stated criterion, the first trigger has fired.** "Idle RSS after
trimming stays above 60 MB, meaning trimming isn't buying what we assumed" — the
resting figure is ~107 MB, comfortably above 60.

### Why that does not change the decision

The trigger was written to detect *paying the memory without getting the
latency*. Half of that turned out to be true and the half that mattered did not:

- The 150 MB budget is met with ~40 MB of headroom, at rest, honestly measured.
- The latency the memory was being traded for is delivered — including in the
  case this document flagged as most likely to break it.

What is actually disproven is the **mechanism** in ADR-0003, not its conclusion.
The ADR says working-set trimming "recovers most of the memory anyway". Over a
few seconds, yes. Over the minutes a user actually idles, no — it recovers most
of it briefly and then hands it back. Anyone reading that sentence and expecting
~30 MB at rest will be wrong by a factor of three.

**Recommended amendment to ADR-0003:** state the resting figure as ~107 MB and
drop the claim that trimming recovers most of the memory. Trimming's real value
here is that it caps the peak and costs nothing on the show path — not that it
keeps the process small.

### Two things worth chasing before v0.2, neither blocking

1. **What faults the pages back?** Nothing should be running in a hidden window.
   A plausible suspect is the idle mark animation: it is specified to stop while
   the Palette is hidden, and if it does not, WebView2 keeps compositing a window
   nobody can see. Worth confirming, since it would also be drawing power.
2. **7 processes at rest.** Expected for WebView2, but worth re-checking once
   Sources exist, because that is the count every future memory number scales
   from.

### What the latency numbers include

Both ends stamped in Rust on one clock; the frontend echoes an id back after a
double `requestAnimationFrame`. The span is *hotkey handler entry to the IPC call
following the committed frame*: **includes** one IPC hop, **excludes** DWM's
final present. The margin is wide enough that the excluded part cannot close it.
The 240fps calibration capture (step I5 of the manual script) is still owed and
converts these into a claim about what the eye sees.

Reproduce with `scripts\bench-idle.ps1`, which writes a transcript, the memory
curve, the raw records and a summary into `bench\results\`.

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
