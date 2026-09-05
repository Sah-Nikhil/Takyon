import { expect, test, type Page } from "./fixtures";
import { paletteHeight } from "@takyon/shared";

/**
 * v0.4's visual layer: the calculator row, and what Enter sends.
 *
 * TBC-0007's exposure is sharpest here: **the answers come from a fixture table
 * in `api.mock.ts`, not the parser.** Whether `12*1.18` is 14.16 is a Rust
 * question, settled in `sources/calc`. This catches whether the row draws.
 */

type Mock = {
  emitShow: () => void;
  emitHide: () => void;
  setIndexing: (on: boolean) => void;
  menuRequest: () => number | null;
  bannerRequest: () => number;
  calcPolicyRequest: () => string;
};

async function open(page: Page) {
  await page.goto("/?window=palette");
  await page.evaluate(() => {
    (window as unknown as { __takyon_mock: Mock }).__takyon_mock.emitShow();
  });
  return page.getByPlaceholder("Search");
}

/** A calculation is a card, so the viewport has to be told (v0.4.5). */
const fitTo = (page: Page, rows: number, calcCard = false) =>
  page.setViewportSize({
    width: 640,
    height: paletteHeight(rows, false, null, 0, calcCard),
  });

test("a calculation draws as the top row, with its expression beneath", async ({ page }) => {
  const input = await open(page);
  await input.fill("12*1.18");

  const rows = page.getByRole("option");
  await expect(rows.first()).toContainText("14.16");
  // The expression is on the row, not only in the input. A result with nothing
  // beside it is a number you have to trust.
  await expect(rows.first()).toContainText("12*1.18");
  await expect(rows.first()).toHaveAttribute("data-selected", "true");

  await fitTo(page, 1, true);
  await expect(page).toHaveScreenshot("palette-calc.png");
});

test("a unit conversion draws with its unit on the answer", async ({ page }) => {
  const input = await open(page);
  await input.fill("40 kg to lb");
  await expect(page.getByRole("option").first()).toContainText("88.1849 lb");
});

/**
 * The labels are the card's legend. Without them it is two numbers and an arrow,
 * and which one you typed is a guess.
 */
test("a calculation is drawn as a card, with both halves labelled", async ({ page }) => {
  const input = await open(page);
  await input.fill("12*1.18");

  // The labels are what make the card readable without a legend: which number is
  // the sum you typed and which is the answer.
  const card = page.getByRole("option").first();
  await expect(card).toContainText("Calculator");
  await expect(card).toContainText("Expression");
  await expect(card).toContainText("Result");
});

/**
 * The card is still a `Command.Item`, which is the whole trick — it keeps arrow
 * keys, Enter and `Ctrl+K` working without a second selection model. If this
 * fails, the card has become its own surface.
 */
test("the card is an ordinary list item and stays selectable", async ({ page }) => {
  const input = await open(page);
  await input.fill("12*1.18");

  const rows = page.getByRole("option");
  await expect(rows.first()).toHaveAttribute("data-selected", "true");
  await expect(rows.first()).toContainText("14.16");
});

/**
 * The `2022` case from the live Raycast screenshots, drawn rather than argued
 * about: the calculation takes the top row from the application named after the
 * same year, and Enter goes with it. The Automatic Policy's known cost.
 */
test("a calculation outranks an application that matches the same text", async ({ page }) => {
  const input = await open(page);
  await input.fill("2024");

  const rows = page.getByRole("option");
  await expect(rows.first()).toContainText("2,024");
  await expect(rows.first()).toHaveAttribute("data-selected", "true");
});

/**
 * Enter on a calculation must send `copy_answer`, not `open`. Sending `open`
 * reaches a Rust arm that refuses it, which would surface as nothing happening —
 * the failure mode that is hardest to notice and easiest to ship.
 */
test("Ctrl+K on a calculation offers copying the answer, on Enter", async ({ page }) => {
  const input = await open(page);
  await input.fill("12*1.18");
  await page.keyboard.press("Control+k");

  const menu = page.getByRole("dialog");
  await expect(menu).toContainText("Copy answer");
  await expect(menu).toContainText("Enter");
  // One row: nothing to launch, elevate or reveal, and a menu item that can only
  // fail teaches users the menu lies.
  await expect(menu.getByRole("option")).toHaveCount(1);
});

/*
  "the Palette tells Rust which calculator Policy is remembered" was here through
  v0.5. The behaviour is gone deliberately: v0.6 stores the Policy and Rust reads
  it at startup, so pushing it on every show was a SQLite write per summon. Its
  guarantee moved to `v0_6_a_stored_calculator_policy_is_what_startup_reads`.
*/

/**
 * The Settings control and the Palette are two windows sharing `settings.db` and
 * nothing else. This asserts the half the browser build can see: choosing a Mode
 * pushes it across the seam, which is what makes it take effect without waiting
 * for a restart.
 */
test("the Calculator page drives the Policy", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Calculator" }).click();
  await page.getByRole("radio", { name: "After =" }).click();

  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as unknown as { __takyon_mock: Mock }).__takyon_mock.calcPolicyRequest(),
      ),
    )
    .toBe("explicit");
});
