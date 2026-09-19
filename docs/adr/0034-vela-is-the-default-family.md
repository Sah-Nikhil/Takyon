---
status: accepted
amends: ADR-0023
---

# Vela is the default family

ADR-0023 settled what a theme *is* — a family carrying both appearances — and, in
passing, which family a fresh install gets. This amends the second part only. The
architecture is untouched: five families still ship, each still states seven roles
per half, and every other token is still derived.

## Context

ADR-0023 gave the default to Graphite and gave one reason for it: "a launcher
opens over whatever wallpaper the user has, and a neutral plate is the only one
that never argues with it". The same sentence is what demoted Cherenkov.

That reason is real and it is also incomplete. It optimises for a plate nobody
notices, and the thing a launcher most needs in the fraction of a second it is on
screen is to be recognisable. A neutral near-black is what every other launcher on
Windows already looks like. Graphite is the safest plate in the set and the least
identifiable one.

## Decision

**`DEFAULT_THEME` is `vela`, in both halves.** Vela's dark plate is
`oklch(0.196 0.0255 293)`: the same L≈0.196 every dark half in the set holds, at
0.0255 chroma against Graphite's 0.0045. It reads as a dark indigo rather than as
a colour, which is the point — enough hue to be this product, not enough to fight
the desktop behind it.

Graphite stays in the set unchanged and is one click away in Settings. Nothing
about it was wrong. It is no longer what a fresh install opens with.

## Consequences

- **The pre-paint fallback in `styles.css` moved with it.** Those tokens are what
  a window paints in the frame before `theme/apply.ts` runs. Left on Graphite they
  would flash the old plate for one frame on every cold open — exactly the class
  of defect v0.10.1 was spent finding, the kind the visual layer cannot see.

- **Rust's `prefs::DEFAULT_THEME` moved with it.** It is a separate literal from
  the TypeScript one by design, and the doc-comment on it already requires the two
  to match.

- **The visual suite barely noticed, and that is the finding worth keeping.**
  Recolouring every surface from Graphite to Vela changed 4 of 119 screenshots.
  Playwright compares in YIQ with a per-pixel threshold, and the plate delta —
  about R20 G20 B22 to R22 G19 B32 — falls under it, so plate-only pixels never
  count as different. Only the views carrying large accent-coloured areas failed.
  **A theme swap is not something this layer can be trusted to catch.** All 48
  baselines were regenerated deliberately rather than only the 4 that failed, or
  the other 44 would have stayed pictures of a theme the product no longer ships.

- **`scripts/verify-drive-v0.10.ps1` still passes, with less margin.** Its
  `A-oklch` step asserts the plate pixel is dark with a bounded channel spread.
  Graphite's spread was about 2 against a ceiling of 24; Vela's is about 13. The
  check still separates a painted plate from an unresolved colour function, which
  is its job, but it is no longer a wide pass.

- **What this costs.** A tinted plate does sit less quietly over an arbitrary
  wallpaper than a neutral one. ADR-0023 was right about the mechanism; this ADR
  accepts the cost rather than disputing it.
