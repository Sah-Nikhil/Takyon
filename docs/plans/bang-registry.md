# Bang registry — parked design

**Status: parked deliberately.** Bangs are not being designed until the base
launcher (v0.1–v0.7) works. This file holds the recommendation as it stood when
the question was deferred, so the discussion resumes from here rather than from
scratch. Partly resumed at v0.9: the `!c` question below is settled, the picker and
user-defined Bangs are not.

What is already settled and should not be re-litigated: Bangs are **prefix-only**
and **mode-switching**, a **Bangless line never touches the network** (ADR-0002),
and **chaining is out of V1** (`post-v1.md`).

## Recommendation as parked

**V1 Bang set:** `!e` files · `!s` web · `!c` Claude · `!v` clipboard.

**`!` alone opens a picker** listing every Bang with its description — this is the
only discovery mechanism, so it has to be good. Nobody reads documentation for a
launcher.

**Users can define URL-template Bangs** (`!yt <query>` → a YouTube search URL) and
**alias existing ones** (`!g` → `!s`), both from settings.

**The `!` sigil itself is not configurable in V1.** It sounds free and isn't: it
touches the parser, the picker, every piece of UI copy and all documentation, to
serve a preference nobody has actually asked for. Revisit only if real users ask.

## Settled at v0.9: one `!c`, not one Bang per Agent

Takyon drives three Agents — Claude Code, Codex and opencode (ADR-0017) — and all
three answer through **`!c` alone**. The Agent is a preference, chosen in
Settings and switchable for one query from the `Ctrl+K` action menu. `!c` reads
as "ask the agent", not "ask Claude", and the V1 Bang set above is unchanged.

Two alternatives were considered and deferred rather than rejected. Both stay
here so v1.0 resumes from the argument instead of re-running it.

**One Bang per Agent — `!c` Claude, `!x` Codex, `!o` opencode.** Explicit, no
hidden state, and it makes "which one answered this" unmissable. It costs two
more Bangs in a V1 set of four, and the `!` picker — the only discovery
mechanism — starts listing the same capability three times. Revisit if users
routinely switch Agent mid-session, which the `Ctrl+K` switch will tell us.

**An Agent token inside the query — `!c @codex explain this`.** One Bang,
explicit per query, no state at all. It costs the property that makes the parser
readable: today everything after a Bang is that Mode's raw query and nothing
else, which is what makes ADR-0002 checkable by reading `bang.rs`. A second
grammar inside a Bang's payload ends that. Revisit only alongside chaining
(`post-v1.md`), which is the same problem.

## Open questions when this resumes

- Do Bangs contribute Entries to the `Ctrl+K` action menu, or is a Mode's action
  set entirely its own?
- Does a Bang with no query show anything useful — recent `!s` searches, recent
  `!c` threads — or an empty state?
- Can a Bang be invoked from a selected Entry (select a file, then `!c` about it)?
  This is chaining wearing a different hat and should be judged as such.
- What does an unknown Bang do — error, or fall through to a Bangless search?
