import { expect, test, type Page } from "./fixtures";
import { BANNER_MARGIN, menuHeight, paletteHeight } from "@takyon/shared";

/**
 * v0.2's visual layer: the Palette with Entries in it.
 *
 * TBC-0007's exposure applies throughout — **none of this catches a bug where the
 * UI is right and the Rust behind it is wrong.** Ranking lives in the Rust tests.
 * What this catches is whether a row draws and what the keyboard does.
 */

type Mock = {
  emitShow: () => void;
  emitHide: () => void;
  setIndexing: (on: boolean) => void;
  menuRequest: () => number | null;
  bannerRequest: () => number;
};

const setIndexing = (page: Page, on: boolean) =>
  page.evaluate((value) => {
    (window as unknown as { __takyon_mock: Mock }).__takyon_mock.setIndexing(value);
  }, on);

async function open(page: Page) {
  await page.goto("/?window=palette");
  await page.evaluate(() => {
    (window as unknown as { __takyon_mock: Mock }).__takyon_mock.emitShow();
  });
  return page.getByPlaceholder("Search");
}

/**
 * Grow the viewport the way Rust grows the window.
 *
 * `playwright.config.ts` pins 640x68, the *empty* Palette. Rust resizes the real
 * window on every query and the browser has no window manager, so the test uses
 * the shared `paletteHeight` and cannot drift from what ships.
 */
async function fitTo(
  page: Page,
  rows: number,
  indexing = false,
  menuActions: number | null = null,
  bannerHeight = 0,
) {
  await page.setViewportSize({
    width: 640,
    height: paletteHeight(rows, indexing, menuActions, bannerHeight),
  });
}

test("typing produces Entries", async ({ page }) => {
  const input = await open(page);
  await input.fill("photo");

  const rows = page.getByRole("option");
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toContainText("Adobe Photoshop");

  await fitTo(page, 1);
  await expect(page).toHaveScreenshot("palette-entries.png");
});

/**
 * Manual verification steps 1 to 3, as far as the mock can honestly carry them.
 * The rungs themselves are Rust's business; what is asserted here is that the
 * query reaches the list and the right row comes back selected.
 */
test("the three verification queries each select their app", async ({ page }) => {
  const input = await open(page);

  for (const [needle, title] of [
    ["phot", "Adobe Photoshop"],
    ["vsc", "Visual Studio Code"],
    ["code", "Visual Studio Code"],
  ] as const) {
    await input.fill(needle);
    await expect(page.getByRole("option").first()).toContainText(title);
  }
});

/**
 * The top Entry is selected without the user pressing anything. A launcher where
 * Enter does nothing until you press Down first is a launcher nobody uses twice.
 */
test("the first Entry is selected as soon as results arrive", async ({ page }) => {
  const input = await open(page);
  await input.fill("c");
  await expect(page.getByRole("option").first()).toHaveAttribute("data-selected", "true");
});

test("arrow keys move the selection and never leave the list", async ({ page }) => {
  const input = await open(page);
  // `c` reaches Calculator by name and Visual Studio Code by its "Code" word, so
  // there are two rows to move between. Picking a needle that matches nothing
  // would make every assertion below vacuously true.
  await input.fill("c");

  const rows = page.getByRole("option");
  const count = await rows.count();
  expect(count).toBeGreaterThan(1);

  await input.press("ArrowDown");
  await expect(rows.nth(1)).toHaveAttribute("data-selected", "true");

  // Past the end wraps rather than losing the selection entirely — every state
  // this list can be in must have exactly one selected row, or Enter has no
  // defined meaning.
  for (let i = 0; i < count + 2; i++) await input.press("ArrowDown");
  await expect(rows.locator("[data-selected=true]")).toHaveCount(1);
});

/**
 * ADR-0001: the Palette remembers nothing. v0.1 proved it for the query string;
 * this is the same guarantee for the list, which is new surface for state to
 * leak into.
 */
test("a second show clears the Entries as well as the query", async ({ page }) => {
  const input = await open(page);
  await input.fill("photo");
  await expect(page.getByRole("option")).toHaveCount(1);

  await page.evaluate(() => {
    (window as unknown as { __takyon_mock: Mock }).__takyon_mock.emitShow();
  });
  await expect(input).toHaveValue("");
  await expect(page.getByRole("option")).toHaveCount(0);
});

test("an empty query shows no Entries at all", async ({ page }) => {
  const input = await open(page);
  await input.fill("photo");
  await expect(page.getByRole("option")).toHaveCount(1);
  await input.fill("   ");
  await expect(page.getByRole("option")).toHaveCount(0);
});

test("a query matching nothing shows an empty list rather than stale rows", async ({
  page,
}) => {
  const input = await open(page);
  await input.fill("photo");
  await expect(page.getByRole("option")).toHaveCount(1);
  await input.fill("zzzznothing");
  await expect(page.getByRole("option")).toHaveCount(0);
});

/**
 * The walk is a background job and the Palette is not where it reports: the tray
 * tooltip and Settings → Applications say it instead.
 *
 * Reverses v0.2's call, whose cost is recorded in `docs/tbd/v0.9.md` §10.
 */
test("an in-progress walk does not put a status row in the palette", async ({ page }) => {
  const input = await open(page);
  await setIndexing(page, true);

  await input.fill("zzzznothing");
  await expect(page.getByText("Indexing applications…")).toHaveCount(0);
  // No reserved row either: an empty list is an empty window, not a blank strip.
  await expect(page.getByRole("option")).toHaveCount(0);

  await fitTo(page, 0);
  await expect(page).toHaveScreenshot("palette-indexing.png");

  // A real match still draws, walk or no walk.
  await input.fill("photo");
  await expect(page.getByRole("option").first()).toBeVisible();
});

/**
 * Manual verification step 6: `Ctrl+K` on a selected app offers Run as
 * administrator and Open file location.
 */
test("Ctrl+K opens the action menu with its accelerators listed", async ({ page }) => {
  const input = await open(page);
  await input.fill("photo");
  await input.press("Control+k");

  const menu = page.getByLabel("Actions");
  await expect(menu).toBeVisible();
  await expect(menu.getByText("Run as administrator")).toBeVisible();
  await expect(menu.getByText("Open file location")).toBeVisible();
  // Task 9: discoverable inside the menu, not folklore.
  await expect(menu.getByText("Ctrl+Enter")).toBeVisible();

  // The window the real app would be showing: one Entry row, grown to make room
  // for a four-action menu. Framing it at any other size would make the baseline
  // a picture of a window that never exists.
  await fitTo(page, 1, false, 4);
  await expect(page).toHaveScreenshot("palette-action-menu.png");
});

/**
 * A packaged app has no file, so every item in its menu must be something that
 * can actually happen. A menu offering "Open file location" for a Store app
 * teaches the user that the menu lies.
 */
test("a Store app is not offered actions that need a file", async ({ page }) => {
  const input = await open(page);
  await input.fill("calc");
  await expect(page.getByRole("option").first()).toContainText("Calculator");
  await input.press("Control+k");

  const menu = page.getByLabel("Actions");
  await expect(menu.getByText("Open")).toBeVisible();
  await expect(menu.getByText("Open file location")).toHaveCount(0);
  await expect(menu.getByText("Run as administrator")).toHaveCount(0);
});

/**
 * Escape means "back one step". From an open menu it closes the menu; the
 * Palette and the query survive. One keypress doing both would lose the query as
 * well as the menu, which is the bug the document-level Escape handler invites.
 */
test("Escape closes the action menu without dismissing the Palette", async ({ page }) => {
  const input = await open(page);
  await input.fill("photo");
  await input.press("Control+k");
  await expect(page.getByLabel("Actions")).toBeVisible();

  await page.keyboard.press("Escape");
  await expect(page.getByLabel("Actions")).toHaveCount(0);
  await expect(input).toHaveValue("photo");
  await expect(page.getByRole("option")).toHaveCount(1);
});

/**
 * Keyboard only, start to finish (manual verification step 7). No mouse is used
 * anywhere in this test — if any step needed one, this would hang rather than
 * quietly passing with a click.
 */
test("the whole flow works without a mouse", async ({ page }) => {
  const input = await open(page);
  await input.pressSequentially("photo");
  await expect(page.getByRole("option").first()).toHaveAttribute("data-selected", "true");
  await input.press("Control+k");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Escape");
  await input.press("Enter");
  // `activate` hides in the mock exactly as Rust hides before launching.
  await expect(page.getByRole("option")).toHaveCount(0);
});

/**
 * TBC-0006: the list stops growing at eight rows and scrolls beyond that. The
 * mock has five fixtures, so the cap itself is Rust's test — what is checked here
 * is that the container scrolls rather than the window being asked to grow
 * without limit.
 */
test("the list scrolls rather than growing without limit", async ({ page }) => {
  const input = await open(page);
  await input.fill("c");
  const list = page.getByRole("listbox");
  const overflow = await list.evaluate((el) => getComputedStyle(el).overflowY);
  expect(overflow).toBe("auto");
});

/**
 * The one thing this layer *can* say about the window-sizing bug.
 *
 * `menuHeight` is built from constants measured off the rendered menu, and
 * nothing notices when a CSS change makes them wrong — the symptom is an action
 * outside the native window. Slack upward only: empty pixels beat a clipped row.
 */
test("the reserved window height still covers the real action menu", async ({ page }) => {
  await page.setViewportSize({ width: 640, height: 600 });
  await page.goto("/?window=palette");
  await page.evaluate(() => {
    (window as unknown as { __takyon_mock: Mock }).__takyon_mock.emitShow();
  });
  const input = page.getByPlaceholder("Search");

  for (const [needle, actions] of [
    ["photo", 4],
    ["calc", 1],
  ] as const) {
    await input.fill(needle);
    await input.press("Control+k");
    const menu = page.getByRole("dialog").locator("[cmdk-root]");
    await expect(menu).toBeVisible();

    const box = await menu.boundingBox();
    expect(box).not.toBeNull();
    const reserved = menuHeight(actions);
    expect(box!.height).toBeLessThanOrEqual(reserved);
    // And not wildly more than it needs, or the window gains dead space every
    // time the menu opens.
    expect(box!.height).toBeGreaterThan(reserved - 32);

    await page.keyboard.press("Escape");
    await expect(page.getByRole("dialog")).toHaveCount(0);
  }
});

/**
 * The Palette must tell the window how many actions it is about to draw. What
 * Rust does with that is unit-tested there; what is checked here is that the
 * message is sent at all, and withdrawn on close.
 */
test("opening and closing the menu tells the window to resize", async ({ page }) => {
  const input = await open(page);
  const request = () =>
    page.evaluate(
      () => (window as unknown as { __takyon_mock: Mock }).__takyon_mock.menuRequest(),
    );

  await input.fill("photo");
  await input.press("Control+k");
  await expect(page.getByRole("dialog")).toBeVisible();
  expect(await request()).toBe(4);

  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(await request()).toBeNull();
});

/**
 * The banner shown when the hotkey could not be registered.
 *
 * Not hypothetical: Raycast holds `Alt+Space` here, so the first run of the real
 * binary drew it. It sits below the list in a content-sized window, so
 * under-reserving clips the sentence the user most needs to read.
 */
test("the banner reports its own measured height", async ({ page }) => {
  await page.setViewportSize({ width: 640, height: 600 });
  await page.goto("/?window=palette&hotkey=failed");
  await page.evaluate(() => {
    (window as unknown as { __takyon_mock: Mock }).__takyon_mock.emitShow();
  });

  const banner = page.getByRole("alert");
  await expect(banner).toBeVisible();
  await expect(banner).toContainText("Alt+Space could not be registered");

  const box = await banner.boundingBox();
  expect(box).not.toBeNull();

  // The number the window is sized from is the one the renderer measured — not a
  // constant, which is what under-reserved by 16px at 150% scaling and clipped
  // the list's last row.
  const reported = await page.evaluate(
    () => (window as unknown as { __takyon_mock: Mock }).__takyon_mock.bannerRequest(),
  );
  expect(reported).toBe(Math.ceil(box!.height));
  expect(paletteHeight(0, false, null, reported)).toBe(
    paletteHeight(0) + reported + BANNER_MARGIN,
  );

  await fitTo(page, 0, false, null, reported);
  await expect(page).toHaveScreenshot("palette-hotkey-failed.png");
});

/**
 * A narrower window wraps the sentence onto more lines, so the reported height
 * grows. This is the whole reason the number is measured rather than chosen: a
 * constant is right at exactly one width and DPI, and wrong everywhere else.
 */
test("a narrower window reports a taller banner", async ({ page }) => {
  await page.setViewportSize({ width: 640, height: 600 });
  await page.goto("/?window=palette&hotkey=failed");
  await expect(page.getByRole("alert")).toBeVisible();
  const read = () =>
    page.evaluate(
      () => (window as unknown as { __takyon_mock: Mock }).__takyon_mock.bannerRequest(),
    );

  const wide = await read();
  await page.setViewportSize({ width: 380, height: 600 });
  await expect
    .poll(read, { message: "the ResizeObserver should report the reflowed banner" })
    .toBeGreaterThan(wide);
});

/** A registered hotkey draws no banner, and reports zero rather than nothing. */
test("a working hotkey draws no banner and reserves no space", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("alert")).toHaveCount(0);
  const reported = await page.evaluate(
    () => (window as unknown as { __takyon_mock: Mock }).__takyon_mock.bannerRequest(),
  );
  expect(reported).toBe(0);
  expect(paletteHeight(1, false, null, 0)).toBe(paletteHeight(1));
});

/**
 * Two installs of one tool, told apart only by their version.
 *
 * The row renderer's job here is narrow and load-bearing: the version must be
 * visible and must survive a long title, because losing it turns two
 * distinguishable rows back into two identical ones.
 */
test("two installs of one tool show their versions", async ({ page }) => {
  const input = await open(page);
  await input.fill("node");

  const rows = page.getByRole("option");
  await expect(rows).toHaveCount(2);
  await expect(rows.filter({ hasText: "24.14.1" })).toHaveCount(1);
  await expect(rows.filter({ hasText: "26.7" })).toHaveCount(1);

  // The path must survive as a path. `dir="rtl"` reorders neutral characters,
  // and a backslash is neutral — if it ever stops rendering, this catches it.
  const nvm = rows.filter({ hasText: "24.14.1" });
  await expect(nvm).toContainText("nvm4w");
  expect(await nvm.innerText()).toContain(String.fromCharCode(92));

  await fitTo(page, 2);
  await expect(page).toHaveScreenshot("palette-versions.png");
});

test("a system settings page draws with an Open-only menu", async ({ page }) => {
  const input = await open(page);
  // Matches the Bluetooth settings page (a system entry) and nothing else.
  await input.fill("bluetooth");

  const rows = page.getByRole("option");
  await expect(rows).toHaveCount(1);
  const bt = rows.first();
  await expect(bt).toContainText("Bluetooth");

  // A settings page has no file, so the action menu offers Open alone — no
  // reveal, no copy path, nothing to elevate.
  await input.press("Control+k");
  const menu = page.getByLabel("Actions");
  await expect(menu.getByText("Open", { exact: true })).toBeVisible();
  await expect(menu.getByText("Open file location")).toHaveCount(0);
  await expect(menu.getByText("Run as administrator")).toHaveCount(0);

  // Escape closes the menu; the single system row remains for the screenshot.
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  await fitTo(page, 1);
  await expect(page).toHaveScreenshot("palette-system-entry.png");
});

/**
 * v0.4.5 task 4: the footer names what Enter will do on the selected row.
 *
 * The verb comes from Rust's table. This checks the frontend reads the right
 * action off the Entry — whether the word is spelled correctly is Rust's
 * business, and its own test.
 */
test("the footer names what Enter does, and follows the selection", async ({ page }) => {
  const input = await open(page);
  await input.fill("photo");

  const footer = page.getByText("Actions").locator("..");
  await expect(footer).toContainText("Open");
  await expect(footer).toContainText("Ctrl K");
});

test("the footer says Copy answer against a calculation", async ({ page }) => {
  const input = await open(page);
  await input.fill("12*1.18");
  await expect(page.getByText("Actions").locator("..")).toContainText("Copy answer");
});

/** No selected row over an empty Palette, so nothing to describe. */
test("an empty Palette draws no footer", async ({ page }) => {
  await open(page);
  await expect(page.getByText("Actions")).toHaveCount(0);
});

/**
 * v0.4.5 task 3: each row says what Kind it is, on the right.
 *
 * Always drawn rather than revealed on selection — revealing it would reflow
 * every row on every arrow key.
 */
test("a row is labelled with its Kind", async ({ page }) => {
  const input = await open(page);

  await input.fill("photo");
  await expect(page.getByRole("option").first()).toContainText("Application");

  await input.fill("bluetooth");
  await expect(page.getByRole("option").first()).toContainText("Settings");
});

/** A calculation is a card, and a card is not a row with a Kind column. */
test("a calculation carries no kind label", async ({ page }) => {
  const input = await open(page);
  await input.fill("12*1.18");
  await expect(page.getByRole("option").first()).not.toContainText("Application");
});
