/**
 * Confetti physics for the end of a Focus session: two cannons in the screen's
 * bottom corners firing up and inward, Raycast's confetti cannon as the model.
 * Pure and seedable, so `confetti.test.ts` can fly it without a canvas.
 * Units are CSS px and seconds; every constant scales with screen height, so
 * a 4K monitor gets the same arc as a laptop.
 */

/** Whole show, launch to last piece gone. `celebrate.rs` sizes the window's life to it. */
export const CONFETTI_MS = 3_600;
const PER_CORNER = 110;
/** Up from horizontal, toward the middle of the screen, ± `SPREAD`. */
const ANGLE = (62 * Math.PI) / 180;
const SPREAD = (22 * Math.PI) / 180;
/** Launch speed as screen heights per second, `[min, min + range)`. */
const SPEED = [1.9, 0.9] as const;
const GRAVITY = 1.6;
/** Air drag per second: pieces slow sharply after launch, as paper does. */
const DRAG = 1.3;
/** Fall speed cap, heights per second: paper flutters, it does not drop. */
const TERMINAL = 0.32;
/** Fraction of the show after which everything fades out. */
const FADE_FROM = 0.7;

export interface Piece {
  x: number;
  y: number;
  vx: number;
  vy: number;
  size: number;
  /** Index into the colour list. */
  colour: number;
  round: boolean;
  /** Rotation in radians, and its speed. */
  angle: number;
  spin: number;
  /** Phase of the flip that makes a strip catch the light. */
  flip: number;
  flipSpeed: number;
  /** Sideways sway once falling. */
  sway: number;
}

/** Mulberry32: tiny, seedable, good enough for paper. */
export function seeded(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Both corners' worth of pieces, at the corners, already moving. */
export function launch(width: number, height: number, colours: number, random: () => number = Math.random): Piece[] {
  const pieces: Piece[] = [];
  for (const left of [true, false]) {
    for (let i = 0; i < PER_CORNER; i++) {
      const up = ANGLE + (random() * 2 - 1) * SPREAD;
      const speed = height * (SPEED[0] + random() * SPEED[1]);
      pieces.push({
        x: left ? 0 : width,
        y: height,
        vx: Math.cos(up) * speed * (left ? 1 : -1),
        vy: -Math.sin(up) * speed,
        size: 6 + random() * 6,
        colour: Math.floor(random() * colours),
        round: random() < 0.3,
        angle: random() * Math.PI * 2,
        spin: (random() * 2 - 1) * 8,
        flip: random() * Math.PI * 2,
        flipSpeed: 6 + random() * 8,
        sway: (random() * 2 - 1) * 0.04 * height,
      });
    }
  }
  return pieces;
}

/** Advance every piece by `dt` seconds. Mutates: 220 pieces at 60 fps. */
export function step(pieces: Piece[], dt: number, height: number): void {
  const drag = Math.exp(-DRAG * dt);
  const terminal = TERMINAL * height;
  for (const p of pieces) {
    p.vx *= drag;
    p.vy = Math.min(p.vy * drag + GRAVITY * height * dt, terminal);
    p.flip += p.flipSpeed * dt;
    p.angle += p.spin * dt;
    // Sway only once falling, so the launch stays a clean arc.
    const sway = p.vy > 0 ? Math.sin(p.flip * 0.5) * p.sway : 0;
    p.x += (p.vx + sway) * dt;
    p.y += p.vy * dt;
  }
}

/** Opacity of the whole show at `elapsed` ms: full, then a fade to nothing. */
export function opacity(elapsed: number): number {
  const t = elapsed / CONFETTI_MS;
  if (t <= FADE_FROM) return 1;
  return Math.max(0, 1 - (t - FADE_FROM) / (1 - FADE_FROM));
}
