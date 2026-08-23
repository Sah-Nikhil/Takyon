---
status: watching
---

# TBC-0006 — The Palette sizes itself to its content

## The bet

The Palette window has no user-selectable size mode. It starts as a single input
row, grows as Entries appear, and grows further to hold an inline `!s` or `!c`
answer. The assumption: **because the window must already grow for inline
answers, a user-chosen fixed size would immediately conflict with
content-driven resizing**, and reconciling the two is more design work than the
feature earns before anyone has asked for it.

Raycast offers Compact and Expanded modes with visual previews in settings, so
this is a deliberate divergence from the reference product, not an oversight.

## How we'd know we were wrong

- Resizing is visually janky — the window growing per keystroke as results arrive
  reads as instability rather than responsiveness. This is the most likely
  trigger and it will show up in the first week of real use.
- Users on large displays want a bigger persistent surface, or users on laptops
  want a strip that never covers their work.
- The Chat Surface and the Palette end up wanting such different geometry that
  one window can't serve both.

## Alternatives

| Option | Improvement if we switch | Added complexity | Switching cost |
|---|---|---|---|
| **Fixed height, scroll inside** | No resize jank at all; the window is a stable target the eye can anchor to | Low — arguably simpler than animating growth | **1–2 days.** Wastes screen space on short result lists and constrains inline answers |
| **Compact / Expanded modes** (Raycast) | Users self-select; matches an interaction people already know | Medium — two layouts to design, test and keep visually consistent, plus reconciling each with content growth | **3–5 days** including settings UI with previews |
| **Remember the last size, user-resizable** | Maximum user control | Medium — persisted geometry, multi-monitor and DPI-change edge cases, and it fights ADR-0001's "the Palette remembers nothing" | **2–4 days**, and it contradicts a decision we deliberately made |

## Verdict if triggered

If the trigger is jank, fix the animation before adding a mode — a fixed maximum
height with internal scrolling removes most of it for a day of work. Only add
Compact/Expanded if users actually ask for a *choice*, rather than us assuming
they want one because Raycast offers it.
