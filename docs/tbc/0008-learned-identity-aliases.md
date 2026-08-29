---
status: retired
pairs-with: ADR-0014
built: v0.3 task 1b, 2026-08-29
retired: 2026-08-29
---

# TBC-0008 — Learning that two Entries are one application

> **Retired the day it shipped.** Built, measured, and then removed in favour of
> static index-time rules that are correct on first launch instead of after two
> learned launches. The two cases that actually occur are handled without it:
> a Windows-dir binary the shell already lists as an app is dropped at discovery
> (`path.rs` `WINDOWS_DIR_APP_DUPLICATES` — `explorer` joins `calc`/`notepad`),
> and two genuinely different same-named executables stay two rows disambiguated
> by version (`version.rs`). The learned-collapse machinery — `collapse.rs`, the
> `launched`/`collapsed` tables, `collapses.txt`, the four-launch flow — is gone.
> Neither Raycast nor PowerToys learns-and-merges either
> ([`../prior-art/ranking-and-dedup.md`](../prior-art/ranking-and-dedup.md)); the
> reason to retire was the counterintuitive fresh-machine behaviour it required.
> The note below is kept as the record of what was tried and why it did not last.

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

## Built 2026-08-29, and the corpus forced two more guards

The note said the generic-icon rule left "the pairs, of which there are four
here, and each still needs corroboration". Both halves of that turned out to be
wrong once the rule was run over the whole corpus rather than over `icons.bin`
alone.

### The measurement that changed it again

`v0_3_measure_the_real_icon_pairs` — `cargo test --test integration -- --ignored`
— joins the real application walk against the real `icons.bin`. 1321
applications, 272 cached icons, 259 Entries carrying one. The rule as written
produced **eleven** candidate pairs, not four. One was a genuine duplicate:

```
aumid:Microsoft.Windows.AdministrativeTools
c:\windows\system32\control.exe|/name microsoft.administrativetools
```

Three of the other ten were dangerous rather than merely wrong, because process
observation would have **agreed** with them:

| Pair | What it actually is |
|---|---|
| `rar.txt` + `whatsnew.txt` | two documents sharing the generic text icon — and both start `notepad.exe` |
| `winrar.chm` + `hh.exe` | a help file and its viewer; the `.chm` *is* started by `hh.exe` |
| `uwpappssamples.url` + `uwpappstoolsdocumentation.url` | generic `.url` icon, and both start the browser |
| `wsl.ico` + `wsl.ico\|--cd ~` | one binary, two argument sets — **exactly what task 0 separated** |

That last one is the serious one. Corroboration cannot save a pair whose two
halves genuinely start the same executable, and "two shortcuts to one host binary
with different arguments" is precisely that case — the case ADR-0014 was amended
to protect. Left alone, this feature would have undone task 0 by a third route.

### The two guards that follow

- **Only an executable's icon can identify it.** A document wears its file
  type's icon, so `.txt` matches `.txt` and `.url` matches `.url`, and both
  halves start the same viewer. `icon_can_identify` keeps `.exe`, AUMIDs and
  Steam ids and drops everything else.
- **Two ids differing only by arguments are never collapsed.** Checked in
  `pairs_by_icon` and again in `CollapseStore::collapses`, because this is the
  guard that keeps task 0 intact and it must hold however a pair arrived.

Re-measured: **eleven candidates became seven.** The document cases and the
argument case are gone. Of the seven that remain, one is the genuine
`AdministrativeTools` duplicate and six are pairs of genuinely different
executables — `devicepairingwizard` beside `hdwwiz`, 64-bit `powershell_ise`
beside the 32-bit one, two Node installs, two AMD tools, two Docker binaries, two
Visual Studio graphics engines. Every one of those six is stopped by
corroboration, because their halves start different images.

**So the icon signal on its own is worth roughly one true positive in seven.**
It is a narrowing filter and nothing more, which is what this note already
suspected and can now say with a number.

### What is built

- `src/collapse.rs` — the store (`launched`, `collapsed`, both in `frecency.db`),
  the two signals, the guards, the winner rule and `learn`, which runs after each
  walk on the discovery thread. Nothing on the query path.
- `Frecency::merge_at` — the loser's decayed weight folds into the winner and the
  loser's row is deleted. Merged once, at the moment a collapse is first decided;
  the `collapsed` primary key is what makes that idempotent.
- `AppSource::apply_collapses` — in place after each walk, beside
  `apply_aliases`, for the same reason: discovery rebuilds the list and would
  otherwise bring the suppressed Entry back.
- `collapses.txt` beside `frecency.db`, rewritten whole on every startup, naming
  every hidden Entry, the one kept and the evidence. Written even when empty,
  because an absent file reads as a broken launcher.
- `launch::open` now goes through `ShellExecuteExW` with `SEE_MASK_NOCLOSEPROCESS`
  rather than `ShellExecuteW`, purely to learn the image path of what started.
  Same verb, file, arguments and show command.

### The winner rule

ADR-0014 still decides, sharpened by the evidence rather than replaced by it. In
order: an id with no path at all (AUMID, Steam) loses; a versioned path loses,
because it dies at the next update; the id that *is* the observed image wins,
because it is a real path and so supports reveal, elevate and copy. Otherwise the
first stands.

### The generic-icon rule blocks this note's own headline example

`explorer` beside **File Explorer** is the case the bet opens with. It cannot be
learned, and the reason is the guard rather than the evidence. Three Entries
match `explorer` here and all three wear the same folder icon:

```
File Explorer                     aumid:Microsoft.Windows.Explorer
explorer                          c:\windows\explorer.exe
Windows Software Development Kit  c:\windows\explorer.exe|"c:\program files (x86)\windows kits\10\"
```

Three sharers, so the icon reads as generic and the pair never surfaces. But the
third is not a third *binary* — it is `explorer.exe` opening a folder, which task
0 correctly gave its own id. **The rule counts Entries where it should count
distinct binaries.** Two argument-variants of one executable are one thing
wearing one icon, and counting them separately inflates every group they appear
in.

The fix is small and principled: canonicalise to `path_of` before counting group
size, so the group above becomes two and the genuine pair surfaces.
`differ_only_by_arguments` still refuses to collapse the SDK Entry with
`explorer`, so nothing task 0 separated is at risk. Not yet made — it changes
what the feature can hide, and the safety measurement should be re-run against
the new candidate list first.

Worth recording as a shape, not just a bug: **the two guards interact.** The
argument guard was added to stop a collapse; it also has to apply to the
generic-icon count, or it silently suppresses true positives instead.

### What is still unproven, and it is the important half

**No observation has ever been recorded on a real machine.** The launch path
changed but nothing has been launched through it, so it is not known whether
`ShellExecuteExW` returns a usable `hProcess` for a `shell:AppsFolder\` item.
If it does not, the AUMID half of the one genuine pair here can never reach
`OBSERVATIONS_REQUIRED`, and the feature is correct but permanently inert on this
machine. `docs/tbd/v0.3.md` §7 owns that question and `docs/verify/v0.3.md` §C
has the steps.

The safety property *is* proven, and by a test that runs in the default suite:
`v0_3_matching_icons_alone_never_hide_a_row` runs `learn` over the real machine
with an empty observation table and asserts that not one row disappears.

## How we'd know we were wrong

- **A real application disappears** and the alias table is why. One occurrence is
  a trigger, not a tuning problem — it is exactly §4a's stated fear arriving by a
  different route.
- ~~**Icon identity turns out to be common between unrelated apps.**~~ **Fired
  before building — see the measurement above.** 41 of 99 icons share bytes with
  another. The signal is kept only with the generic-icon exclusion; if a pair
  still collapses wrongly under that rule, the signal is dead and only process
  observation survives.
- ~~**Icon identity is noisy between unrelated apps.**~~ **Fired a second time,
  2026-08-29.** Over the whole corpus rather than `icons.bin` alone the rule
  produced eleven candidates for one true positive, three of them documents that
  process observation would have confirmed. Two further guards were added rather
  than dropping the signal; see above. **A third firing kills it** and leaves
  process observation alone.
- **Process attribution is wrong more than rarely.** Measurable the same way:
  record the mapping without acting on it for a week, then read the table.
  Nothing has been recorded yet — see `docs/tbd/v0.3.md` §7.
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

## Why it was not built until v0.3

It needs `frecency.db`, which is v0.3 task 1, and it needs somewhere to merge a
losing Entry's usage into a winner's, which is task 3. Building it before those
means inventing storage twice. It is v0.3 task 1b in
[`../plans/v0.3-ranking.md`](../plans/v0.3-ranking.md).

**Measure first, and the measurement is nearly free**: the icon-collision count
can be taken today, from the blob that task 0 just made real.
