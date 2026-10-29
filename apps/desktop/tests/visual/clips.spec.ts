import { expect, test, type Page } from "./fixtures";
import { paletteHeight, VIEW_HEIGHT } from "@takyon/shared";

/**
 * v0.5's visual layer: the `!v` clipboard view.
 *
 * TBC-0007's exposure is total here — **the clips are a fixture list in
 * `api.mock.ts`.** Encryption, capture and the sweep are answered against the
 * real file in `tests/clips_disk.rs`. This catches whether the view draws.
 */

type Mock = {
  emitShow: () => void;
  emitHide: () => void;
};

async function open(page: Page) {
  await page.goto("/?window=palette");
  await page.evaluate(() => {
    (window as unknown as { __takyon_mock: Mock }).__takyon_mock.emitShow();
  });
  return page.getByPlaceholder("Search");
}

const fitTo = (page: Page, rows: number) =>
  page.setViewportSize({ width: 640, height: paletteHeight(rows) });

test("!v lists clipboard history, newest first", async ({ page }) => {
  const input = await open(page);
  await input.fill("!v");

  const rows = page.getByRole("option");
  await expect(rows).toHaveCount(4);
  await expect(rows.first()).toContainText("github.com/tauri-apps");
  await expect(rows.first()).toHaveAttribute("data-selected", "true");
  // An image clip draws its thumbnail in the icon slot (v0.14).
  await expect(rows.nth(1).locator("img")).toBeVisible();

  await fitTo(page, 4);
  await expect(page).toHaveScreenshot("palette-clips.png");
});

/** The Bang is a Mode, so the rest of the line searches inside it. */
test("!v with a query filters the history", async ({ page }) => {
  const input = await open(page);
  await input.fill("!v select");
  await expect(page.getByRole("option")).toHaveCount(1);
  await expect(page.getByRole("option").first()).toContainText("SELECT id");
});

/**
 * ADR-0006 in the one place a user could see it broken. The fixtures deliberately
 * contain a string an ordinary query would match if Clips were ever ranked.
 */
test("a Bangless query never returns a clip", async ({ page }) => {
  const input = await open(page);
  await input.fill("com.v3sper.takyon");
  await expect(page.getByRole("option")).toHaveCount(0);
});

test("a clip row is labelled and the footer names Paste", async ({ page }) => {
  const input = await open(page);
  await input.fill("!v");

  await expect(page.getByRole("option").first()).toContainText("Clip");
  // The footer reads the Entry's first action, so this is also the assertion
  // that a Clip's actions are ordered with Paste first.
  await expect(page.getByText("Paste", { exact: true })).toBeVisible();
});

/**
 * The Raycast shape, and the reason this phase was reworked: clipboard history is
 * a **command in ordinary results**, not something you have to know a Bang for.
 */
test("Clipboard History is findable Bangless, by name and by keyword", async ({ page }) => {
  const input = await open(page);

  await input.fill("his");
  const row = page.getByRole("option").first();
  await expect(row).toContainText("Clipboard History");
  await expect(row).toContainText("Takyon");
  await expect(row).toContainText("Command");

  await input.fill("clipboard");
  await expect(page.getByRole("option").first()).toContainText("Clipboard History");
  // The footer names what Enter does, and it is not "Open".
  await expect(page.getByText("Open Command", { exact: true })).toBeVisible();
});

test("Enter on the command opens the history surface", async ({ page }) => {
  const input = await open(page);
  await input.fill("clipboard");
  await page.setViewportSize({ width: 640, height: VIEW_HEIGHT });
  await input.press("Enter");

  // The two-pane surface: filter, type control, day grouping, detail pane.
  await expect(page.getByPlaceholder("Type to filter entries...")).toBeVisible();
  await expect(page.getByRole("combobox", { name: "Type" })).toBeVisible();
  await expect(page.getByText("Today")).toBeVisible();
  await expect(page.getByText("Information")).toBeVisible();
  await expect(page.getByText("Source")).toBeVisible();
  // The app name, not the path it was launched from. A full path here is both
  // unreadable and a wider metadata leak than ADR-0008 accepted.
  await expect(page.getByText(/^firefox$/i)).toBeVisible();
  // Scoped to the list: the type `<select>` also contains `option` elements, and
  // an unscoped query matches "All Types" first.
  const rows = page.getByRole("listbox", { name: "Clipboard history" }).getByRole("option");
  await expect(rows.first()).toHaveAttribute("aria-selected", "true");
  await expect(rows).toHaveCount(4);
  // Delete is visible, not only a chord (v0.14): before it, a clip could only
  // go by waiting out retention unless you knew the chord.
  await expect(page.getByRole("button", { name: "Delete from history" })).toBeVisible();
  await expect(page.getByText("Ctrl D", { exact: true })).toBeVisible();
  await expect(page).toHaveScreenshot("clipboard-history.png");
});

async function openHistory(page: Page) {
  const input = await open(page);
  await input.fill("clipboard");
  await page.setViewportSize({ width: 640, height: VIEW_HEIGHT });
  await input.press("Enter");
  return page.getByRole("listbox", { name: "Clipboard history" }).getByRole("option");
}

test("an image clip shows its thumbnail, the image and its size", async ({ page }) => {
  const rows = await openHistory(page);
  const image = rows.filter({ hasText: "Image 1920" });
  await expect(image.locator("img")).toBeVisible();
  await image.click();

  await expect(page.getByRole("img", { name: "Image 1920 × 1080" })).toBeVisible();
  await expect(page.getByText("Dimensions")).toBeVisible();
  await expect(page.getByText("1920 × 1080", { exact: true })).toBeVisible();
  await expect(page.getByText("1.4 MB")).toBeVisible();
  await expect(page.getByText("Characters")).toHaveCount(0);
  await expect(page).toHaveScreenshot("clipboard-history-image.png");
});

test("the type filter narrows to images", async ({ page }) => {
  const rows = await openHistory(page);
  await page.getByRole("combobox", { name: "Type" }).click();
  await page.getByRole("option", { name: "Images" }).click();
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toContainText("Image 1920");
});

/** The reported bug: yesterday's clip could not be removed until retention. */
test("Delete removes one clip and selects its neighbour", async ({ page }) => {
  const rows = await openHistory(page);
  await rows.filter({ hasText: "SELECT id" }).click();
  await page.getByRole("button", { name: "Delete from history" }).click();

  await expect(rows).toHaveCount(3);
  await expect(rows.filter({ hasText: "SELECT id" })).toHaveCount(0);
  // The row below takes the selection, not the newest: mid-cleanup, the user
  // keeps their place.
  await expect(rows.filter({ hasText: "com.v3sper.takyon" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
});

test("Ctrl+D deletes the selected clip", async ({ page }) => {
  const rows = await openHistory(page);
  await page.getByPlaceholder("Type to filter entries...").press("Control+d");
  await expect(rows).toHaveCount(3);
  await expect(rows.filter({ hasText: "github.com" })).toHaveCount(0);
});

/** The footer advertised Ctrl K here before it did anything. */
test("Ctrl+K in the history opens the action menu, and Delete works from it", async ({
  page,
}) => {
  const rows = await openHistory(page);
  await page.getByPlaceholder("Type to filter entries...").press("Control+k");
  const menu = page.getByLabel("Actions");
  await expect(menu.getByText("Paste")).toBeVisible();
  await expect(menu.getByText("Delete from history")).toBeVisible();

  await menu.getByText("Delete from history").click();
  await expect(menu).toHaveCount(0);
  await expect(rows).toHaveCount(3);
  // Focus is back in the filter, so typing carries on filtering.
  await expect(page.getByPlaceholder("Type to filter entries...")).toBeFocused();
});

test("the surface filters, and Escape goes back rather than dismissing", async ({
  page,
}) => {
  const input = await open(page);
  await input.fill("clipboard");
  await page.setViewportSize({ width: 640, height: VIEW_HEIGHT });
  await input.press("Enter");

  const filter = page.getByPlaceholder("Type to filter entries...");
  await filter.fill("SELECT");
  const rows = page.getByRole("listbox", { name: "Clipboard history" }).getByRole("option");
  await expect(rows).toHaveCount(1);

  await filter.press("Escape");
  // Back to the root search, empty, rather than a dismissed window.
  await expect(page.getByPlaceholder("Search")).toBeVisible();
  await expect(page.getByPlaceholder("Search")).toHaveValue("");
});

/**
 * The highlight moved but the list did not: arrowing past the bottom of the
 * history left the selected row off-screen. Four clips do not overflow a full
 * window, so the viewport is shrunk until the list must scroll.
 */
test("arrow keys scroll the history list to keep the selected row in view", async ({ page }) => {
  const rows = await openHistory(page);
  await page.setViewportSize({ width: 640, height: 220 });

  const list = page.getByRole("listbox", { name: "Clipboard history" });
  expect(await list.evaluate((el) => el.scrollHeight > el.clientHeight)).toBe(true);

  // Inside the list's own box, not the page's: the page never scrolls here.
  const inList = (row: ReturnType<typeof rows.last>) =>
    row.evaluate((el) => {
      const box = el.closest('[role="listbox"]')!.getBoundingClientRect();
      const r = el.getBoundingClientRect();
      return r.top >= box.top && r.bottom <= box.bottom;
    });

  const filter = page.getByPlaceholder("Type to filter entries...");
  for (let i = 0; i < 3; i++) await filter.press("ArrowDown");
  await expect(rows.last()).toHaveAttribute("aria-selected", "true");
  expect(await inList(rows.last())).toBe(true);
  expect(await list.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);

  for (let i = 0; i < 3; i++) await filter.press("ArrowUp");
  await expect(rows.first()).toHaveAttribute("aria-selected", "true");
  expect(await inList(rows.first())).toBe(true);
  // Back to the top edge of the content: the heading sits under the list's own
  // padding, which is allowed to stay hidden.
  expect(
    await list.evaluate((el) => el.scrollTop <= parseFloat(getComputedStyle(el).paddingTop)),
  ).toBe(true);
});

test("Ctrl+K on a clip offers paste, copy and delete", async ({ page }) => {
  const input = await open(page);
  await input.fill("!v");
  await input.press("Control+k");

  const menu = page.getByLabel("Actions");
  await expect(menu.getByText("Paste")).toBeVisible();
  await expect(menu.getByText("Copy to clipboard")).toBeVisible();
  await expect(menu.getByText("Delete from history")).toBeVisible();
  await expect(menu.getByText("Ctrl+D")).toBeVisible();
  // Nothing that touches a file: a Clip has no path, and offering one would put
  // clipboard content into Explorer as a path.
  await expect(menu.getByText("Open file location")).toHaveCount(0);
  await expect(menu.getByText("Copy path")).toHaveCount(0);
});
