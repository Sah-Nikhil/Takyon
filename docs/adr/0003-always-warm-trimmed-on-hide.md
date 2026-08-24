---
status: accepted
---

# The Palette window stays warm and is trimmed on hide

To show the Palette within tens of milliseconds of the hotkey, its WebView2
instance must already exist and be rendered before the user presses anything. We
keep exactly one Palette window alive for the process lifetime, hidden rather than
destroyed, and release its working set when hidden so idle memory falls back
toward the floor. Creating the window on demand was rejected: WebView2
initialisation costs hundreds of milliseconds to seconds, which loses the only
race this product is trying to win.

## Considered Options

- **Always cold** (create on hotkey): idle RSS around 15 MB, but misses the
  latency budget by roughly an order of magnitude.
- **Warm on signal** (pre-warm on a modifier press, a keystroke burst, or during
  active hours): keeps most of the speed at lower average RAM, but adds a
  heuristic that is wrong sometimes, and being wrong looks exactly like being
  slow.
- **Always warm, untrimmed**: simplest, but leaves ~100 MB resident permanently
  for a tool that is idle 99% of the time.

## Consequences

Latency was chosen over idle RAM deliberately: users feel latency on every single
invocation and check memory once.

**Measured 2026-08-24 (v0.1), and the conclusion stands while one premise does
not.** First show after 35 minutes idle: **22.8 ms**, against a 50 ms budget and
indistinguishable from a warm show — so trimming costs nothing on the show path,
which is the claim that mattered.

But the sentence above about trimming recovering "most of the memory anyway" is
wrong over the timescales users actually idle. The trim does drop the process
tree to ~12 MB, and WebView2 faults it back to **~107 MB within two minutes**,
where it rests. Trimming caps the peak; it does not keep the process small. The
150 MB budget is still met, with roughly 40 MB of headroom.

Full curve, method and the amendment this implies: `docs/tbc/0002`. Revisit with
all four strategies is still scheduled — see `docs/plans/post-v1.md`.

Because the window is warm, any frontend framework's mount cost is paid once at
boot into a hidden window and never on the hot path. This materially weakens the
usual argument for picking the smallest possible framework.
