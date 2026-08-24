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
- **Motion never changes the geometry, and always returns to it.** The mark may
  breathe and it may sweep, but its resting frame is the locked drawing above,
  exactly. Anything that holds the mark off-true when the animation stops is
  wrong — that frame is what screenshots, print and every reduced-motion user
  see.

### Motion — the idle beat

The only animation the product has. While the Palette is open with an empty
query, the mark is alive in two ways at once:

| Shape | What it does | Timing |
|---|---|---|
| Particle | opacity 1 → 0.35, scale 1 → 0.72 | 1.15s, alternating |
| Cone | tip sweeps 0° → −8° → +8° → 0°, pivoting on the midpoint of its back edge | 2.3s, one full cycle |

Same easing (`cubic-bezier(0.4, 0, 0.6, 1)`) and the same start frame, and the
cone's cycle is exactly two of the particle's passes, so the two read as one beat
rather than two things happening near each other. Both are defined in
`apps/desktop/src/styles.css` and switched on by `Mark.tsx`'s `pulse` prop.

The cone is written as a full cycle through level rather than as an alternating
tilt between −8° and +8° for one reason: the first frame of an alternating tilt is
an extreme, and the first frame is what a disabled animation shows. That version
left the locked mark permanently eight degrees out of true everywhere motion is
off.

What the beat means is narrow and worth keeping narrow: *the surface is awake and
waiting on you*. It stops on the first keystroke — motion that continues while the
Palette is searching becomes a spinner, and would be claiming the opposite thing.
It is also off whenever the window is hidden, since the warm window outlives every
summon (`docs/tbc/0002`) and animating an invisible mark would cost frames forever.

Two switches turn it off, either one sufficient. Windows' own
`prefers-reduced-motion: reduce`, and **Settings → Turn off animations**, which is
the user's own switch and does not require changing an OS-wide setting to quiet
one app. The second writes `data-reduce-motion` onto `<html>`, and the rule it
drives is a wildcard, so every animation the app grows later is covered by the
switch on the day it is written. Nothing is lost when it is off: the state the
beat signals is already carried by the caret and the placeholder.

## Assets

The mark is generated, not drawn twice. `brand/geometry.js` holds the two shapes
above and nothing else does; `bun run --cwd brand build` renders every surface
from it — the installer `.ico`, the Tauri bundle set, the Store tiles, both tray
polarities, the favicon and the standalone SVGs. `brand/README.md` is the surface
map.

Consequences worth knowing before touching any of it:

- **Never hand-edit a generated file**, and never run `tauri icon` or let
  `tauri init` scaffold `src-tauri/icons/` — both replace the set with Tauri's
  default artwork.
- **The tray ships in two polarities.** Windows draws the notification area over
  a taskbar that follows the system theme, so a single light glyph vanishes when
  someone switches to light. `tray-dark` and `tray-light` are both verified
  legible at 16 pixels.
- **There is no vector wordmark.** The typeface is not locked, so drawing a
  logotype would settle a decision nobody has made. `Lockup.tsx` sets `takyon` in
  the app's own UI font instead, and is the single place to change when a
  typeface is chosen.

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

Until then `brand/tokens.json` carries three placeholder values so the assets can
exist at all: a near-white foreground, a near-black plate, and a Cherenkov cyan
standing in for the accent. They are a stand-in, not a decision. Swapping in the
real scheme is one edit to that file plus a rebuild — no asset is redrawn.

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
