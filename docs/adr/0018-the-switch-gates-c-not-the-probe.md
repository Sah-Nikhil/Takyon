---
status: accepted
---

# The switch gates `!c`, not the probe

Which Agent `!c` reaches is decided by two stored preferences — a ranked order
and a per-Agent on/off switch — and by nothing else. Sign-in state does not gate
it. The Palette names its Agent on the first keystroke, and pressing Enter starts
a Turn whether or not any Agent has been probed yet.

## The problem

v0.9 shipped with the opposite rule. `!c` showed the Agent's Sign-in state, and
Enter refused until a probe had come back and said the Agent could answer. That
probe is three process spawns of somebody else's CLI, and on a cold machine it
takes longer than anyone will wait in a launcher.

The verification drive caught it directly. The script typed `!c`, waited five
seconds, typed a question and pressed Enter. The row still read
`Checking which agent can answer.`, Enter did nothing, and the Palette sat there
until it lost focus. Every check around it passed — the window was the right
size, the webview had painted, the process was alive. Only the thing the user
came for had not happened.

Five seconds was enough on the machine where this was first written, because the
CLIs were warm from the test suite that had just run them. That is the shape of
bug that survives a green suite.

## Why the switch fixes it

An on/off switch is a preference: a lookup in `settings.db`, cached in
`Pipeline`, readable inside the 30 ms first-Entry budget. Sign-in state is a
question you have to spawn a process to ask. They look similar on a settings page
and they are nothing alike on the keystroke path.

So the switch carries the decision, and `!c` composes what it needs from two
cheap facts: the order, filtered by the switches. Where that leaves an Agent that
is switched on but signed out, `!c` asks it anyway and the Agent's own error is
the answer — which is more use than a Palette that ignores Enter and does not say
why. The probe still runs, in the background, and only *refines* the row: once it
lands, a signed-out Agent is stepped over in favour of the next one on the list.

This is also what T3 Code does. Its provider list is a set of toggles, and the
status line under each name is reporting, not gating.

## What we gave up

An Agent that is switched on but signed out now costs a failed Turn to discover,
where before it was refused up front. That is a worse first second and a better
every-second-after: the failure carries the CLI's own sentence, including the
command to sign back in, and the switch is the durable fix for an Agent the user
does not intend to use.

The switch is also a new thing to get wrong — every Agent switched off is a state
`!c` has to have copy for. It says so in amber and Enter does nothing, which is
the one case where there is genuinely nothing to try.

## Consequences

- `agents::route` is the only thing that decides which Agents `!c` walks, and it
  reads preferences only. **Nothing on that path may spawn a process.**
- `Ask.agent` is nullable. Empty means every Agent is switched off.
- `blockedReason` returns `null` for an unprobed Agent. An unfinished probe is
  not a blocked Agent, and treating it as one is the bug this ADR exists for.
- The manual verification script gained §5d, which restarts Takyon and asks
  within a second — the case that fails if this is ever reverted by accident.
