import { expect, test, type Page } from "./fixtures";
import { paletteHeight } from "@takyon/shared";

/**
 * The Palette's update notice (ADR-0040): Compact pill, Expanded row.
 *
 * The notice comes from Rust's memory on the show event, after a background
 * check. Mocked here; the check, the daily schedule and the window height for
 * the row are Rust's, tested in `update.rs`, `tests/updater.rs` and `window.rs`.
 */

type Mock = {
  emitShow: () => void;
  setUpdateNotice: (n: unknown) => void;
  setUpdateScenario: (s: string) => void;
  setStoredPreference: (p: { windowMode: string }) => void;
  openedUrls: () => string[];
};

const AVAILABLE = { kind: "available", version: "1.0.0" };
const WHATS_NEW = {
  kind: "whatsNew",
  version: "0.12.4",
  url: "https://github.com/acme/takyon/releases/tag/v0.12.4",
};

async function show(page: Page, notice: unknown, expanded = false, scenario = "available") {
  await page.goto("/?window=palette");
  await page.evaluate(
    ([n, e, s]) => {
      const m = (window as unknown as { __takyon_mock: Mock }).__takyon_mock;
      m.setUpdateNotice(n);
      m.setUpdateScenario(s as string);
      if (e) m.setStoredPreference({ windowMode: "expanded" });
      m.emitShow();
    },
    [notice, expanded, scenario] as const,
  );
}

/** Compact pill fits the empty one-line Palette. */
const compactHeight = () => paletteHeight(0);

test("Compact shows an available update as a pill in the input line", async ({ page }) => {
  await page.setViewportSize({ width: 640, height: compactHeight() });
  await show(page, AVAILABLE);

  const pill = page.getByTestId("update-notice");
  await expect(pill).toContainText("Update available");
  await expect(pill).toContainText("v1.0.0");
  await expect(pill).toContainText("Install Update ↵");
  await expect(pill).toHaveAttribute("data-selected", "true");
  await expect(page).toHaveScreenshot("palette-compact-update-pill.png");
});

test("typing hides the pill, and clearing the line brings it back", async ({ page }) => {
  await page.setViewportSize({ width: 640, height: compactHeight() });
  await show(page, AVAILABLE);
  const input = page.getByPlaceholder("Search");

  await input.fill("photo");
  await expect(page.getByTestId("update-notice")).toHaveCount(0);
  await input.fill("");
  await expect(page.getByTestId("update-notice")).toBeVisible();
});

/** Enter installs straight away, and the row reports progress until the exit. */
test("Enter on the pill installs and shows progress", async ({ page }) => {
  await page.setViewportSize({ width: 640, height: compactHeight() });
  await show(page, AVAILABLE);

  await page.keyboard.press("Enter");
  await expect(page.getByTestId("update-notice")).toContainText(
    "Installing. Takyon restarts by itself.",
  );
});

test("an install that fails says why on the pill", async ({ page }) => {
  await page.setViewportSize({ width: 640, height: compactHeight() });
  await show(page, AVAILABLE, false, "badSignature");

  await page.keyboard.press("Enter");
  await expect(page.getByRole("alert")).toContainText("Nothing was installed");
});

test("after an update, See what's new opens the release notes once", async ({ page }) => {
  await page.setViewportSize({ width: 640, height: compactHeight() });
  await show(page, WHATS_NEW);

  await expect(page.getByTestId("update-notice")).toContainText("See what's new");
  await expect(page.getByText("Show Release Notes")).toBeVisible();
  await expect(page).toHaveScreenshot("palette-compact-whats-new-pill.png");

  await page.keyboard.press("Enter");
  const opened = await page.evaluate(() =>
    (window as unknown as { __takyon_mock: Mock }).__takyon_mock.openedUrls(),
  );
  expect(opened).toContain(WHATS_NEW.url);
  await expect(page.getByTestId("update-notice")).toHaveCount(0);

  // Dismissed in Rust too: the next show has no row.
  await page.evaluate(() =>
    (window as unknown as { __takyon_mock: Mock }).__takyon_mock.emitShow(),
  );
  await expect(page.getByTestId("update-notice")).toHaveCount(0);
});

test.describe("Expanded", () => {
  test.use({ viewport: { width: 640, height: 520 } });

  test("an available update is a row below the input, selected first", async ({ page }) => {
    await show(page, AVAILABLE, true);

    const row = page.getByTestId("update-notice");
    await expect(row).toContainText("Update available");
    await expect(page.getByRole("option").first()).toHaveAttribute("aria-selected", "true");
    await expect(page.getByText("Install Update")).toBeVisible();
    await expect(page).toHaveScreenshot("palette-expanded-update-row.png");

    // Down leaves the row for suggestions; Up from the first comes back.
    await page.keyboard.press("ArrowDown");
    await expect(page.getByRole("option").first()).not.toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("ArrowUp");
    await expect(page.getByRole("option").first()).toHaveAttribute("aria-selected", "true");
  });

  test("the what's-new row names the release notes", async ({ page }) => {
    await show(page, WHATS_NEW, true);
    await expect(page.getByTestId("update-notice")).toContainText("See what's new");
    await expect(page.getByText("Show Release Notes")).toBeVisible();
  });
});

test("no notice, no row: the empty Palette is unchanged", async ({ page }) => {
  await show(page, null);
  await expect(page.getByTestId("update-notice")).toHaveCount(0);
});
