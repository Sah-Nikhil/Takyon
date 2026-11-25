---
status: accepted
---

# A Turn falls through the Agent order, and `!s` stops waiting on pages

`!c` and `!s` walk the ranked Agent order at run time. An Agent that cannot
answer hands the Turn to the next one, and only a Turn that every Agent failed
ends in an error. Separately, `!s` no longer waits for every page before it
asks an Agent to write.

## The problem

Settings ranks the Agents, and that order was only ever consulted before a Turn
started. `pickAgent` in the Palette stepped over an Agent the last probe called
missing or signed out. Then the first remaining Agent got the Turn, and if it
failed, the Turn failed. The common case is the one no probe can see: Claude
Code installed and signed in, with its five-hour or weekly window used up. The
probe says Ready, the Turn spawns, Claude prints its limit error, and `!c`
shows that error. Codex, ranked second and fully usable, is never asked.

`!s` had the same hole, and it was also slow. Its wait was spent in three
places before the Agent wrote a word:

- `fetch::pages` read all ten pages and returned only when the last one
  finished or 12 seconds passed. A page's own WinHTTP timeouts were 6 seconds
  per phase, so one slow host decided the wait.
- Favicons for new hosts were fetched after the pages and before the Turn
  started, with the same 6-second timeouts.
- The Agent ran at whatever effort Settings had locked, or the CLI's own
  default when none was locked. For Claude on this machine that default is Opus
  with thinking. Summarising pages the Agent was handed needs little reasoning.

## How t3code handles it

t3code is where the driver shape came from, so it was checked first. It
classifies every failure, with `class: "usage_limit"` and a `resetAt` taken from
Claude's `rate_limit_event` frames (`status: "rejected"`) or from Codex's
`usageLimitExceeded` code. Then `UsageLimitRecoveryWorker` resumes the *same*
thread on the *same* provider once that time passes. It never moves a thread to
another provider.

That fits a chat window that stays open for hours. It does not fit a Palette
that is dismissed in seconds and kills its Turns when it hides (ADR-0033):
nobody is there to see a resume. Two pieces of it carry over directly. The
`rejected` rate-limit frame is how a driver learns a reset time, and a reset
time says how long an Agent is worth skipping.

## The decision

**A Turn is a chain of Attempts** (`agents/turn.rs`). `agent_ask` builds one:
the Agent the Palette named, then the rest of the switched-on order. `!s` builds
the whole order. Each Attempt carries that Agent's own locked model and effort,
never the first Agent's.

**Fallback only before text.** An Attempt that fails without having emitted any
text hands over. This covers not found, a broken launcher, a spawn failure, the
Agent's own error event, a non-zero exit, and a clean exit with nothing said.
Rust emits `fallback { to, label, …failure }` and spawns the next Agent. Once
any text has reached the Palette, a failure stands as that Agent's failure.
Restarting an answer the user is already reading would be worse than the error.

**Any failure, not just usage limits.** Classifying limits reliably would mean
parsing three CLIs' error prose, which changes between releases. The rule "it
said nothing and stopped" needs no parsing, and a wrong guess costs one extra
spawn of an Agent the user ranked anyway.

**A follow-up never falls back.** A resumed session belongs to one Agent. Asking
another Agent the follow-up with no context would answer a different question.

**Exhausted is its own failure.** When every Attempt in a chain of two or more
failed, the Turn ends with `exhausted { tried, last }`. The copy says no Agent
could answer, names the ones tried, and says to sign in to one with usage left.
The last Agent's own reason goes underneath. A chain of one fails with that one
Agent's reason, which is the more useful sentence when there was no order to
exhaust.

**Failed Agents are benched** (`agents/fallback.rs`). An Agent that failed is
moved to the end of the order. That reordering applies to the Palette's `!c` row
too, so the row names the Agent that will actually answer. The bench lasts until
the reset time Claude reported, capped at seven days, or ten minutes when no
reset time was given. A success, a Settings change to that Agent, or a reorder
clears it. The bench is in memory only. Without it, every `!c` after Claude runs
out would pay a doomed Claude spawn of several seconds before Codex was asked.

**`!s` reads pages against a deadline, not to completion.** `fetch::pages`
returns at 3.5 seconds, or sooner once every page is in. Each page gets 3-second
WinHTTP timeouts, its own thread, and the same deadline for reading its body.
A page queued behind Focus's icon fetches on the shared pool could miss the
deadline, which is why the shared pool is not used. Pages still out at the
deadline go into the prompt as their snippets, the same path a page that failed
to load already took.

**A keyed `!s` reads no pages.** Exa returns page text with each hit, and it is
now asked for a full share of the prompt budget instead of a 400-character
snippet. A hit that came with text is never fetched.

**Favicons happen beside the Turn.** They are cached on their own thread after
the pages are read, and the `icons` event fires when they land, which is how the
rows already learned about late icons.

**`!s` has its own model, fast by default.** Each Agent has a `!s` model in
Settings (`agents.searchModel.<kind>`), separate from the `!c` pair. Left unset,
it is that Agent's fast default: `haiku` for Claude Code (the CLI's own alias, so
it never goes stale), `gpt-6-luna` for Codex (accepted on a ChatGPT sign-in),
and `opencode/big-pickle` for opencode (on opencode's own Zen provider, which
every install reaches without connecting one of its own). All three answered a
live one-word Turn while this was built: 2.9 s, 5.3 s and 3.7 s. "Same as `!c`"
is one choice away, stored as `@ask`, which no model id can collide with.

**`!c`'s effort follows `!c`'s model only.** When `!s` uses `!c`'s model it uses
`!c`'s effort too, or `low` for Claude and Codex where none is locked. Any other
`!s` model gets that quick effort instead. The locked effort was picked for a
different model, and an opencode variant only exists per model, so carrying it
across could name a variant the fast model does not have. opencode has no quick
effort, because no single variant word is safe across providers.

**The prompt budget drops from 24,000 to 16,000 characters.** The answer is three
to six lines, and the extra 8,000 characters added input for the Agent to read
without changing them.

**Tools-off Claude Turns skip MCP servers.** `--tools ""` removes the built-in
tools but still starts every MCP server the user configured, and those cost
startup time on every reflex Turn. `--strict-mcp-config` with no `--mcp-config`
starts none, which is what "tools off" meant all along.

## Consequences

- A user with Claude out of usage gets a Codex answer from `!c` and `!s` with no
  setting changed, and the Palette names Codex before Enter.
- A misconfigured Agent, such as one with a model name its CLI rejects, now
  falls through silently instead of failing loudly. The fallback notice names
  it on every Turn until the bench holds it, and fixing the model in Settings
  clears the bench.
- On a cold `!s` the pages wait is at most 3.5 seconds instead of 12 or more,
  and zero with an Exa key. Thin pages that would have arrived at second eight
  are cited from their snippets instead.
- Each `!s` can leave up to ten page threads running after it returns. A
  thread stops reading its body at the same deadline, so a host that trickles
  bytes inside each 3-second receive timeout cannot keep it alive. Connect and
  headers can still take up to four timeouts (about 12 seconds). Across all
  searches at most 24 page threads exist at once; past that, a page goes into
  the prompt as its snippet without a thread.

## What was not done

- **Codex reset times.** `codex exec --json` reports a limit as prose in an
  `error` event, with no structured reset field like Claude's. Codex is benched
  for the flat ten minutes.
- **Starting the Agent while pages load.** Measured on Claude Code 2.1.331: with
  the prompt held back three seconds, the time from the prompt arriving to the
  answer finishing barely changed, so the CLI does its startup after reading
  stdin, not before. There was nothing to overlap.
- **Checking that a fast default exists before using it.** A default the user's
  CLI does not have fails the Turn, and the fallback walks on to the next Agent.
  Checking first would cost a model listing on every `!s`.
