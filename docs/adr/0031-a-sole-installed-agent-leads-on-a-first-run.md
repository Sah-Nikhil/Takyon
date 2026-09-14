---
status: accepted
amends: ADR-0018
---

# A sole installed Agent leads on a first run

On a first run, if the probe finds exactly one Agent installed, Takyon ranks that
Agent first in the Agent order and switches it on. The change is written to
`settings.db`, and it happens once.

## The problem

A fresh install orders the Agents `claude`, `codex`, `opencode`, with every one
switched on. Someone who has only Codex installed opens Settings → Agents and
sees it ranked second, under a Claude Code row that reads "Not found". `!c`
still reaches Codex, because once the probe lands it steps over the missing
Agent (ADR-0018). The Settings page makes it look as if Takyon picked the wrong
Agent, and the user's first job becomes fixing a ranking they never chose.

## The rule

`agents::lead_sole_agent` runs inside the `agent_snapshots` command, straight
after the probe. That command is the only place installed-ness is known, and it
already costs three process spawns, so the rule adds nothing to the keystroke
path. The rule applies only when all of these hold:

- **No Agent choice is stored.** `agents.order`, the legacy `agents.default`, and
  every `agents.enabled.<kind>` are all absent. Any one of them means a person has
  already made a choice here, and a probe never overrides that.
- **Exactly one Agent reports `installed`.** Signed-out counts as installed: the
  user asked about installed Agents, and a signed-out one is still the only CLI
  `!c` could run.

When both hold, the Agent's switch is written as on and the order is written with
that Agent first. Writing the order is what makes the rule one-shot: on the next
probe the first condition fails. A user who switches the Agent off or moves it
down afterwards keeps that choice.

A probe that finds no Agents, or several, writes nothing. The first run is not
used up, so a later probe that finds one Agent still applies the rule.

## What we gave up

An install that upgraded from v0.10 and never touched Settings → Agents counts as
a first run. The defaults it holds are defaults, not choices, so this is
intended, but it does mean an existing install can see its order change once.

The rule does not re-apply when the machine changes. If a user who already has a
stored order uninstalls every Agent but one, the order stays as they left it.
Doing more would mean guessing whether a stored order still reflects what they
want, and ADR-0018 already stops `!c` from being blocked by a missing Agent.

## Consequences

- `agents::route` still reads preferences only. The probe may now write them
  once, but nothing on the keystroke path spawns a process.
- Settings → Agents re-reads the order and switches when a probe lands, so the
  new ranking shows without reopening the window.
- The mock's `agentSnapshots` applies the same rule, and `setAgentMissing` is the
  hook a visual test uses to reach one installed Agent.
