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

### What faults the pages back — checked, and it is not our code

The obvious suspect was the idle mark animation: if it kept running against a
hidden window, WebView2 would keep compositing something nobody can see, and that
would explain both the refill and a power cost.

**It was checked and ruled out.** `Palette.tsx` subscribes to the hide event and
clears its `shown` state, which removes the `data-cone-sweep` and
`data-particle-pulse` attributes the animation is bound to. The pulse genuinely
stops while hidden.

So the refill is WebView2's own background work — timers, garbage collection,
compositor bookkeeping — not something this codebase controls. That matters for
what can be done about it: the ~107 MB resting figure is a property of hosting a
browser engine, and the only lever that moves it is the native-Palette option in
the alternatives table below, not a fix in our code.

### One thing worth re-checking at v0.2

**7 processes at rest.** Expected for WebView2, and it is the count every future
memory number scales from. If it changes once Sources exist, every comparison
against these figures needs redoing.

## Measured — v0.2, 2026-08-25

Same machine. Release build, `bun run bench --runs 100 --alt-hotkey`, with one
Source (applications: 1078 discovered), icons, and a content-sized window.

| Metric | Budget | v0.1 | v0.2 | Verdict |
|---|---|---|---|---|
| Hotkey to first pixel | < 50 ms | p95 **22.6** (n=30) | p50 22.6 · p95 **25.0** · max 26.4 (n=100) | PASS |
| Keystroke to first Entry | < 30 ms | not measurable | p50 13.2 · p95 **17.7** · max 31.0 (n=100) | PASS |
| Process start to hotkey responsive | < 500 ms | **311.6 ms** | **254.4 ms** | PASS |
| Idle RSS, warm and trimmed | < 150 MB | ~107 MB steady | **37.1 MB** working set, 250.8 MB committed, 8 processes | PASS |

Four things worth stating plainly rather than leaving to be inferred:

- **First pixel moved 22.6 → 25.0 ms at p95.** A real regression, and an expected
  one: the Palette now mounts a list, a row renderer and an action menu where v0.1
  mounted an input. Half the budget is still unused. Worth watching rather than
  acting on.
- **The first-Entry budget starts here**, because v0.2 is the first phase with a
  Source to produce an Entry. What is timed is *keystroke to the frame that drew
  its Entries* — the Palette opens empty by design (ADR-0001), so there is nothing
  to draw until something is typed. `p95` was 19.7 ms before the icon key was
  hoisted out of the query path; leaving it lazy meant twelve `fs::metadata` calls
  per keystroke.
- **The process count went 7 → 8.** As this section predicted it might. Memory
  figures before and after this phase are therefore not directly comparable.
- **The 37 MB working-set figure is not evidence the ~107 MB finding was wrong.**
  It is a single reading taken three seconds after a hide, whereas the v0.1 number
  came from a ten-minute curve, and the *committed* figure went the other way
  (196 → 251 MB). The honest reading is that trimming still works and the resting
  curve has not been re-measured at v0.2. Anyone quoting a memory number for this
  phase should run `scripts\bench-idle.ps1` first.

**`Alt+Space` was unavailable on this machine** — Raycast holds it — so the run
used `--alt-hotkey`, which registers `Ctrl+Alt+F9` instead. Only the chord differs;
the path from hotkey handler to first pixel is identical.

### What the latency numbers include

Both ends stamped in Rust on one clock; the frontend echoes an id back after a
double `requestAnimationFrame`. The span is *hotkey handler entry to the IPC call
following the committed frame*: **includes** one IPC hop, **excludes** DWM's
final present. The margin is wide enough that the excluded part cannot close it.
The 240fps calibration capture (step I5 of the manual script) is still owed and
converts these into a claim about what the eye sees.

Reproduce with `scripts\bench-idle.ps1`, which writes a transcript, the memory
curve, the raw records and a summary into `bench\results\`.

## Measured — v0.7, 2026-09-04

Same machine. Release build, `bun run bench --alt-hotkey` (n=30), with the file
index now built at boot and four `ReadDirectoryChangesW` watchers running
throughout the run.

| Metric | Budget | v0.2 | v0.7 | Verdict |
|---|---|---|---|---|
| Hotkey to first pixel | < 50 ms | p95 **25.0** (n=100) | min 21.8 · p50 25.1 · p95 **30.4** · max 30.7 (n=30) | PASS |
| Keystroke to first Entry | < 30 ms | p95 **17.7** (n=100) | min 10.9 · p50 15.0 · p95 **20.7** · max 36.7 (n=30) | PASS |
| Process start to hotkey responsive | < 500 ms | **254.4 ms** | **308.4 ms** | PASS |
| Idle RSS, warm and trimmed | < 150 MB | 37.1 MB working set, 250.8 MB committed, 8 processes | **28.4 MB** working set, 250.0 MB committed, 7 processes | PASS |

Three things worth stating plainly:

- **First-Entry p95 is 20.7 ms, 69% of its budget.** Five phases of Sources sit on
  that path now — applications, system entries, recents, the calculator and
  commands — and the sample is 30 rather than 100, so the p95 is drawn from a much
  smaller tail; the max of 36.7 ms is over budget and a larger sample would move
  the p95 toward it. Re-measure at n=100 before v1.0. This is the first budget
  that will fail if a Source is added carelessly.
  The file Source is **not** among the contributors here: it is off by default
  Bangless (`files.bangless`), so it returns before doing any work.
- **Startup moved 254 → 308 ms** with the index mapped and the watchers started.
  Both happen *below* `hotkey::register`, which is where they were deliberately
  put: resolving the roots costs 3.5 ms of shell calls and mapping the index
  costs 87 µs, and neither joins the queue above registration.
- **The process count went 8 → 7** and the working set fell. Neither is a
  file-index effect worth reading into — WebView2's process count varies by
  version and by what the machine was doing, and the committed figure barely
  moved (250.8 → 250.0 MB).
- **Whole-drive scope costs nothing here.** These numbers are from the build that
  indexes 309,802 entries across both fixed drives, with watchers on each. The
  index is mapped, not loaded, so its size does not reach the working set, and the
  walk is off the startup path entirely.

**One flake worth recording**, because it will look alarming next time: an earlier
run of the same binary died at show 14 of 30 with `timed out waiting for the
Palette to report a painted frame`, preceded by Chromium's `Failed to unregister
class Chrome_WidgetWin_0. Error = 1412`. No panic log was written, and an
immediate re-run of the same build completed 30/30 with the numbers above. Treat a
single mid-run timeout as a flake; treat two in a row as a regression.

`--alt-hotkey` again, for the same reason: Raycast holds `Alt+Space` here.

**And the trap that cost two runs:** `cargo test --release` leaves a `takyon.exe`
in `target\release\` with no frontend embedded, and the bench runs whatever binary
is there. It reports "timed out waiting for the Palette to report a painted
frame", which reads exactly like a rendering regression. Always `bun run build`
immediately before benching.

## Measured — v0.10, 2026-09-06

Same machine. Release build, `bun run bench --alt-hotkey` (n=30). **Twice**: once
in Compact against the real profile, and once in Expanded against a scratch
`LOCALAPPDATA` seeded with `ui.window-mode=expanded`, which is the first time
anyone has measured the second mode at all (`docs/tbd/v0.10.md` §9, now closed).

| Metric | Budget | v0.2 (n=100) | v0.7 (n=30) | v0.10 (n=100) | v0.10 Expanded (n=30) | Verdict |
|---|---|---|---|---|---|---|
| Hotkey to first pixel | < 50 ms | p95 **25.0** | p95 **30.4** | min 19.8 · p50 22.2 · p95 **25.9** · max 31.5 | min 23.5 · p50 27.9 · p95 **34.8** · max 39.3 | PASS |
| Keystroke to first Entry | < 30 ms | p95 **17.7** | p95 **20.7** | min 11.0 · p50 14.0 · p95 **19.3** · max 38.3 | min 6.9 · p50 8.9 · p95 **14.0** · max 34.6 | PASS |
| Process start to hotkey responsive | < 500 ms | **254.4 ms** | **308.4 ms** | **260.3 ms** | **341.3 ms** | PASS |
| Idle RSS, warm and trimmed | < 150 MB | 37.1 MB | **28.4 MB** | **32.7 MB** / 251.7 committed, 7 procs | **27.6 MB** / 242.8 committed, 7 procs | PASS |

The Compact column is post-guard; the Expanded column is not. See the third note.

- **The phase shipped a first-pixel regression and then removed it, and the
  removal is worth more than the regression cost.** `prefs.refresh()` runs on
  every show — that is what keeps the two windows in step without cross-window
  plumbing — and it now reaches `applyTheme`, which wrote seven custom properties
  onto `<html>` plus a synchronous `localStorage` write. Seven custom properties
  on the root invalidate computed style for the whole document, every summon,
  almost always to repaint the colour already painted. Measured at n=100:

  | | pre-guard | post-guard |
  |---|---|---|
  | first pixel p95 | 35.7 ms | **25.9 ms** |
  | first Entry p95 | 22.4 ms | **19.3 ms** |
  | start to hotkey | 405.4 ms | **260.3 ms** |

  `applyTheme` now returns early when the painted half *and* the stored choice
  are both unchanged. That is the whole fix, and it leaves every budget better
  than v0.7 measured them — first-pixel is back under v0.2's 25.0 to within a
  millisecond after eight phases of features.
- **n=30 lied about first-Entry.** A first n=30 Compact run read p95 **29.2 ms**,
  97% of budget, which looked like a regression this phase had caused. At n=100
  it is 19.3. v0.7 asked for n=100 before v1.0 and was right to; treat any n=30
  first-Entry reading as indicative only.
- **Expanded is not measurably slower where it counts, but its column is stale.**
  34.8 ms first-pixel p95, taken on the pre-guard build against a scratch
  `LOCALAPPDATA` with no Frecency and no clipboard history — so its first-Entry
  figure measures a lighter machine and **is not comparable** on that row. The
  honest reading is that painting 520px instead of 68px cost roughly nothing
  detectable; re-measure post-guard before quoting it. `docs/tbd/v0.10.md` §9.

`--alt-hotkey` again: Raycast holds `Alt+Space` on this machine.

## Measured — v0.10.1, 2026-09-06

Same machine, same session, n=100 Compact. Re-run because v0.10.1 dropped
`backdrop-blur-xl` from the Palette panel — the blur could never have worked
(the window is `transparent: true`, so there is no backdrop to sample) but it
still forced a backdrop-filter compositing pass on every paint.

| Metric | Budget | v0.10 (n=100) | v0.10.1 (n=100) | Verdict |
|---|---|---|---|---|
| Hotkey to first pixel | < 50 ms | p95 **25.9** | min 19.5 · p50 22.1 · p95 **25.6** · max 26.8 | PASS |
| Keystroke to first Entry | < 30 ms | p95 **19.3** | min 4.1 · p50 7.8 · p95 **12.0** · max 69.4 | PASS |
| Process start to hotkey | < 500 ms | 260.3 | **257.7** | PASS |
| Idle RSS | < 150 MB | 62.6 | **59.8** (249.1 committed) | PASS |

- **First-Entry p95 fell 19.3 → 12.0 ms, and p50 halved: 14.0 → 7.8.** That is
  the backdrop-filter, and the size of it is the surprise. First pixel barely
  moved, because the panel paints once there; typing repaints the list *under*
  the filtered surface on every keystroke, which is where the pass was being
  paid for. 24% of a budget recovered by deleting a declaration that did nothing.
- **`max` is the only untidy number** — 69.4 ms against a 12.0 p95, up from 38.3.
  One outlier in 302 keystrokes while the p95 more than halved, so this reads as
  a scheduling stall rather than a slow path. Worth a second look only if p95
  starts drifting toward it.
- **The Expanded column is still stale**, and still pre-guard. Unchanged from the
  note above: `docs/tbd/v0.10.md` §9.

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
