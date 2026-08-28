---
status: watching
pairs-with: IMPLEMENTATION_PLAN §3
---

# TBC-0009 — The two numbers that decide how fast ranking learns

## The bet

Ranking is governed by two constants, both chosen by reasoning rather than
measurement:

| Constant | Value | What it controls |
|---|---|---|
| `frecency::HALF_LIFE_DAYS` | 30 | How fast a launch stops counting |
| `rank::FRECENCY_LIFT` | 0.6 | The most usage can multiply a match score by, less one |
| `rank::FRECENCY_HALF` | 1.0 | The weight at which half the lift is reached |

Together they say: one launch is worth a lot, the hundredth almost nothing, an
Entry can climb about one rung of the matching ladder and no further, and a month
of not using something halves what it learned.

The assumption is that **a launcher should commit fast and forget slowly.** Fast,
because the whole promise is guessing right before you finish typing, and a
ranker that needs ten launches to notice is indistinguishable from no ranker for
the first week. Slowly, because the applications a person reaches for change over
months, not days.

## There is no telemetry to decide from

[ADR-0010](../adr/0010-no-telemetry-in-v1.md) rules it out for V1, so nothing is
being collected and no dashboard is coming. Every trigger below is either
something noticed in use or something read out of the local database:

```sql
-- What the ranker currently believes, most-trusted first.
SELECT entry_id, count, score, datetime(last_used, 'unixepoch')
FROM usage ORDER BY score DESC LIMIT 20;

-- Applications launched once and never again: candidates for "it committed too fast".
SELECT entry_id, datetime(last_used, 'unixepoch') FROM usage WHERE count = 1;
```

That is the whole instrument. It is enough, because the failure modes below are
things a person notices rather than things a percentile reveals.

## How we'd know we were wrong

**Too eager** — the lift is too large, or `FRECENCY_HALF` too small:

- Typing an application's *exact full name* does not put it first, because
  something used more often outranks it. One occurrence is a bug rather than a
  tuning question: an exact name is an unambiguous request.
- A tool launched once, months ago, still sits above one used weekly.
- The top row for a common prefix stops changing even as habits change.

**Too sluggish** — the lift is too small, or the half-life too long:

- After a week of real use, the ten most-used applications are *not* each
  reachable in one or two keystrokes. This is v0.3's own exit criterion, so
  failing it is already a phase-level signal rather than a new one.
- A newly installed application stays buried under something you have stopped
  using entirely.

**Half-life specifically:** if `count` keeps climbing on rows whose applications
you no longer recognise, 30 days is too long. If applications you use every
fortnight keep falling out of the top row, it is too short.

## Alternatives

| Option | Improvement if we switch | Added complexity | Switching cost |
|---|---|---|---|
| **Leave them** | none; they are guesses that may simply be right | none | 0 d |
| Re-tune the constants | ranking matches this user's habits | none — three literals | **0.5 d**, plus the caveat below |
| Store raw launch timestamps instead of a decayed scalar | re-tuning becomes exact and retroactive; any half-life can be applied to full history | one row per launch instead of one per Entry, and a sweep to bound it | 1–2 d |
| Per-kind constants | a document and an application need not learn at the same rate | four more numbers to justify, and no evidence yet that they differ | 1 d |

### The caveat, and it is the reason this note exists

**Scores are stored already decayed.** `record_at` decays the stored value
forward and adds one; `weight_at` decays it again on read. That is what makes the
lazy scheme work with no background job — but it also means the half-life is
baked into every value already written.

Change `HALF_LIFE_DAYS` and the stored numbers are not re-interpreted, they are
*mis*-interpreted: a score that decayed for a month under 30 days, then read
under 60, is not the value it would have had if 60 had always been set. The error
is bounded, always in the direction of the old setting, and washes out as new
launches land — but it is real, and it means a re-tune is not a clean experiment.

Two honest ways to handle it, if the tuning ever matters enough:

- **Accept the drift.** Re-tune, and treat the first fortnight afterwards as
  unreliable rather than as evidence about the new value.
- **Reset the database.** Deleting `frecency.db` makes the new constant exact
  from a clean start, at the cost of everything learned. For a personal launcher
  with a 30-day half-life, that is a fortnight of mild annoyance, not a
  catastrophe.

The third option — storing raw timestamps — removes the problem entirely and is
the row above. It is the right answer if these constants turn out to need more
than one adjustment.

## Verdict if triggered

Change one constant at a time and give it a fortnight. Changing both at once
means learning nothing, because they push in the same direction and the evidence
is qualitative.

Start with `FRECENCY_LIFT` for anything about *ordering* and with
`HALF_LIFE_DAYS` for anything about *forgetting* — the two failure modes above
map cleanly onto one constant each, which is the one genuinely useful property of
having kept them separate.

Record the change here with the date and what prompted it, so the next
adjustment has a history rather than a fresh opinion.
