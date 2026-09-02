/**
 * The window-sizing arithmetic, from the TypeScript side.
 *
 * `paletteHeight` is mirrored by Rust's `window::content_height`, and the two have
 * to agree exactly: Rust resizes the real window, and the Playwright suite resizes
 * the browser viewport to match. A Rust test asserts the *constants* are the same
 * on both sides; this asserts the *formula* built from them, so a change to one
 * implementation without the other fails on whichever side was not updated.
 *
 * Worth having in TypeScript specifically because every bug this function has had
 * so far was a piece of chrome nobody counted — the list's border, the action
 * menu, the hotkey banner — and each was found by looking at a rendered window,
 * not by reading the Rust.
 */

import { expect, test } from "bun:test";
import {
  BANNER_MARGIN,
  FOOTER_HEIGHT,
  CALC_CAPTION_HEIGHT,
  CALC_CARD_HEIGHT,
  EMPTY_HEIGHT,
  LIST_CHROME,
  MAX_VISIBLE_ROWS,
  ROW_HEIGHT,
  menuHeight,
  paletteHeight,
} from "./ipc";

test("an empty Palette is one input row", () => {
  expect(paletteHeight(0)).toBe(EMPTY_HEIGHT);
});

test("the window grows a row at a time", () => {
  expect(paletteHeight(1)).toBe(EMPTY_HEIGHT + ROW_HEIGHT + LIST_CHROME + FOOTER_HEIGHT);
  expect(paletteHeight(3) - paletteHeight(2)).toBe(ROW_HEIGHT);
});

test("growth stops at the visible-row cap and the rest scroll", () => {
  // IMPLEMENTATION_PLAN §3 ranks twelve Entries and all twelve stay reachable by
  // arrow key. This is only how many are on screen at once (TBC-0006).
  const capped = paletteHeight(MAX_VISIBLE_ROWS);
  expect(paletteHeight(MAX_VISIBLE_ROWS + 1)).toBe(capped);
  expect(paletteHeight(12)).toBe(capped);
  expect(paletteHeight(999)).toBe(capped);
});

test("the indexing notice occupies exactly one row", () => {
  // So the window does not jump when the walk finishes and real Entries take its
  // place.
  expect(paletteHeight(0, true)).toBe(paletteHeight(1));
  // Once there are Entries, the notice is no longer what sets the height.
  expect(paletteHeight(3, true)).toBe(paletteHeight(3));
});

test("opening the action menu grows a short Palette to fit it", () => {
  // The bug this exists for: a four-action menu is about 200px and a one-row
  // Palette is 121px, so without growing the window the last two actions are cut
  // off at its bottom edge.
  expect(paletteHeight(1, false, 4)).toBe(menuHeight(4));
  expect(paletteHeight(1, false, 4)).toBeGreaterThan(paletteHeight(1));
});

test("a tall Palette does not grow further for a menu", () => {
  // The menu overlays the list rather than sitting below it, so a Palette that is
  // already tall enough must not lurch every time the menu opens.
  const tall = paletteHeight(MAX_VISIBLE_ROWS);
  expect(paletteHeight(MAX_VISIBLE_ROWS, false, 4)).toBe(tall);
});

test("the hotkey banner adds its own space rather than overlapping", () => {
  expect(paletteHeight(1, false, null, 57)).toBe(paletteHeight(1) + 57 + BANNER_MARGIN);
  // And it stacks with the menu, which does overlap.
  expect(paletteHeight(1, false, 4, 57)).toBe(menuHeight(4) + 57 + BANNER_MARGIN);
});

test("the banner height is taken from the caller, not assumed", () => {
  // The bug that reached the real window: the same sentence wraps to two lines at
  // 100% scaling and three at 150%, and a constant cannot know which happened.
  // The window follows the measurement.
  expect(paletteHeight(1, false, null, 73) - paletteHeight(1, false, null, 57)).toBe(
    73 - 57,
  );
  // Zero means no banner at all, not a zero-height one.
  expect(paletteHeight(1, false, null, 0)).toBe(paletteHeight(1));
});

test("the list chrome counts the border as well as the padding", () => {
  // The list is a border-box element with an explicit height, so its padding and
  // its 1px top border sit inside that height. Reserving only the padding leaves
  // the content one pixel taller than its box, and a list that fits exactly grows
  // a scrollbar.
  expect(LIST_CHROME).toBe(9);
  expect(paletteHeight(1) - EMPTY_HEIGHT - ROW_HEIGHT - FOOTER_HEIGHT).toBe(LIST_CHROME);
});

test("a calculation is sized as a card rather than a row", () => {
  // Mirrored by `window::content_height`; a Rust test asserts the constants
  // agree. This side only has to agree about the arithmetic.
  expect(paletteHeight(1, false, null, 0, true)).toBe(
    EMPTY_HEIGHT + CALC_CAPTION_HEIGHT + CALC_CARD_HEIGHT + LIST_CHROME + FOOTER_HEIGHT,
  );
  expect(paletteHeight(1, false, null, 0, true)).toBeGreaterThan(paletteHeight(1));
});

test("the card replaces a row rather than adding one", () => {
  const withOneApp = paletteHeight(2, false, null, 0, true);
  expect(withOneApp - paletteHeight(1, false, null, 0, true)).toBe(ROW_HEIGHT);
});

test("the row cap applies to what is left after the card", () => {
  // MAX_VISIBLE_ROWS counts rows and a card is not one, so eight rows *plus* a
  // card would exceed the shape TBC-0006 settled on.
  const capped = paletteHeight(MAX_VISIBLE_ROWS + 1, false, null, 0, true);
  expect(paletteHeight(999, false, null, 0, true)).toBe(capped);
  expect(capped).toBe(
    EMPTY_HEIGHT +
      CALC_CAPTION_HEIGHT +
      CALC_CARD_HEIGHT +
      MAX_VISIBLE_ROWS * ROW_HEIGHT +
      LIST_CHROME +
      FOOTER_HEIGHT,
  );
});

test("the footer is drawn with the list and only with it", () => {
  // Raycast shows none over an empty Palette either: there is no selected row to
  // describe. Mirrored by `window::content_height`, constants asserted in Rust.
  expect(paletteHeight(0)).toBe(EMPTY_HEIGHT);
  expect(paletteHeight(1) - paletteHeight(0)).toBe(
    ROW_HEIGHT + LIST_CHROME + FOOTER_HEIGHT,
  );
  // One strip, however many rows are under it.
  expect(paletteHeight(2) - paletteHeight(1)).toBe(ROW_HEIGHT);
});
