# ADR-0023: A theme is a family carrying both appearances

**Status:** accepted (v0.10)
**Supersedes:** the open colour question in `docs/brand.md`

## Context

`docs/brand.md` deliberately left the palette open "until v0.6, which is the
first phase that needs a real theme". v0.6 came and went: the settings window
needed a surface hierarchy before the hue was settled, so it borrowed
[t3code](https://github.com/pingdotgg/t3code)'s construction — surfaces raised
rather than grey, every separation an alpha of the foreground — and kept the
question open by making everything derived. That was the right call and it held
for four phases.

v0.10 is where it stops holding. The moment there is more than one palette, the
question is no longer "which three hex values" but "what *is* a theme", and the
second question has an architecture attached to it.

Two facts forced the decision:

- **The light theme had never worked.** Not because its four values were wrong,
  but because only the settings window was ever tokenised. Everything under
  `apps/desktop/src/palette/` still named raw colours: `border-white/10` for the
  panel edge, `bg-white/10` for the selected row, `bg-white/5` for keycaps,
  `bg-black/40` for the action-menu scrim, `amber-*` for the outbound state.
  White at 10% is a hairline on a near-black plate and nothing at all on a
  near-white one, so light mode shipped with an invisible panel border and an
  invisible selection from v0.6 to v0.9. Thirty-one occurrences across seven
  files.
- **`--color-accent` and Tailwind's `amber-*` were doing three jobs between
  them.** `docs/brand.md` states a semantic — cool means contained, warm means it
  left — and then the warm half was spelled as a palette constant no theme could
  move, in the same class as an error message and a dead-hotkey banner.

## Decision

**A theme is a family that carries both appearances**, and the model is
t3code's `themePalettes.ts` taken wholesale:

```ts
interface ThemeFamily {
  id: string;
  label: string;
  dark: ThemeHalf;
  light: ThemeHalf;
}
```

Both halves are always present. **Dark theme** and **Light theme** are two
independent choices over one list, and **Follow system appearance** decides which
is live. A family with only one half would appear in one picker and not the
other, which reads as a bug rather than as a constraint.

A half is **seven roles**, not fifty:

| Role | What it is |
|---|---|
| `plate` | the window canvas |
| `fg` | text, and the source of every derived separation |
| `accent` | something local: selection, tick, "Applied", the mark's particle |
| `outbound` | the network signal, and only that: `!s`, the outbound header |
| `warning` | a refused write, a dead hotkey, a dead alias |
| `card` | the raised surface |
| `sidebar` | the settings sidebar |

Seven because v0.6's two rules survive: **surfaces are raised, never grey**, and
**every separation is an alpha of the foreground**. `--color-control`,
`--color-hairline`, `--color-edge`, `--color-seam`, `--color-key`,
`--color-row-hover` and `--color-row-selected` are all derived from `plate` and
`fg` in `styles.css` and no theme states them. `card` and `sidebar` are stated
because the surface order inverts between halves: dark lifts by mixing the
foreground in, light lifts by being pure white against a tinted ground.

**Colours are authored in oklch**, and the mixes moved from `in srgb` to
`in oklab`. Not fashion. The discipline a five-family set needs is *equal
lightness across families* — every dark plate at L≈0.19, every dark accent at
L≈0.78, every light plate at L≈0.975 — and that is a property oklch states and
hex hides. The mix space matters for the same reason: blending a hue-tinted
near-black toward a near-white through sRGB desaturates in the middle, which is
exactly the milky failure v0.6's two rules exist to prevent.

**Five families ship**: Graphite (the default, both halves), Vela, Cherenkov,
Aurora, Halide. Three is not a library and twelve is a page nobody reads.

**Cherenkov is no longer the default.** It stays in the set — it is the mark's
own hue and it is what shipped through v0.9 — but a launcher opens over whatever
wallpaper the user has, and a neutral plate is the only one that never argues
with it. Demoting it in an ADR is better than demoting it silently.

**`--color-scrim` is the one role that is neither derived nor theme-owned.**
Every other separation inverts correctly because the foreground inverts. A scrim
does not: its job is to push the surface behind it back, and that is a darkening
in *both* appearances. Derived, it would wash a light theme white and brighten
the thing it was meant to dim. It is stated per appearance in `styles.css`.

## Consequences

- **Nothing under `apps/desktop/src` names a colour**, with one deliberate
  exception: `#c42b1c` in `settings/TitleBar.tsx`, which is Windows' own
  close-button red and belongs to the platform's title bar rather than to us.
- The theme registry is **TypeScript only**. Rust stores the family id without
  interpreting it, because duplicating the registry would be two sources of truth
  for a set that changes whenever a theme is added. An id a build does not
  recognise falls back in the renderer.
- The stored key `ui.theme` became `ui.appearance`, because "theme" now means a
  family. `settings::migrate` carries the value across on first start; a rename
  without it would silently reset every existing override to `system`.
- **Halide's accent is gold, and it is the one accent in the set that is not
  cool.** The first version obeyed `docs/brand.md` literally and made it teal,
  which cost the family its identity: an amber plate at L 0.19 shows almost no
  hue, so with a cool accent Halide and Aurora drew the same preview. The rule
  governs *Bang* surfaces, and `outbound` is what carries those — an accent
  paints a selection ring, which is about something local whatever colour it is.
  Halide's `outbound` moves to red-orange, 56° away, to keep the two apart.
- A custom-theme import stays cheap to add: seven roles and a JSON file. It is
  deliberately **not** in this phase (`docs/tbd/v0.10.md` §1) — the ask was a set
  of good themes, not a colour picker.
- Every visual baseline was regenerated. The selection highlight changed value on
  dark as well, because `--color-row-selected` is 10% of `--color-fg` where
  `bg-white/10` was 10% of pure white.
