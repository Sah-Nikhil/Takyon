import { expect, test } from "./fixtures";
import { VIEW_HEIGHT } from "@takyon/shared";

/**
 * The answer renderer (v0.9). Agents write markdown whatever they are asked
 * for, and this used to arrive with its asterisks intact.
 */

type Mock = { emitShow: () => void; setAnswer: (text: string) => void };

const MARKDOWN =
  "The next **total solar eclipse** is **August 2, 2027**.\n" +
  "Its path crosses *southern Spain* and `Luxor`.\n\n" +
  "A second paragraph, to prove the break survives.";

async function answered(page: import("@playwright/test").Page) {
  await page.goto("/?window=palette");
  await page.evaluate((text) => {
    const m = (window as unknown as { __takyon_mock: Mock }).__takyon_mock;
    m.setAnswer(text);
    m.emitShow();
  }, MARKDOWN);
  await page.setViewportSize({ width: 640, height: VIEW_HEIGHT });
  const input = page.getByPlaceholder("Search");
  await input.fill("!c when is the next eclipse");
  await input.press("Enter");
  return input;
}

test("bold, italic and code render rather than printing their marks", async ({ page }) => {
  await answered(page);

  await expect(page.getByText("total solar eclipse")).toHaveJSProperty("tagName", "STRONG");
  await expect(page.getByText("southern Spain")).toHaveJSProperty("tagName", "EM");
  await expect(page.getByText("Luxor")).toHaveJSProperty("tagName", "CODE");
  // The marks themselves are gone from the rendered text.
  await expect(page.getByText("**", { exact: false })).toHaveCount(0);

  await expect(page).toHaveScreenshot("palette-ask-markdown.png");
});

/** A blank line is a paragraph, and a single newline is a line break. */
test("paragraph and line breaks survive", async ({ page }) => {
  await answered(page);
  const answer = page.locator("p", { hasText: "A second paragraph" });
  await expect(answer).toBeVisible();
  await expect(page.locator("p", { hasText: "total solar eclipse" })).toBeVisible();
  // Two paragraphs, not one run-together block.
  await expect(page.locator("p", { hasText: "A second paragraph" })).toHaveCount(1);
});
