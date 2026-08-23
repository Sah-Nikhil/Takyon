# TBC notes — decisions we expect to re-examine

An ADR records a decision and why it was made. A **TBC note** records the
*assumption underneath* a decision, the observable signal that would prove that
assumption wrong, and what switching would actually cost if it does.

They exist because the expensive mistake is not choosing wrong — it's choosing
wrong, finding out eighteen months later, and having no record of what the
alternatives were or why they were passed over. By then the reasoning is gone and
the switch gets estimated from scratch, usually badly.

## When to write one

Write a TBC note when a decision is **load-bearing but made on reasoning rather
than measurement**, or when it rests on an assumption that could plausibly turn
out false. A decision that is easy to reverse doesn't need one. Neither does a
decision that was measured — that one just needs the numbers in the ADR.

Most TBC notes pair with an ADR. The ADR says what we did; the TBC note says what
would make us undo it.

## Format

```md
---
status: watching | triggered | resolved | retired
pairs-with: ADR-0003
---

# TBC-000N — {the component}

## The bet
{What we chose, and the assumption that makes it correct. One paragraph.}

## How we'd know we were wrong
{Observable, ideally numeric triggers. Not "if it feels slow" — a threshold.}

## Alternatives

| Option | Improvement if we switch | Added complexity | Switching cost |
|---|---|---|---|

## Verdict if triggered
{What we'd actually do, so the decision is half-made before the crisis.}
```

Switching costs are estimates in developer-days by a single developer already
familiar with the codebase. They are guesses; treat them as orders of magnitude,
not commitments. Update them when reality disagrees.

## Status values

- **watching** — the bet stands, no trigger has fired
- **triggered** — a trigger fired; the note is now a live decision
- **resolved** — we switched, or measured and confirmed; say which
- **retired** — no longer relevant (the component is gone, or the risk evaporated)
