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
A separate, persistent window holding a multi-turn conversation. Exists only after
a Promotion; has its own lifecycle and survives the Palette being dismissed.
_Avoid_: thread window, chat window, conversation panel

**Promotion**:
The moment a Palette interaction becomes a Chat Surface, triggered by the user
asking a follow-up. Lazy by definition — a single question answered inline is
never promoted.
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

**Source**:
A producer of Entries for Bangless queries (applications, files, clipboard
history, calculator). Sources are queried in parallel and their Entries compete in
one ranked list.
_Avoid_: provider, index, backend, plugin

**Frecency**:
A per-Entry score combining how often and how recently the user has chosen it,
decayed over time. Used to rank Entries above raw match quality.
_Avoid_: popularity, usage score, weight, ranking

**Stability**:
The guarantee that the Palette's top Entry does not change once the user has
stopped typing, even as slower Sources report late. Prevents launching the wrong
thing by pressing Enter mid-reorder.
_Avoid_: debounce, settling, locking
