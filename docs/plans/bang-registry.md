# Bang registry — parked design

**Status: parked deliberately.** Bangs are not being designed until the base
launcher (v0.1–v0.7) works. This file holds the recommendation as it stood when
the question was deferred, so the discussion resumes from here rather than from
scratch. Revisit alongside the `!c` / Claude Code design (ROADMAP v0.9).

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

## Open questions when this resumes

- Do Bangs contribute Entries to the `Ctrl+K` action menu, or is a Mode's action
  set entirely its own?
- Does a Bang with no query show anything useful — recent `!s` searches, recent
  `!c` threads — or an empty state?
- Can a Bang be invoked from a selected Entry (select a file, then `!c` about it)?
  This is chaining wearing a different hat and should be judged as such.
- What does an unknown Bang do — error, or fall through to a Bangless search?
