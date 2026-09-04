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

---

## Amended again at v0.7 — whole fixed drives, not curated folders

The curated bet above is retired. **Every fixed drive is a root**, with a widened
exclusion list. The trigger this note wrote for itself fired within a day of the
feature existing: three separate folders a user could see in Explorer and `!e`
could not find — `C:\Data\0Projects\Create\HH`, `C:\GG\FitGirl\EA SPORTS FC 26`,
and `C:\FC 26 Live Editor`. All were top-level folders outside every curated root.

The measurement that settled it, on the development machine:

| Scope | Entries | Walk | Index |
|---|---|---|---|
| Curated roots | 26,844 | 0.9 s | 2.5 MB |
| Whole `C:\` + `D:\` | **309,802** | **2.3 s** warm, 7.8 s cold | **24.6 MB** |

Against budgets of 60 s and ~150 MB, that is 13% of the time and 16% of the size.
The assumption underneath the original bet — that total coverage was unaffordable
— was simply wrong, and it was wrong because the exclusion list does the work.
The excluded set is where the volume lives, not the included set.

Note also how close 309,802 sits to Raycast's 288,592, the reference point
ADR-0007 used to argue for curation. The competitor was not indexing a curated
subset of a large machine; it was indexing most of one, minus its junk.

### What changed

- `roots::fixed_drives()` enumerates drives through `GetLogicalDrives` and keeps
  those reporting `DRIVE_FIXED`. Removable and network drives are excluded: a USB
  stick would be walked once and then found missing, and a mapped drive puts the
  walk on the network.
- The exclusion list grew from 12 names to 33 — `Windows`, `ProgramData`, both
  `Program Files`, `System Volume Information`, `Recovery`, `PerfLogs`, plus
  build output (`.cxx`, `CMakeFiles`, `.gradle`, `Pods`, `DerivedData`) and
  caches (`appcache`, `librarycache`, `depotcache`, `shadercache`, `htmlcache`,
  `.cache`, `.nuget`, `.rustup`, `.cargo`). Steam's `librarycache` alone was
  answering two-letter queries with a page of texture hashes.
- Both remain user-editable, and the entry count in Settings is still the
  instrument for the triggers below.

### What this cost, and what it bought

The scope change exposed a latency regression that curation had been hiding:
worst-case query went from 568 µs to **20.8 ms**, against a 20 ms budget. Eleven
times the candidates meant eleven times the per-candidate work, and the query was
building a `Haystack` — a lowercased `String` and a `Vec<String>` of tokens — for
every candidate the trigram index returned, then reconstructing a full path for
every one that scored.

Two fixes, both in `live.rs`:

- **A prefilter before the `Haystack`.** Every rung a file can clear implies the
  name contains the needle, so `rank::contains_fold` rejects on raw bytes with no
  allocation. The acronym rung is the one casualty and it was never worth much on
  a filename.
- **Score ids, materialise paths last.** A common needle matches tens of thousands
  of candidates against twelve visible rows. Paths are now built only for the rows
  that survive the cut.

Result: **2.6 ms** worst case, 961 µs mean. 13% of the budget.

### The triggers, restated for this scope

The old lower bound is gone — 309k is not a narrow index. What remains worth
watching:

- **Entry count over ~1.5M**, where the index approaches the 150 MB ceiling at
  the measured ~83 bytes/entry.
- **A cold first walk over 60 s**, which the 7.8 s cold figure leaves room for but
  a mechanical disk or a much fuller drive would not.
- **A query worst case back over 20 ms.** The prefilter is what holds this, and
  it holds it by a factor of eight rather than a margin.
- **Results that feel noisy rather than absent.** The failure mode has inverted:
  the old risk was missing files, the new one is a top row that is technically a
  match and obviously not what was wanted.
