# To be done

One file per phase, recording what that phase left undone and which later phase
has to pick it up. `ls docs/tbd/` answers "what is outstanding across the whole
project?" without opening a single plan.

A phase ships when its exit criteria are met. Some of it is always left behind
anyway — blocked on hardware, on a certificate, on a game nobody owns — and the
expensive failure is not leaving it, it is **forgetting why it was left**. Six
months later the only record is an unticked box, and someone either redoes the
investigation or quietly ticks it.

## What belongs here

A gap. Something that is not done, where being not-done is a fact rather than a
choice:

- untested surfaces (`steam://rungameid` has never run)
- work blocked on the environment (no game installed, no signing certificate)
- a fix landed but not confirmed where it mattered
- a filter that may be too strict, with the evidence that raised the suspicion

## What does not

**Decisions.** Those go to `docs/adr/` when settled, or `docs/tbc/` when settled
but provisional. The distinction is sharp and worth keeping sharp:

| | Question it answers |
|---|---|
| `docs/adr/` | Why is it built this way? |
| `docs/tbc/` | Which of those calls do we expect to revisit, and what would trigger it? |
| `docs/tbd/` | What is not done, and who does it? |

A TBC entry has an assumption, a trigger and a switching cost. A TBD entry has a
blocker and an owner-phase. Filing a gap in `tbc/` turns it into a to-do list and
destroys the thing it is good at.

**Deliberate omissions** are not gaps. "The Entry list is not virtualised" is a
decision, and it belongs in a *Not open* section at the foot of the phase file, so
nobody re-litigates it — but it is not outstanding work.

**Procedures.** Verification scripts live in [`../verify/`](../verify/). A file
here says *the script has not been run, and here is why*, then links to it.
Copying steps across would give them two homes and one would go stale.

## Shape of an entry

Frontmatter names the phase it came from and every phase that has to act:

```yaml
---
phase: v0.2
carries-into: [v0.3, v1.0]
---
```

Then one section per item, each answering four questions: what is undone, why,
what would close it, and **which phase owns it**. An item with no owning phase is
one that will not get done.

When a phase closes an item, strike it there and note it in the phase that did the
work. When a phase file has nothing left open, delete it — an empty TBD file is
noise, and git remembers.
