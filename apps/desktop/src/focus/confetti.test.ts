import { describe, expect, test } from "bun:test";
import { CONFETTI_MS, launch, opacity, seeded, step } from "./confetti";

const W = 1920;
const H = 1080;

/** Fly a seeded show to `ms`, frame by frame at 60 fps. */
function fly(ms: number, width = W, height = H) {
  const pieces = launch(width, height, 6, seeded(7));
  const top = pieces.map((p) => p.y);
  for (let t = 0; t < ms / 1000; t += 1 / 60) {
    step(pieces, 1 / 60, height);
    pieces.forEach((p, i) => (top[i] = Math.min(top[i] ?? p.y, p.y)));
  }
  return { pieces, top };
}

describe("confetti", () => {
  test("half the pieces leave each bottom corner, aimed up and inward", () => {
    const pieces = launch(W, H, 6, seeded(1));
    const left = pieces.filter((p) => p.x === 0);
    const right = pieces.filter((p) => p.x === W);
    expect(left.length).toBe(pieces.length / 2);
    expect(right.length).toBe(pieces.length / 2);
    expect(pieces.every((p) => p.y === H && p.vy < 0)).toBe(true);
    expect(left.every((p) => p.vx > 0)).toBe(true);
    expect(right.every((p) => p.vx < 0)).toBe(true);
  });

  test("the typical piece climbs past a third of the screen, none off the top", () => {
    const { top } = fly(CONFETTI_MS);
    const heights = top.map((y) => (H - y) / H).sort((a, b) => a - b);
    expect(heights[Math.floor(heights.length / 2)]).toBeGreaterThan(0.4);
    expect(heights[heights.length - 1]).toBeLessThan(1);
  });

  test("each corner's pieces stay on their own side of the far edge", () => {
    const { pieces } = fly(CONFETTI_MS);
    const half = pieces.length / 2;
    expect(pieces.slice(0, half).every((p) => p.x < W)).toBe(true);
    expect(pieces.slice(half).every((p) => p.x > 0)).toBe(true);
  });

  test("the arc scales with the screen: a 4K monitor gets the same shape", () => {
    const small = fly(1_000).pieces;
    const big = fly(1_000, W * 2, H * 2).pieces;
    small.forEach((p, i) => {
      expect(big[i]!.y / (H * 2)).toBeCloseTo(p.y / H, 5);
    });
  });

  test("it fades to nothing by the end of the show and stays gone", () => {
    expect(opacity(0)).toBe(1);
    expect(opacity(CONFETTI_MS * 0.5)).toBe(1);
    expect(opacity(CONFETTI_MS * 0.85)).toBeGreaterThan(0);
    expect(opacity(CONFETTI_MS * 0.85)).toBeLessThan(1);
    expect(opacity(CONFETTI_MS)).toBe(0);
    expect(opacity(CONFETTI_MS * 2)).toBe(0);
  });
});
