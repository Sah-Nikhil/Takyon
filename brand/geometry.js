// The locked mark, in one place.
//
// A Cherenkov wavefront: the cone a particle drags behind it when it exceeds
// light speed in a medium, with the particle itself detached beyond the apex.
// These two shapes are copied verbatim from docs/brand.md and are LOCKED. Every
// asset in the repo is generated from this file, so the mark cannot drift
// between the tray, the installer, the favicon and the UI.
//
// Rules that live in the geometry (docs/brand.md):
//   - the particle stays outside the cone; the gap never closes
//   - the back edge bows inward, or the mark reads as a play button

/** Author-space canvas the paths below are drawn in. */
export const VIEWBOX = 64;

/** The cone. Apex right at x=46, back edge bowed inward via Q23,32. */
export const CONE = "M46,32 L12,16.5 Q23,32 12,47.5 Z";

/** The particle, already past the apex. */
export const PARTICLE = { cx: 56, cy: 32, r: 3.9 };

/**
 * Tight bounding box of cone + particle in author space.
 * The Q control point at x=23 bows toward the apex, so it never extends the
 * box; the extremes are the back edge (x=12), the particle (x=59.9) and the
 * back edge corners (y=16.5 / y=47.5).
 */
export const BOX = {
  x: 12,
  y: 16.5,
  w: PARTICLE.cx + PARTICLE.r - 12, // 47.9
  h: 47.5 - 16.5,                   // 31
};

/**
 * Transform that fits the mark into `canvas`, occupying `fill` of its width,
 * centred on both axes. Returns an SVG transform attribute value.
 */
export function fit(canvas, fill) {
  const scale = (canvas * fill) / BOX.w;
  const tx = canvas / 2 - (BOX.x + BOX.w / 2) * scale;
  const ty = canvas / 2 - (BOX.y + BOX.h / 2) * scale;
  return `translate(${round(tx)},${round(ty)}) scale(${round(scale)})`;
}

const round = (n) => Number(n.toFixed(4));
