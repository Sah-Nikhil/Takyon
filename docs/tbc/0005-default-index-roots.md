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

---

## Amended at v0.7 — the code root is probed, not hardcoded

The bet above named `C:\Programming` as a default root and flagged it, correctly,
as a personal-workflow assumption. It ships as a **probe** instead: at first index
Takyon tests a short candidate list and keeps whichever paths exist on disk —
`C:\Programming`, `%USERPROFILE%\source\repos` (Visual Studio's own default),
`~\dev`, `~\code`, `~\projects`, `~\git`, `~\repos`.

This machine still gets `C:\Programming`, another machine gets whatever its owner
actually uses, and nobody gets a default pointing at a folder that isn't there.
The cost is about twenty lines in the defaults and no new UI, against a revisit
this note already predicted would be the first one needed.

The candidate list is a guess like any other and falls under the same trigger
conditions as the rest of this note. What it removes is the *certainty* of being
wrong everywhere except one machine.

## The measurement that decided it

Windows Search was proposed as the thing that would cover code directories, on the
theory that the OS already indexes them. Measured on the development machine
against `SystemIndex` through the `Search.CollatorDSO` provider:

| Query | Result | Time |
|---|---|---|
| `SCOPE='file:C:/Programming'` | `.idea`, `.vscode`, `0dump`, `0vsc_setup`… | 12 ms |
| `SCOPE='file:C:/Programming/SELF'` | **zero rows** | 26 ms |
| `System.FileName LIKE 'takyon%'` | `D:\Takyon`, `D:\Takyon\takyon.exe` | 10 ms |

The crawl scope has `file:///C:\  include=1` — the whole drive is nominally
indexed, which is why Windows' own Start menu finds the `Programming` *folder*.
But `WorkingSetRules` also carries `include=0` for `Programming\SELF`,
`Programming\pitchr`, `Programming\NSE\*`, `Programming\Gigs\*`,
`Programming\MIT\*` and dozens more, so **not one source file of this repository
is in the Windows index**. Searching `takyon` in the Start menu returns the
installed build on `D:\` and nothing from the tree it was built from.

Two conclusions, both load-bearing for v0.7:

1. **Windows Search covering a directory is not something Takyon can rely on**,
   even on a machine where the whole drive is in scope. Per-folder exclusions are
   invisible until a query comes back empty, so "the OS already indexes it" is not
   a property any default can be built on. If code should be findable, Takyon has
   to walk it.
2. **The fallback cannot sit on the fast path.** Those queries took 10, 12, 26 and
   72 ms against a 20 ms p95 budget, on a warm service answering trivial filters.

A default-on fallback would also have been *worse* than none here: it returns some
results, so it reads as working, and the gap is never noticed.
