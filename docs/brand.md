# Brand

## The mark — locked

A Cherenkov wavefront: the cone a particle drags behind it when it exceeds light
speed in a medium, with the particle itself detached beyond the apex, having
outrun the wake it created.

```svg
<svg viewBox="0 0 64 64" xmlns="http://www.w3.org/2000/svg">
  <path d="M46,32 L12,16.5 Q23,32 12,47.5 Z" fill="currentColor"/>
  <circle cx="56" cy="32" r="3.9" fill="var(--accent)"/>
</svg>
```

Two filled shapes, no strokes — it inverts without redrawing and greys out
cleanly when Windows renders the tray monochrome. Verified legible to 18px.

### Rules

- **The particle stays outside the cone.** It has outrun its own wake; that is the
  entire name. The gap never closes.
- **The back edge bows inward.** A wavefront is not a triangle, and a straight
  back edge turns the mark into a play button.
- **The wordmark is lowercase**, `takyon`, at every size. Never title case.
- **Monospace is for measurement only** — timings, hex values, paths, Bang
  strings. Mono used as atmosphere is the costume every developer tool owns, and
  it is what made an earlier direction read as a terminal.

## Colour — deliberately not locked

The mark is settled; the palette is not. The leading proposal, from Direction IV,
is **one hue in two states** rather than two palettes: the accent runs flat and
precise on paper (documentation, site, print) and luminous on the instrument (the
app itself), because Cherenkov light is only visible in the dark. Its warm
counterpoint follows the same rule.

Alongside it sits a semantic proposal worth keeping whatever hues win: **cool means
contained, warm means it left** — a Bang wears its colour in the picker, the input
chip and the result header, so the ADR-0002 guarantee becomes something a person
can see rather than read.

Neither is decided. Revisit before v0.6 (Settings), which is the first phase that
needs a real theme.

## Directions explored

Four identity boards, all live:

| | Direction | Idea | Outcome |
|---|---|---|---|
| I | Cherenkov | Ring and offset chord; cyan derived from Cherenkov radiation | Colour idea survived |
| II | Negative Time | Detached prompt caret on warm film stock | **Rejected** — the chevron is a terminal cliché and mono-everywhere reinforced it |
| III | Wavefront | The cone mark, set as a printed scientific plate | Mark and world survived |
| IV | Cherenkov (synthesis) | III's mark and plate, with I's accent as a two-state hue | **Mark locked from here** |

- I — https://claude.ai/code/artifact/443242d1-9bf2-4611-bb8f-12be5ddd987f
- II — https://claude.ai/code/artifact/5291fe07-529a-4e16-b543-ea827709ccf2
- III — https://claude.ai/code/artifact/fd474764-e92f-42dc-8fb6-ce73c978b976
- IV — https://claude.ai/code/artifact/7ac26987-3cd6-492d-a25c-19ee78d47dbe

## The timing chart

Not a logo, but the brand's primary asset: keypress at 0 ms, first pixel at 12 ms,
results ranked at 28 ms, and *you finish the word* at ~340 ms. The shaded window
is everything Takyon does, closing before a human finishes a four-letter word.

It belongs on the site, in the README and in first-run. **These numbers are the
budget, not measurements** — they are claims the product has to earn, and the
chart must be redrawn from real benchmark output before it is published anywhere.
Shipping the aspirational version as though it were measured would be a lie.
