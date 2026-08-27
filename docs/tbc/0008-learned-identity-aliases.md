---
status: watching
pairs-with: ADR-0014
---

# TBC-0008 — Learning that two Entries are one application

## The bet

Some duplicate rows cannot be collapsed by any rule that looks only at names.
`explorer` beside **File Explorer**, `mspaint` beside **Paint**, `SnippingTool`
beside **Snipping Tool**: two Sources, two legitimate ids, one application. v0.2
settled these by curation — `SHIMS_TO_PACKAGED_APP` is a hand-written list — and
[`../tbd/v0.2.md`](../tbd/v0.2.md) §4a explicitly left the differently-spelled
pairs alone, because a fuzzier title match eventually hides a real tool.

The bet here is that **the collapse can be learned from evidence rather than
declared in a list**, and that the two pieces of evidence worth trusting are:

- **Identical icon bytes.** Two Entries whose extracted icons are byte-identical
  are almost always one application. The icons are already extracted and already
  in memory, so the signal is free.
- **The same process actually starting.** After an activation, the process that
  appeared has an image path. Two Entries that produce the same image path are
  one application, and this is not a heuristic — it is what happened.

The assumption that makes it correct: **a collapse justified by evidence is safe
where a collapse justified by spelling is not.** §4a's fear was a fuzzy name rule
hiding a real tool; neither signal above can fire on two tools that are genuinely
different, because two different tools do not ship the same icon bytes and do not
start the same executable.

The second assumption is that it stays cheap. Nothing runs on the query path.
Icon comparison happens where icons are already being extracted; process
observation happens only after the user has launched something, which is the one
moment the machine is already busy doing something much more expensive.

## Shape, so the cost is arguable rather than vague

Storage is a table in `frecency.db` — this is learned usage data, it belongs with
the other learned usage data, and it must not ship alongside the binary because
the conflicts are per-machine. Nothing is deleted: the losing Entry is
**suppressed** from the list and its Frecency merged into the winner, so a wrong
collapse costs a row rather than a history. Which one wins is not a new decision —
[ADR-0014](../adr/0014-durable-identity-wins-a-collision.md) already says the more
durable id does.

Two guards, both load-bearing:

- **A collapse needs the same evidence twice.** One observation can be wrong —
  the user launched two things in the same second, or a Squirrel stub exited and
  handed off to its child. Requiring a repeat costs a few days of use and removes
  the whole class of one-off misattribution.
- **A suppressed Entry is recorded, not forgotten.** It stays visible in
  diagnostics with the evidence that suppressed it, so "why did my app
  disappear?" has an answer that is one command away rather than an archaeology
  problem. This is the specific failure that makes learned behaviour frightening,
  and it is cheap to prevent.

## Measured 2026-08-27, before building: the icon signal is noisy

Taken from the real `icons.bin` the moment task 0 made it real. 99 extracted
icons:

| | |
|---|---|
| Distinct icon bytes | 71 |
| Groups sharing bytes | 13 |
| Icons inside a shared group | **41 of 99** |
| Largest group | **8 icons** |

So **byte-identical icons are common, not rare**, and the naive form of this
signal is wrong. The largest groups are generic shell icons — the console icon
that every `cmd.exe`-hosted prompt inherits is the obvious candidate at eight,
and those nine prompts are exactly the applications task 0 spent its effort
*separating*. Collapsing on icon identity alone would undo §9 by another route.

The signal survives in a narrower form, and the measurement hands us the rule:
**an icon shared by three or more Entries is generic and carries no identity**.
It is a property of the corpus, not a threshold anyone has to tune — a real
application's icon is its own. That leaves the pairs, of which there are four
here, and each still needs corroboration before anything is collapsed.

This is the cheap half of the feature doing its job before a line of it was
written. It does not kill the note; it removes the version of it that would have
shipped a regression.

## How we'd know we were wrong

- **A real application disappears** and the alias table is why. One occurrence is
  a trigger, not a tuning problem — it is exactly §4a's stated fear arriving by a
  different route.
- ~~**Icon identity turns out to be common between unrelated apps.**~~ **Fired
  before building — see the measurement above.** 41 of 99 icons share bytes with
  another. The signal is kept only with the generic-icon exclusion; if a pair
  still collapses wrongly under that rule, the signal is dead and only process
  observation survives.
- **Process attribution is wrong more than rarely.** Measurable the same way:
  record the mapping without acting on it for a week, then read the table.
- **It stops being free.** Any measurable cost on the query path, or a resident
  cost against the 150 MB idle ceiling, kills it — the feature is worth one
  redundant row, not one millisecond of latency.

## Alternatives

| Option | Improvement if we switch | Added complexity | Switching cost |
|---|---|---|---|
| Leave it (v0.2's answer) | none; one redundant row per pair, and Frecency sorts which one you reach | none | 0 d |
| Curated list, per `SHIMS_TO_PACKAGED_APP` | exact, no false positives, works from first launch with no learning period | a list somebody maintains forever, and it only ever covers this machine's pairs | 0.5 d |
| Name normalisation (drop common words, compare) | catches the whole class at once, no learning period | the §4a failure by construction: `code` is a substring of `Visual Studio Code` | 1 d |
| **Learned aliases (this note)** | catches pairs nobody predicted, on evidence rather than spelling | a table, a background observer, and a class of bug where the launcher hides something | 3–4 d |

## Verdict if triggered

If a real application is hidden: **suppress the suppression** — a single flag that
makes the alias table advisory (shown in diagnostics, applied to nothing) rather
than ripping the feature out. That keeps the collected evidence, which is the
expensive part, while restoring the rows immediately.

If icon identity proves noisy: drop that signal and keep process observation
alone. It is the stronger of the two and the one that cannot be coincidence; the
cost is that a pair is only learned once both halves have been launched.

If the whole thing proves not worth it: the fallback is the curated list, which is
half a day and already has a working precedent in `steam.rs` and `path.rs`.

## Why it is not built yet

It needs `frecency.db`, which is v0.3 task 1, and it needs somewhere to merge a
losing Entry's usage into a winner's, which is task 3. Building it before those
means inventing storage twice. It is v0.3 task 1b in
[`../plans/v0.3-ranking.md`](../plans/v0.3-ranking.md).

**Measure first, and the measurement is nearly free**: the icon-collision count
can be taken today, from the blob that task 0 just made real.
