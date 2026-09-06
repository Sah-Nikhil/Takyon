import { expect, test } from "./fixtures";

/**
 * Expanded mode (v0.10): the Palette's second shape.
 *
 * The height itself is Rust's, asserted in `window.rs`. What this layer proves
 * is what the height buys: category headings, a first view, and a mark that
 * knows it is not idle.
 */
test.use({ viewport: { width: 640, height: 520 } });

/** Show the Palette in Expanded mode, as if Settings had stored it. */
async function openExpanded(page: import("@playwright/test").Page) {
  await page.goto("/?window=palette");
  await page.evaluate(() => {
    const m = (
      window as unknown as {
        __takyon_mock: {
          setStoredPreference: (p: { windowMode: string }) => void;
          emitShow: () => void;
        };
      }
    ).__takyon_mock;
    m.setStoredPreference({ windowMode: "expanded" });
    m.emitShow();
  });
}

/**
 * The whole reason the mode exists. A 520px window answering an empty line with
 * nothing is a hole; Compact answering it with nothing is correct (ADR-0001),
 * and the difference is what Expanded had to earn.
 */
test("the first view suggests, and groups what it suggests", async ({ page }) => {
  await openExpanded(page);

  await expect(page.getByPlaceholder("Search")).toHaveValue("");
  /*
    Headings, not just rows: the grouping is the mode's other half.

    Located by `data-group-heading`, not by text: a Settings *section* sits over
    rows each labelled Settings, so matching on the word finds both and proves
    neither.
   */
  await expect(page.locator("[data-group-heading]")).toHaveCount(3);
  await expect(page.locator('[data-group-heading="app"]')).toHaveText("Applications");
  await expect(page.locator('[data-group-heading="command"]')).toHaveText("Commands");
  await expect(page.locator('[data-group-heading="system"]')).toHaveText("Settings");

  await expect(page).toHaveScreenshot("palette-expanded-suggestions.png");
});

/**
 * The idle beat is gated on the query being empty, which in Expanded is also the
 * state with eleven rows under it. A mark breathing over a full list reads as
 * "working" while the shell is idle and waiting — the exact lie `docs/brand.md`
 * says motion must never tell.
 */
test("the mark holds still over the first view", async ({ page }) => {
  await openExpanded(page);

  await expect(page.locator('[data-particle-pulse="true"]')).toHaveCount(0);

  // And Compact, with the same empty query and no rows, still breathes.
  await page.goto("/?window=palette");
  await page.evaluate(() => {
    (window as unknown as { __takyon_mock: { emitShow: () => void } }).__takyon_mock.emitShow();
  });
  await expect(page.locator('[data-particle-pulse="true"]')).toHaveCount(1);
});

/** Typing replaces the suggestions with results, still grouped. */
test("results are grouped once something is typed", async ({ page }) => {
  await openExpanded(page);
  await page.getByPlaceholder("Search").fill("c");

  await expect(page.locator('[data-group-heading="app"]')).toBeVisible();
  await expect(page).toHaveScreenshot("palette-expanded-results.png");
});

/**
 * Compact draws one flat list and no headings. Stated as a test because the
 * grouping is a single ternary in `Palette.tsx`, and inverting it would look
 * entirely reasonable in a diff.
 */
test("Compact draws no headings", async ({ page }) => {
  await page.goto("/?window=palette");
  await page.evaluate(() => {
    (window as unknown as { __takyon_mock: { emitShow: () => void } }).__takyon_mock.emitShow();
  });
  await page.getByPlaceholder("Search").fill("photo");

  await expect(page.getByText("Adobe Photoshop")).toBeVisible();
  await expect(page.locator("[data-group-heading]")).toHaveCount(0);
});
