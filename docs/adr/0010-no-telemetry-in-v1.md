---
status: accepted
pairs-with: ADR-0002
---

# V1 ships zero telemetry; analytics are post-V1, opt-in, and never carry query content

Takyon V1 sends nothing anywhere. No analytics, no automatic crash reports, no
update pings beyond the explicit updater check. Crash information is written to a
local file that the user can open from settings and attach to a report themselves.

This exists because ADR-0002's promise — nothing leaves the machine unless the
user typed a Bang — is only worth making if it is *checkable*. Background
telemetry would technically not be a Bangless query, but it would make the
promise a lawyer's sentence rather than a fact, and the whole point of the
positioning is that it is a fact.

## Post-V1: PostHog, under three constraints

Analytics and crash reporting are planned after V1 using PostHog. They ship only
under all three of:

1. **Opt-in, default off**, with the choice presented plainly rather than buried
   in a first-run flow nobody reads.
2. **Never any query content.** Not Bangless queries, not `!s` searches, not `!c`
   prompts, not file paths, not clipboard content, not application names the user
   launched. Counts and versions and error types, nothing that describes what a
   specific person did.
3. **Its own ADR** amending this one, written when the shape is actually decided
   rather than assumed from this paragraph.

Constraint 2 is the one that will be under pressure, because the interesting
product questions ("what do people search for?") are exactly the ones it forbids.
That pressure is the reason it is written down now, before there is a growth
metric arguing the other way.
