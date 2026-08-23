---
status: accepted
---

# Bangless queries never touch the network

Every capability that leaves the machine is reachable only through an explicit
Bang (`!s`, `!c`, `!e` and successors). A Bangless line is served entirely from
local Sources and issues no network request of any kind — no search suggestions,
no telemetry on keystrokes, no prefetch. Bangs are prefix-only and mode-switching:
they consume the whole line, so there is no way to accidentally half-trigger one.

This is a product guarantee, not an optimisation. "Nothing leaves this machine
unless I typed a Bang" is a stronger claim than "it's fast", and it is only
credible if it holds without exception.

## Consequences

Any feature that wants network access must earn a Bang or live behind an explicit,
default-off setting. A code review that finds an outbound request on the Bangless
path should treat it as a correctness bug, not a performance issue.

Bang chaining is deliberately excluded from V1 so that a line has exactly one Mode
and the guarantee stays trivially checkable — see `docs/plans/post-v1.md`.
