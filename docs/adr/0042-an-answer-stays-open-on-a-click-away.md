---
status: accepted
---

# An answer stays open on a click away

A `!c` or `!s` answer no longer disappears when focus leaves the Palette. The
plain launcher list still does. The behaviour is a setting, on by default.

## The problem

Since v0.1 the Palette hides on any `Focused(false)`. That is right for a
launcher: you type, you launch, and a click anywhere else means you are done.
It is wrong for an answer. An answer is read beside other work. People copy
part of it into an editor, type it into a form, or write it down, and every one
of those starts with a click somewhere else. That click closed the answer, and
under ADR-0033 it also killed the Agent mid-answer.

A user asked for this directly: keep the window for a few messages and let them
work in the window behind it.

## The decision

**A pinned answer survives focus loss.** `window::should_hide_on_focus_loss`
returns false while the open View is `Ask` or `Web` and `palette.pinAnswers` is
on. Clipboard history, Focus and the list keep the old rule: they are choices
made in a second, not something to read.

**On by default.** The request is the common case, and a setting nobody finds
helps nobody. Settings → Agents → "Keep answers open when you click away" turns
it off. The value is cached in an atomic, as the layout preferences are, so a
focus event never reads SQLite.

**The hotkey brings it back.** A pinned answer behind other windows is visible
but not focused. Pressing the hotkey there focuses it, where it used to hide a
visible Palette. That is how anyone reaches for it again. On a focused Palette
the hotkey still hides it, so the three ways out are unchanged: hotkey, Escape,
and a click away from anything that is not a pinned answer.

**The answer says so.** The footer of a pinned answer reads "Stays open when
you click away · Esc to go back", because a window that no longer behaves like
the rest of the launcher should say why.

**Agents keep running.** ADR-0033 ties Agent processes to the Palette being
*hidden*. A pinned answer is not hidden, so its Turn finishes while you work
elsewhere, and a follow-up resumes the same session.

## Consequences

- The Palette stays always-on-top while pinned, so it can cover part of the
  window you clicked into. It was always topmost, and that is what keeps an
  answer readable beside the work. It is dismissed the usual ways.
- A Turn left running in a pinned answer spends the Agent's usage until it ends
  or the Palette is dismissed, which before this would have stopped it.
- Nothing changes for a Bangless query, so the launcher's own feel and its
  latency budgets are untouched.
