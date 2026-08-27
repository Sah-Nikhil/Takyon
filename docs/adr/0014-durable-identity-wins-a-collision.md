---
status: accepted
---

# When two Sources produce one application, the more durable identity wins

`EntryId` is both the launch target and, from v0.3, the Frecency key. Those two
jobs pull in different directions: the launch target wants to be whatever starts
the application right now, and the Frecency key wants to be the same string in six
months. Where they disagree, **the durable one wins**, and the Source holding it
outranks the Source that does not.

This is a single rule with several instances, and it was written three times
before it was written down once.

## Why identity is the thing that matters

An unstable `EntryId` fails silently. Nothing errors, no test goes red: the
application simply arrives one morning with its usage history reset, and `vsc`
stops finding VS Code because the ranker no longer knows it was ever launched. By
the time it is noticed the cause is weeks in the past.

Duplicates are the visible half of the same problem. Two rows for one application
mean two ids, so every launch teaches only half of it, and neither half ever
reaches the top.

## The rule, and the collisions it has decided

Where two Entries denote one application, keep the one whose id survives an
update, an uninstall-reinstall, or a move.

| Collision | Winner | Because |
|---|---|---|
| Squirrel: `app-1.0.9253\Discord.exe` vs `Update.exe` | the stub | the versioned path dies at the next update, taking the Frecency with it |
| `AppsFolder` AUMID vs a Start Menu shortcut of the same title | Start Menu | it has a real path, so it also supports reveal, elevate and copy path |
| `PATH` shim vs the packaged app it starts | packaged app | `calc.exe` and `notepad.exe` exit immediately having launched the real thing |
| Desktop shortcut vs Start Menu (v0.3 task 11) | Start Menu | desktop icons are deleted casually; the id must not go with them |
| Epic manifest (v0.3 task 9) | `epic:<AppName>` | the catalog GUID survives a library move; the install path does not |
| Two shortcuts to one host binary with different arguments | **both**, id includes the arguments | not a collision at all — see the amendment below |

The first three shipped in v0.2. Two of them were found by a user reporting
duplicate rows, not by a test, which is the honest reason this ADR exists.

## Consequences

**Source order is load-bearing, not cosmetic.** The per-user Start Menu is walked
before the machine-wide one, and Desktop after both, because "first found wins" is
the tiebreak once durability is equal. Any new Source must declare where it sits
in that order, and a Source added carelessly at the front will quietly take over
ids that belonged to a better one.

**Dedupe is by title, which is fuzzy on purpose.** Titles are what collide in
practice and what the user sees. The cost is that differently-spelled pairs
survive: `mspaint` beside "Paint", `SnippingTool` beside "Snipping Tool". They are
left alone deliberately — a fuzzier match would eventually hide a real tool, and
Frecency sorts the pair out within a few uses.

**Every Source needs an existence check.** A durable id is worth nothing if it
points at a file that is gone. `lnk.rs` checks, and ADR-0013 explains why it does
so by testing the path rather than by calling `Resolve`. The Epic Source must do
the same: on the development machine all seven manifests are stale, and a
competitor that trusts them shows seven rows that cannot launch.

## Amended by evidence, 2026-08-27 — landed in v0.3 task 0

One row of the table above was wrong as stated. It is corrected below and the
code now matches: `EntryId::for_launch` folds arguments in where a shortcut has
them, `v0_3_launch_arguments_are_part_of_identity` asserts it, and
`v0_3_an_argument_free_id_is_unchanged_by_the_amendment` asserts that every id
v0.2 wrote is unchanged.

`EntryId::for_launch` treats launch arguments as detail rather than identity, which
is right for two shortcuts to one application with different switches and wrong for
a **host binary**. Nine Start Menu shortcuts on the development machine point at
`cmd.exe` — Command Prompt, four Visual Studio tools prompts, KiCad's, two of
Node.js' — and the arguments *are* the application. They collapse onto one id and
fifteen distinctly-named applications disappear, measured in
[`../tbd/v0.2.md`](../tbd/v0.2.md) §9.

The rule survives; its worked example does not. Durability still decides, but
"the same executable" turns out not to mean "the same application", and identity
has to carry whatever distinguishes them. Arguments are folded in where they
exist, joined by `|`, and the argument-free case stays byte-identical so nothing
already learned is invalidated. `v0_2_launch_arguments_do_not_change_identity`
was amended rather than deleted.

The working directory stays out. It is where an application starts, not which
application it is, and folding it in would give two ids to one shortcut edited in
Explorer.

**Confirmed against the real machine after the change**: 34 Entries now carry an
argument-bearing id, thirteen of them on the three host binaries §9 named — nine
on `cmd.exe` (Git CMD, KiCad's prompt, both Node.js prompts, and the five Visual
Studio tools prompts), two on `javacpl.exe`, two on the x86 `powershell.exe`. The
argument-free member of each family keeps the bare path, which is what makes the
change additive. The measurement is `v0_2_measure_the_real_walk`, run with
`--ignored`.

## What would change this

Title collisions hiding a real application. If two genuinely different tools ever
share a title and one disappears from the Palette, the rule needs a second
discriminator — most likely the target path, already computed and currently used
only as a secondary check.

Evidence for the decisions above is in `docs/tbd/v0.2.md` §3a, §4a and §8, with
the measured duplicate counts.
