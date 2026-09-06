/**
 * One theme, as a sphere.
 *
 * The idea is t3code's `ThemePreviewCircles`, and it is the right one: a swatch
 * of the plate is nearly black in every dark theme, so five of them side by side
 * are indistinguishable. A lit sphere shows the plate *and* both signal colours
 * at once, and it survives being 44px wide, which a wireframe does not.
 *
 * Everything is one element and three gradients — no canvas, no asset, no
 * library. Interpolated `in oklab` so the falloff has no grey midpoint: mixing a
 * saturated accent into a near-black plate through sRGB desaturates through the
 * middle and produces a visible dead ring.
 */

import { half, type Appearance, type ThemeFamily } from "./themes";

/**
 * How the two appearances are lit.
 *
 * A dark orb is a light source seen in the dark; a light orb is a surface
 * *under* a light. Lighting both alike is what makes a light theme's preview
 * read as a washed-out dark one.
 */
const LIGHTING = {
  dark: { base: "oklch(0 0 0)", baseWeight: 94, accentPeak: 92, accentMid: 40, warmPeak: 52 },
  light: { base: "oklch(1 0 0)", baseWeight: 92, accentPeak: 78, accentMid: 30, warmPeak: 40 },
} as const;

export function ThemeOrb({
  family,
  appearance,
  size = 44,
}: {
  family: ThemeFamily;
  appearance: Appearance;
  size?: number;
}) {
  const colors = half(family.id, appearance);
  const light = LIGHTING[appearance];

  return (
    <span
      aria-hidden
      style={{
        width: size,
        height: size,
        backgroundColor: `color-mix(in oklab, ${colors.plate} ${light.baseWeight}%, ${light.base})`,
        backgroundImage: [
          // The accent, up and to the left, as if lit from off-frame. Contained
          // rather than filling the sphere: the plate has to stay the dominant
          // colour or every family reads as its accent and the neutral one reads
          // as broken.
          `radial-gradient(circle at 32% 27% in oklab, color-mix(in oklab, ${colors.accent} ${light.accentPeak}%, transparent) 0%, color-mix(in oklab, ${colors.accent} ${light.accentMid}%, transparent) 34%, transparent 68%)`,
          // The warm signal from the opposite corner, softer and never a second
          // hotspot — two equal highlights read as headlights, not one lit
          // object. Not decoration either: at half this weight two families with
          // neighbouring accents drew the same orb.
          `radial-gradient(circle at 78% 80% in oklab, color-mix(in oklab, ${colors.outbound} ${light.warmPeak}%, transparent) 0%, transparent 56%)`,
        ].join(", "),
        // Offset and blurred, not a zero-offset halo: the sphere sits on the
        // card, so it casts downward.
        boxShadow: `inset 0 0 0 1px color-mix(in oklab, ${colors.fg} 14%, transparent), 0 2px 6px -2px rgb(0 0 0 / 0.4)`,
      }}
      className="shrink-0 rounded-full"
    />
  );
}
