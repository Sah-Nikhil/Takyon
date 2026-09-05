import { expect, test, type Page } from "./fixtures";
import { VIEW_HEIGHT } from "@takyon/shared";

/**
 * The app's own dropdown (v0.9), driven where it is hardest: inside the Palette.
 *
 * A native `<select>` popup is drawn by the OS, so none of this was reachable
 * before. The Palette is the demanding case because Escape means two different
 * things there, and because the window is 640px with nowhere to overflow into.
 */

type Mock = { emitShow: () => void };

async function history(page: Page) {
  await page.goto("/?window=palette");
  await page.evaluate(() => {
    (window as unknown as { __takyon_mock: Mock }).__takyon_mock.emitShow();
  });
  await page.setViewportSize({ width: 640, height: VIEW_HEIGHT });
  const input = page.getByPlaceholder("Search");
  await input.fill("clipboard");
  await input.press("Enter");
  await expect(page.getByPlaceholder("Type to filter entries...")).toBeVisible();
  return page.getByRole("combobox", { name: "Type" });
}

test("the list opens inside the palette and stays in the window", async ({ page }) => {
  const type = await history(page);
  await type.click();

  const list = page.getByRole("listbox", { name: "Type" });
  await expect(list).toBeVisible();
  // Inside the window, not clipped by it: a popup that opens past the bottom of
  // a fixed-height surface is a list with rows nobody can reach.
  const box = await list.boundingBox();
  expect(box).not.toBeNull();
  expect(box!.y).toBeGreaterThanOrEqual(0);
  expect(box!.y + box!.height).toBeLessThanOrEqual(VIEW_HEIGHT);

  await expect(page).toHaveScreenshot("palette-clips-type-open.png");
});

/**
 * Escape means two things here and the inner one has to win: close the list, and
 * leave the surface it is drawn on alone.
 */
test("escape closes the list without dismissing the surface", async ({ page }) => {
  const type = await history(page);
  await type.click();
  await expect(page.getByRole("listbox", { name: "Type" })).toBeVisible();

  await page.keyboard.press("Escape");
  await expect(page.getByRole("listbox", { name: "Type" })).toHaveCount(0);
  // Still on the history surface, and still the same query.
  await expect(page.getByPlaceholder("Type to filter entries...")).toBeVisible();

  // A second Escape now reaches the surface, which is the behaviour it had
  // before a dropdown was ever opened.
  await page.keyboard.press("Escape");
  await expect(page.getByPlaceholder("Type to filter entries...")).toHaveCount(0);
});

test("a click outside closes the list and changes nothing", async ({ page }) => {
  const type = await history(page);
  await type.click();
  await expect(page.getByRole("listbox", { name: "Type" })).toBeVisible();

  await page.getByPlaceholder("Type to filter entries...").click();
  await expect(page.getByRole("listbox", { name: "Type" })).toHaveCount(0);
  await expect(type).toHaveText("All Types");
});

test("choosing a row filters the list and closes", async ({ page }) => {
  const type = await history(page);
  await type.click();
  await page.getByRole("listbox", { name: "Type" }).getByRole("option", { name: "Text" }).click();

  await expect(type).toHaveText("Text");
  await expect(page.getByRole("listbox", { name: "Type" })).toHaveCount(0);
});

test.describe("in settings", () => {
  test.use({ viewport: { width: 880, height: 620 } });

  /** A disabled control has nothing to show, so it must not open at all. */
  test("a disabled dropdown does not open", async ({ page }) => {
    await page.goto("/?window=settings");
    await page.getByRole("button", { name: "Agents" }).click();

    // opencode is signed out, so it has no pickers at all to disable. Codex is
    // not installed. Claude's is the only enabled one, which is the control.
    const claude = page.getByRole("combobox", { name: "Model for Claude Code" });
    await expect(claude).toBeEnabled();
    await claude.click();
    await expect(page.getByRole("listbox", { name: "Model for Claude Code" })).toBeVisible();
    await page.keyboard.press("Escape");

    // Switching the Agent off removes its pickers entirely, which is the
    // stronger version of disabled and what the page actually does.
    await page.getByRole("switch", { name: "Use Claude Code for !c" }).click();
    await expect(claude).toHaveCount(0);
  });

  /** Typeahead is the native behaviour people notice only when it is missing. */
  test("typing jumps to a row by name", async ({ page }) => {
    await page.goto("/?window=settings");
    await page.getByRole("button", { name: "Agents" }).click();

    const model = page.getByRole("combobox", { name: "Model for Claude Code" });
    await model.click();
    await page.keyboard.type("hai");
    await page.keyboard.press("Enter");
    await expect(model).toHaveText("haiku");
  });
});
