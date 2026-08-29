---
status: proposed
pairs-with: TBC-0005
---

# TBC-0010 — A recents list Takyon owns

## The bet

Takyon records what **Takyon** opens, in its own table, and serves that as the
Recents Source. The shell's `%APPDATA%\Microsoft\Windows\Recent` becomes a second
input rather than the only one.

The reason is concrete rather than architectural. The shell's list is gated on
**Show recently opened items in Start, Jump Lists and File Explorer**
(`Start_TrackDocs`), which is off on the development machine, so v0.3 task 7
shipped a Source that is permanently empty here and cannot be verified
(`docs/tbd/v0.3.md` §1). Reading around that setting — the jump-list databases in
`AutomaticDestinations` hold the same data — is technically easy and is a
straightforward violation of what the user asked Windows for. That door stays
shut.

**A record of our own actions is a different thing entirely.** The user opened it
*through Takyon*; no setting governs whether Takyon may remember its own history,
and nothing is read that the user did not do here.

## What it is not

- **Not Frecency.** Frecency answers "how often and how recently did you choose
  this", decays, and ranks. This answers "what did I touch last", is
  chronological, and is shallow — a few dozen entries, not a scored index. Two
  questions, two shapes. They share a database and nothing else.
- **Not a file index.** It knows only what passed through the Palette. Finding a
  file by name is v0.7 and is unrelated.
- **Not a substitute for the shell's list.** It cannot see a document opened in
  VS Code or double-clicked in Explorer, and it never will. That limit is the
  feature: it observes nothing outside itself.

## Shape

A table in `frecency.db`, beside the other learned usage data:

```sql
CREATE TABLE opened (
    entry_id  TEXT PRIMARY KEY,   -- what was activated
    path      TEXT NOT NULL,      -- what it resolved to, for display and relaunch
    kind      TEXT NOT NULL,      -- App, File, Folder
    opened_at INTEGER NOT NULL    -- unix seconds, last time
);
```

Written from `Pipeline::record_activation`, which already runs after every
successful launch and already writes Frecency and the collapse observation. One
more statement on a path that is off every latency budget.

Read into a `RecentsSource` that merges both inputs and prefers its own where an
entry appears in both. **Existence-checked on read** (ADR-0013): people delete
things, and a recents list of dead rows is the classic way this feature rots.

Capped, oldest evicted. A hundred is a guess and belongs in TBC-0009's company.

## The catch, and it is the whole timing question

**Today Takyon opens almost nothing but applications**, and Frecency already
ranks those better than a chronological list would. The only files it can open
are the shell's own recents, which are empty here — so a Takyon-owned list built
today would record applications, duplicate Frecency, and show nothing new.

Its value arrives with **v0.7**, when file search gives the Palette files to
open and `!e` gives it an action to open them with. Building it before that is
writing a table that answers a question nobody can yet ask.

The honest sequencing: build it as part of v0.7, not as a v0.3 patch. What v0.3
should do is stop pretending the shell's list is the only possible source, which
is what this note records.

## How we'd know we were wrong

- **It duplicates Frecency in practice.** If the recents list and the Frecency
  ordering show the same things in nearly the same order, one of them is
  redundant and this is the one to drop.
- **The cap is wrong in either direction.** Too small and yesterday's work is
  gone; too large and it stops being "recent" and becomes a second, worse index.
- **Dead rows outnumber live ones.** Then the existence check is running too late
  and should move to write time, or the feature is being used on volatile paths
  it should not follow.
- **Someone is surprised by what it remembers.** This is a local record of what
  the user did, and it must be deletable in one action from Settings (v0.6). If
  that is not built at the same time, the feature should not ship.

## Alternatives

| Option | Improvement if we switch | Added complexity | Switching cost |
|---|---|---|---|
| Leave it (v0.3's answer) | none; the Source is empty wherever `Start_TrackDocs` is off, which is a setting privacy-minded people turn on purpose | none | 0 d |
| Read the jump-list databases | complete history regardless of the setting | parsing an undocumented binary format | **Refused**, not costed. It defeats a choice the user made |
| **Own table (this note)** | works regardless of the setting, and the data is ours by construction | one table, one write per activation, one merge on read | **1 d**, and worth nothing before v0.7 |
| Own table *and* the shell's, merged | best coverage where the setting is on | de-duplication between two lists that disagree about paths | 1.5 d |

## Verdict if triggered

Build it inside v0.7, beside `!e`. Ship the Settings control that clears it in
the same phase, not after — a local history with no visible off switch is the
kind of thing that is fine until the first time somebody asks about it, and then
it is not fine at all.

If v0.7 slips, this slips with it. It has no value on its own.
