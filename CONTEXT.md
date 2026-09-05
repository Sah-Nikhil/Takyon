# Takyon

A local-first keyboard launcher for Windows. This glossary defines the vocabulary
used across the codebase and docs. Terms are settled during design sessions and
should be used exactly as written — including in identifiers, comments and UI copy.

## Language

### Surfaces

**Palette**:
The single-line input and its result list — the thing that appears on the hotkey
and disappears on Escape. Holds no state between openings.
_Avoid_: launcher, bar, spotlight, command palette, window

**Chat Surface**:
The Palette showing a multi-turn conversation rather than a list. Exists only
after a Promotion, replaces the Entry list while it is open, and Escape goes back
to the search line. **Not a second window** — one launcher window holds
everything, so a conversation lives as long as the Palette is up.
_Avoid_: thread window, chat window, conversation panel

**Promotion**:
The moment a Palette interaction becomes a Chat Surface, triggered by the user
asking a follow-up. Lazy by definition — a single question answered inline is
never promoted. It is also where tools turn on: the reflex answer has none, and
asking again is an explicit act (ADR-0017).
_Avoid_: expanding, opening a thread, escalating

### Input

**Bang**:
A prefix token beginning with `!` that switches the whole input line into a
different mode (`!s`, `!c`, `!e`). Valid only at the start of the line; everything
after it is that mode's raw query rather than a ranked search.
_Avoid_: command, trigger, prefix, shortcut, alias

**Bangless**:
An input line with no Bang. Strictly local and strictly offline — a Bangless query
never touches the network. This is a product guarantee, not an implementation
detail.
_Avoid_: plain query, normal search, default mode

**Mode**:
The behaviour a Bang selects. Each Mode owns its own query semantics, result
rendering and actions.
_Avoid_: provider, plugin, handler, extension

### Results

**Entry**:
A single actionable row in the Palette's result list, regardless of what it came
from — an application, a file, a clipboard item, a calculation.
_Avoid_: result, item, hit, match, row

**Subtitle**:
An Entry's second line. Shown only when another Entry in the same list carries the
same title, because its one job is telling those two apart (ADR-0016).
_Avoid_: description, detail, caption, path

**Source**:
A producer of Entries for Bangless queries (applications, files, clipboard
history, calculator). Sources are queried in parallel and their Entries compete in
one ranked list.
_Avoid_: provider, index, backend, plugin

**System entry**:
An Entry that opens a Windows settings page or a control-panel task rather than
launching a program: `bluetooth` reaches the Bluetooth page, "Change how your
keyboard works" opens its task. Two Kinds, because the two halves behave
differently. A **settings page** is a destination you ask for by name, so it
shares the App rank tier and competes on match quality and Frecency. A
**control-panel task** is one of ~198 long sentences that can only ever match by
word prefix, so it sits below every app. Both carry a weight below 1 so a system
entry has to be decisively better matched or more used to take the top row from an
application, not marginally.
_Avoid_: setting, command, control panel item, shortcut

**Game launcher**:
Steam, Epic, GOG and the like — a third-party program that owns a set of installed
games and starts them itself. Never bare "launcher": Takyon is a launcher, and the
two readings collide. A game Entry is an ordinary App Entry whose launch goes
through its game launcher's URI rather than an executable, because run directly
most games refuse to start and none gets cloud saves or playtime.
_Avoid_: launcher (bare), store, platform, client

**Frecency**:
A per-Entry score combining how often and how recently the user has chosen it,
decayed over time. Used to rank Entries above raw match quality.
_Avoid_: popularity, usage score, weight, ranking

**Stability**:
The guarantee that the Palette's top Entry does not change once the user has
stopped typing, even as slower Sources report late. Prevents launching the wrong
thing by pressing Enter mid-reorder.
_Avoid_: debounce, settling, locking

**Hit**:
One result a search provider names, before its page has been read: a title, a URL
and the provider's own snippet. A Hit is not an Entry — it is never ranked and
never appears in the Bangless list.
_Avoid_: result, link, search result, item

**Citation**:
A Hit together with whatever text its page yielded, numbered so the answer can
refer to it as `[1]`. Deliberately not "Source", which already names the producers
of Entries. The answer's own list is headed **Sources** in the interface, because
that is what a reader expects above a set of links, and this is the one place the
two words are allowed to diverge.
_Avoid_: source (in code), reference, document, page

### Agents

**Agent**:
A coding-agent CLI that Takyon drives as a subprocess — Claude Code, Codex,
opencode. Takyon holds no account, key or token for any of them; it runs the one
the user already installed and already signed in to. "Agent" is the whole
program, not the model inside it.
_Avoid_: provider, backend, model, assistant, LLM, integration

**Agent Driver**:
The Rust implementation of one Agent: where its binary is, how to read its
Sign-in state, and how to run a Turn against it. One trait, one file per Agent,
no Agent-specific branches outside them.
_Avoid_: provider, adapter, plugin, connector, harness

**Agent order**:
The ranking of every Agent, first to last, set in Settings. `!c` asks the first
one switched on and works down from there, so it is a preference rather than a
choice — the second entry is what answers when the first cannot. Always holds
every Agent exactly once, switched-off ones included.
_Avoid_: default agent, fallback chain, priority list, provider order

**Switched on**:
Whether an Agent is one `!c` may reach at all, set by its switch in Settings.
Distinct from Sign-in state, and the distinction is the point: this is a stored
preference, so `!c` reads it on the keystroke, while Sign-in state costs a
process to learn. A switched-off Agent is skipped without being probed.
_Avoid_: enabled provider, active agent, available, connected

**Locked pair**:
The model and effort level chosen in Settings for one Agent. Every Turn uses that
pair and nothing may override it — not a Bang, not a keystroke, not the frontend,
which never sends either value. Picked from what that Agent itself reports.
_Avoid_: model setting, default model, preset, profile

**Sign-in state**:
What Takyon can currently say about an Agent's credentials, asked of the Agent
itself and never stored by Takyon: **signed in** (with the account label the
Agent reports), **signed out**, or **unknown** when the Agent is installed but
would not answer. Signing in happens in the Agent's own CLI (ADR-0017).
_Avoid_: auth status, login state, credentials, session

**Turn**:
One question and the answer to it. The unit an Agent Driver runs, the unit that
streams, and the unit a Palette Escape may never interrupt once a Chat Surface
owns it.
_Avoid_: request, message, exchange, round trip, completion

**Scratch directory**:
The empty directory a Turn runs in when the user has not chosen another one. Its
job is to be uninteresting: an Agent pointed at a directory the user did not pick
must not find a repo there (ADR-0017).
_Avoid_: workspace, project, sandbox, temp dir
