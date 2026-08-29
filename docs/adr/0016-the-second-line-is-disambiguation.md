---
status: accepted
---

# An Entry's second line appears only when it tells two rows apart

The subtitle under an Entry's title is shown when — and only when — another Entry
in the same list carries the same title. A row whose title is unique in the list
is drawn with no second line at all.

Decided at v0.3 task 0, alongside the change that stopped the second line lying.

## What the line is for

It exists to answer one question: *which of these two is which?* "Code" beside
"Code - Insiders", two installed Photoshop versions, `mspaint` beside "Paint" —
[ADR-0014](./0014-durable-identity-wins-a-collision.md) deliberately lets
differently-spelled pairs survive, so the Palette has to make them tellable apart.

Every other use of it was decoration. On this machine a typical query returns
rows whose titles are already unique, and under each one sat a full path the
reader did not need in order to choose. Three hundred pixels of grey text per
screen, answering a question nobody had asked.

## Why not "reveal it on the selected row"

The tempting version — hide it, then show it under whichever row is selected —
does not survive contact with [TBC-0006](../tbc/0006-content-sized-window.md).
The Palette sizes its window to its content. A row that grows when selected
resizes the native window on every arrow key, which is the exact jank that TBC
predicted from a user-chosen fixed size and the reason the window is
content-sized in the first place.

Deciding per *query* has none of that. The list is computed once, the answer is
the same for every row in it, and arrowing through changes nothing.

`ROW_HEIGHT` is fixed at 44px whether or not the second line is drawn, so this
changes no geometry and no window arithmetic.

## Where it runs

`rank::disambiguate_subtitles`, last in the pipeline — after ranking, after
dedupe, after the truncation to `MAX_ENTRIES`. That ordering is the point: "does
this title repeat?" has to be asked about the list the Palette is actually sent,
not about a longer one it was cut down from. A title that repeats only among
Entries nobody will see is not an ambiguity.

It lives in Rust rather than in `EntryRow.tsx` because ADR-0009 keeps logic on
one side of the IPC seam, and because it is testable there without a browser.

## Measured, and it is stronger than expected

**Zero of this machine's 1036 discovered applications share a title with another.**
Counted by `v0_2_measure_the_real_walk`, which now reports collisions.

So today the rule means the applications list shows **no second line at all**,
ever — not "rarely". That is not a surprise so much as a receipt: v0.2 spent §3a,
§4a and `collapse_by_name` making sure one application produces one row, and this
is what that work looks like from the front. There is nothing left to
disambiguate, and a path under every row was the last of the redundancy.

It does not make the rule pointless, because the collision it guards against is
about to become reachable. Every Source v0.3 adds — Recents, system entries, Epic
— produces Entries in the *same list* as applications, and a recently-opened file
called `Photoshop.psd` beside Adobe Photoshop is exactly the case. The rule is
cheap now and correct then.

The v0.2 evidence in `docs/verify/v0.2.md` A4 and A5 was written when the path
was unconditional, and both steps are amended rather than deleted.

## Consequences

**A Source still supplies its subtitle unconditionally.** Sources do not know
what else is in the list and must not — nothing UI-aware in a Source. They say
what the row *could* show; the pipeline decides whether it is needed.

**A subtitle that is not disambiguation has nowhere to go.** `Store app` is the
live example: it is a kind label rather than a discriminator, and under this rule
it disappears whenever the title is unique, which is nearly always. That is
accepted — the row is not ambiguous, so the label was not earning its place. If a
kind ever genuinely needs to be visible at a glance, it wants its own affordance
(a badge, an icon treatment), not the disambiguation line.

**Two verification steps changed meaning**, `A4` and `A5` in
[`../verify/v0.2.md`](../verify/v0.2.md), both of which asserted a subtitle that
is now conditional. They are amended rather than dropped: the assertion becomes
"unique title, no second line", and the path assertion moves to a query that
returns a genuine collision.

## What would change this

A week of use where two rows look identical and the user picks wrong. The rule
keys on the exact title, and titles that differ by a character — "HWiNFO® 64" and
"HWiNFO 64" — read as the same row to a person and as two to the comparison. If
that happens the comparison needs normalising, not the rule reversing.

## Extended by the same principle, 2026-08-29 — the version beside the title

Two Node installs are two applications with different titles (`node`,
`Node.js`), so the second-line rule never fires — it triggers on a shared
*title*, and these do not share one. Nothing on either row said which was which.

The version does, and it follows this ADR rather than bending it: **shown only
where it disambiguates.** Two same-named executables that report the same version
get nothing, because `powershell.exe` ships identically in `System32` and
`SysWOW64` and stamping `6.2.26100.8875` on both rows costs width and says
nothing. Measured on the dev machine: 8 filenames collide over 16 files, and only
4 of those are actually told apart by their version.

It sits beside the title rather than on the second line, because it is part of
identifying the row rather than an explanation beneath it — and because the
second line is a path, which is a different question.

**The cost is why it is scoped this tightly.** Reading the version resource of
every executable found takes **13.3 seconds** against a 450 ms walk. Reading only
the colliding names takes **3 ms**. The rule is not a style preference; it is
what makes the feature affordable at all. See `version.rs`.
