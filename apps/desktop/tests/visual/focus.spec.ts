import { expect, test, type Page } from "./fixtures";
import { paletteHeight, VIEW_HEIGHT } from "@takyon/shared";

/**
 * v0.16's visual layer: the Focus view, the floating bar, Settings → Focus and
 * Focus Stats. The session is the mock's (`api.mock.ts` § Focus); blocking,
 * clock and store are Rust's, answered in `tests/focus.rs`.
 */

type Mock = { emitShow: () => void };

async function open(page: Page, query = "") {
  await page.goto(`/?window=palette${query}`);
  await page.evaluate(() => {
    (window as unknown as { __takyon_mock: Mock }).__takyon_mock.emitShow();
  });
  return page.getByPlaceholder("Search");
}

async function openForm(page: Page, query = "") {
  const input = await open(page, query);
  await input.fill("focus");
  await page.getByRole("option", { name: /Start Focus Session/ }).click();
  await page.setViewportSize({ width: 640, height: VIEW_HEIGHT });
  await expect(page.getByRole("textbox", { name: "Goal" })).toBeVisible();
}

test("typing focus finds the four Focus commands", async ({ page }) => {
  const input = await open(page);
  await input.fill("focus");
  await expect(page.getByRole("option")).toHaveCount(4);
  await expect(page.getByRole("option").first()).toContainText("Start Focus Session");
  await page.setViewportSize({ width: 640, height: paletteHeight(4) });
  await expect(page).toHaveScreenshot("palette-focus-commands.png");
});

test("Start Focus Session opens the form with the last Block list", async ({ page }) => {
  await openForm(page);
  await expect(page.getByRole("textbox", { name: "Duration" })).toHaveValue("45m");
  await expect(page.getByText("Social", { exact: true })).toBeVisible();
  await expect(page.getByText("Video & streaming", { exact: true })).toBeVisible();
  await expect(page.getByText("Discord", { exact: true })).toBeVisible();
  await expect(page).toHaveScreenshot("focus-form.png");
});

test("a typed website is offered and added to the Block list", async ({ page }) => {
  await openForm(page);
  const picker = page.getByRole("textbox", { name: "Add apps, websites or Focus categories" });
  await picker.fill("nytimes.com");
  await expect(page.getByRole("option", { name: "Block website: nytimes.com" })).toBeVisible();
  await expect(page).toHaveScreenshot("focus-form-site.png");
  await picker.press("Enter");
  await expect(page.getByText("nytimes.com", { exact: true })).toBeVisible();
});

test("the form names what is running out of Focus's reach before starting", async ({ page }) => {
  await openForm(page, "&refusals=1");
  const refused = page.getByRole("status", { name: "Can't block" });
  await expect(refused).toContainText("Can't block Discord while it runs as administrator.");
  await expect(refused).toContainText("Can't block sites in Opera GX: Focus can't read its address bar.");
  await expect(page).toHaveScreenshot("focus-form-refusals.png");
});

test("a duration that is not one says why", async ({ page }) => {
  await openForm(page);
  await page.getByRole("textbox", { name: "Duration" }).fill("soon");
  await expect(page.getByText("Not a length or a time")).toBeVisible();
  await expect(page.getByRole("button", { name: /Start Focus Session/ })).toBeDisabled();
});

test("starting shows the running session with its controls", async ({ page }) => {
  await openForm(page);
  await page.getByRole("textbox", { name: "Goal" }).fill("Write Focus manual page");
  await page.keyboard.press("Control+Enter");
  await expect(page.getByRole("heading", { name: "Write Focus manual page" })).toBeVisible();
  await expect(page.getByText("45:00")).toBeVisible();
  await expect(page.getByRole("button", { name: "Complete" })).toBeVisible();
  await expect(page).toHaveScreenshot("focus-running.png");
});

test("a Strict session asks for the goal before ending early", async ({ page }) => {
  const input = await open(page, "&focus=strict");
  await input.fill("complete focus");
  await page.getByRole("option", { name: /Complete Focus Session/ }).click();
  await page.setViewportSize({ width: 640, height: VIEW_HEIGHT });
  const retype = page.getByRole("textbox", { name: "Retype your goal" });
  await expect(retype).toBeVisible();
  await expect(page.getByRole("button", { name: "End session" })).toBeDisabled();
  await retype.fill("Write Focus manual page");
  await expect(page.getByText("0:30 to wait")).toBeVisible();
  await expect(page).toHaveScreenshot("focus-friction.png");
});

test("!f is plain text until Settings switches it on", async ({ page }) => {
  const input = await open(page);
  await input.fill("!f write report 45m");
  await expect(page.getByTestId("focus-note")).toHaveCount(0);
});

test("!f with a goal and a length starts the session on Enter", async ({ page }) => {
  const input = await open(page, "&focusbang=1");
  await input.fill("!f write report 45m");
  await expect(page.getByTestId("focus-note")).toHaveText("Focus on write report for 45 min — press Enter");
  await page.setViewportSize({ width: 640, height: paletteHeight(1) });
  await expect(page).toHaveScreenshot("palette-focus-bang.png");
  await input.press("Enter");
  await page.setViewportSize({ width: 640, height: VIEW_HEIGHT });
  await expect(page.getByRole("heading", { name: "write report" })).toBeVisible();
});

test("!f without a length opens the form with the goal typed", async ({ page }) => {
  const input = await open(page, "&focusbang=1");
  await input.fill("!f write report");
  await expect(page.getByTestId("focus-note")).toContainText("press Enter to pick a length");
  await input.press("Enter");
  await expect(page.getByRole("textbox", { name: "Goal" })).toHaveValue("write report");
});

test.describe("the floating bar", () => {
  test.use({ viewport: { width: 360, height: 52 } });

  test("shows the goal and time left", async ({ page }) => {
    await page.goto("/?window=bar&focus=running");
    await expect(page.getByText("Write Focus manual page")).toBeVisible();
    await expect(page.getByText("23:41")).toBeVisible();
    await expect(page).toHaveScreenshot("focus-bar.png");
  });

  test("says Paused while on a break", async ({ page }) => {
    await page.goto("/?window=bar&focus=paused");
    await expect(page.getByText("Paused")).toBeVisible();
    await expect(page).toHaveScreenshot("focus-bar-paused.png");
  });

  test("draws in the light half too", async ({ page }) => {
    await page.emulateMedia({ colorScheme: "light" });
    await page.goto("/?window=bar&focus=running");
    await expect(page.getByText("23:41")).toBeVisible();
    await expect(page).toHaveScreenshot("focus-bar-light.png");
  });
});

test.describe("Settings", () => {
  test.use({ viewport: { width: 880, height: 900 } });

  test("Focus lists preferences, Focus categories and the Coming soon rows", async ({ page }) => {
    await page.goto("/?window=settings");
    await page.getByRole("button", { name: "Focus", exact: true }).click();
    await expect(page.getByRole("combobox", { name: "Snooze duration" })).toBeVisible();
    await expect(page.getByText("Video & streaming")).toBeVisible();
    await expect(page.getByText("Scheduled sessions")).toBeVisible();
    await expect(page.getByText("Coming soon", { exact: true }).first()).toBeVisible();
    await expect(page).toHaveScreenshot("settings-focus.png", { fullPage: true });
  });

  test("a built-in Focus category duplicates into an editable one", async ({ page }) => {
    await page.goto("/?window=settings");
    await page.getByRole("button", { name: "Focus", exact: true }).click();
    await page.getByRole("button", { name: "Duplicate" }).first().click();
    const name = page.getByRole("textbox", { name: "Focus category name" });
    await expect(name).toHaveValue("Social (copy)");
    await page.getByRole("button", { name: "Save Focus category" }).click();
    await expect(page.getByText("Social (copy)")).toBeVisible();
    await expect(page.getByRole("button", { name: "Edit" })).toBeVisible();
  });

  test("Focus stats opens as Focus / Stats and draws the line, total and breakdown", async ({ page }) => {
    await page.goto("/?window=settings");
    await page.getByRole("button", { name: "Focus", exact: true }).click();
    await page.getByRole("button", { name: /Focus stats/ }).click();
    await expect(page.getByRole("heading", { name: "Focus Stats", level: 1 })).toBeVisible();
    await expect(page.getByRole("img", { name: "Focused time per day" })).toBeVisible();
    await expect(page.getByRole("cell", { name: "YouTube" })).toBeVisible();
    await expect(page).toHaveScreenshot("settings-focus-stats.png", { fullPage: true });
    await page.getByRole("radio", { name: "By day" }).click();
    await page.getByRole("radio", { name: "7 days" }).click();
    await expect(page).toHaveScreenshot("settings-focus-stats-7.png", { fullPage: true });
    // Back through the crumb, then once more through the sidebar.
    await page.locator("main h1").getByRole("button", { name: "Focus" }).click();
    await expect(page.getByRole("combobox", { name: "Snooze duration" })).toBeVisible();
    await page.getByRole("button", { name: /Focus stats/ }).click();
    await page.locator("nav").getByRole("button", { name: "Focus", exact: true }).click();
    await expect(page.getByRole("combobox", { name: "Snooze duration" })).toBeVisible();
  });
});
