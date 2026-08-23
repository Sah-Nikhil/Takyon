---
status: watching
pairs-with: ADR-0007
---

# TBC-0005 — Which locations we index by default

## The bet

We walk `Desktop`, `Documents`, `Downloads`, `Pictures`, `Videos`, `OneDrive` and
`C:\Programming`, excluding `node_modules`, `.git`, `target`, `dist`, `build`,
`.next`, `venv`, `__pycache__`, `AppData`, `$Recycle.Bin` and `Program Files`.
Both lists are user-editable, and settings display a live entry count.

The assumption: **a curated subset beats total coverage**, because the excluded
locations are overwhelmingly build artefacts and application internals that
nobody searches for by name but which would dominate the index by volume. The
reference point is Raycast, which indexes ~289k entries out of the 1.8M files
present in this machine's profile and ships that as a finished feature.

This is the least-evidenced product decision in the file-search design. Unlike the
others it has no benchmark — only a competitor's choice and an intuition about
what people search for.

## How we'd know we were wrong

- Users regularly search for files that exist but aren't indexed, i.e. the "it's
  broken" failure. Watch for `!e` queries returning nothing where the file
  demonstrably exists.
- Entry count exceeds **~400k** on a normal machine, suggesting an exclusion rule
  is missing and junk is crowding out real results.
- Entry count is under **~20k**, suggesting the roots are so narrow the feature
  can't justify its existence.
- The initial walk exceeds 60 s, or the on-disk index exceeds ~150 MB.
- `C:\Programming` turns out to be a personal-workflow assumption that doesn't
  generalise to other users — likely, and the first thing to revisit if this ships.

## Alternatives

| Option | Improvement if we switch | Added complexity | Switching cost |
|---|---|---|---|
| **Whole user profile, exclusions only** | Nothing is ever missing | Low code; index grows several-fold and relevance degrades | **1 day** — it's a default-value change. The cost is quality, not effort |
| **Ask on first run** ("which folders should I search?") | Defaults become the user's problem rather than ours, and they learn the feature exists | Medium — a first-run flow to design and build | **2–3 days.** Risks a decision-shaped wall in front of a new user |
| **Learn the roots** — watch which folders opened files come from and offer to index them | Defaults improve on their own; no guessing | Medium-high — needs usage tracking and a suggestion UI | **4–6 days.** The best long-term answer, and much easier once Frecency exists |
| **Whole volume via MFT** | Total coverage, instant index | High — a service, elevation, NTFS internals (see TBC-0003) | **15–25 days.** Rejected in ADR-0007 |

## Verdict if triggered

If files are being missed, **widen the roots and show the entry count more
prominently** before anything else — this is a settings-default change, not an
architecture change. The learned-roots option is where this should eventually go,
and it becomes cheap once `frecency.db` exists, since the signal it needs is
already being collected for ranking.
