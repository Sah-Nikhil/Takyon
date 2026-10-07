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
  const chips = page.getByRole("group", { name: "Block list" });
  await expect(chips.getByText("Social", { exact: true })).toBeVisible();
  await expect(chips.getByText("Video & streaming", { exact: true })).toBeVisible();
  await expect(page.getByText("Discord", { exact: true })).toBeVisible();
  const holds = page.getByRole("definition");
  await expect(holds.first()).toHaveText("X, Instagram, Reddit and TikTok");
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
  test.use({ viewport: { width: 300, height: 116 } });

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

  test("hover slides Pause and Complete out from behind the pill", async ({ page }) => {
    // A bar low on the screen: the buttons go up, toward the middle.
    await page.addInitScript(() => Object.defineProperty(window, "screenY", { get: () => 2000 }));
    await page.goto("/?window=bar&focus=running");
    const pause = page.getByRole("button", { name: "Pause" });
    await expect(pause).toHaveCSS("opacity", "0");
    await page.getByText("Write Focus manual page").hover();
    await expect(pause).toHaveCSS("opacity", "1");
    await expect(page.getByRole("button", { name: "Complete" })).toHaveCSS("transform", "none");
    await expect(page.getByRole("button", { name: "Pause" })).toBeVisible();
    const pill = (await page.getByText("23:41").boundingBox())!;
    const box = (await pause.boundingBox())!;
    expect(box.y + box.height).toBeLessThan(pill.y);
    await expect(page).toHaveScreenshot("focus-bar-hover.png");
  });

  test("near the top of the screen the buttons slide down instead", async ({ page }) => {
    await page.goto("/?window=bar&focus=running");
    await page.getByText("Write Focus manual page").hover();
    const pill = (await page.getByText("23:41").boundingBox())!;
    const pause = (await page.getByRole("button", { name: "Pause" }).boundingBox())!;
    expect(pause.y).toBeGreaterThan(pill.y + pill.height);
  });

  test("a goal too long to fit scrolls on hover and truncates at rest", async ({ page }) => {
    const goal = "Draft the Tesseract ideation brief and the three launch experiments";
    // The real window grows to the pill, up to 440; the test viewport has to.
    await page.setViewportSize({ width: 440, height: 116 });
    await page.goto(`/?window=bar&focus=running&goal=${encodeURIComponent(goal)}`);
    await expect(page.getByText(goal)).toHaveCSS("text-overflow", "ellipsis");
    await page.getByText(goal).hover();
    const moving = page.locator("[style*='--marquee-shift']");
    await expect(moving).toHaveCSS("animation-name", "focus-marquee");
    await expect(page).toHaveScreenshot("focus-bar-long.png", { animations: "disabled" });
    // Leaving the window, as the real one's region makes any move off the pill.
    await page.getByTestId("focus-bar").dispatchEvent("mouseout", { relatedTarget: null });
    await expect(page.getByText(goal)).toHaveCSS("text-overflow", "ellipsis");
    await expect(page).toHaveScreenshot("focus-bar-long-rest.png");
  });

  test("hugs its content: no gap between the goal and the clock", async ({ page }) => {
    await page.goto("/?window=bar&focus=running");
    const goal = (await page.getByText("Write Focus manual page").boundingBox())!;
    const time = (await page.getByText("23:41").boundingBox())!;
    expect(time.x - (goal.x + goal.width)).toBeLessThanOrEqual(24);
    const pill = (await page.locator("[data-tauri-drag-region]").first().boundingBox())!;
    expect(pill.width).toBeLessThan(270);
  });

  test("fades only its panel to the opacity Settings holds", async ({ page }) => {
    await page.goto("/?window=bar&focus=running&baropacity=50");
    await expect(page.getByText("23:41")).toBeVisible();
    const panel = page.locator("[aria-hidden]").first();
    await expect(panel).toHaveCSS("opacity", "0.5");
    await expect(page.getByText("Write Focus manual page")).toHaveCSS("opacity", "1");
    await expect(page).toHaveScreenshot("focus-bar-faded.png");
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
    await expect(page.getByRole("combobox", { name: "Floating bar position" })).toContainText("Bottom");
    await expect(page.getByRole("slider", { name: "Floating bar opacity" })).toHaveValue("90");
    await expect(page.getByText("Video & streaming")).toBeVisible();
    await expect(page.getByText("Scheduled sessions")).toBeVisible();
    await expect(page.getByText("Coming soon", { exact: true }).first()).toBeVisible();
    await expect(page).toHaveScreenshot("settings-focus.png", { fullPage: true });
  });

  async function openCategory(page: Page, name: string) {
    await page.goto("/?window=settings");
    await page.getByRole("button", { name: "Focus", exact: true }).click();
    await page.getByRole("button", { name: new RegExp(`^${name}`) }).click();
    await expect(page.locator("main h1")).toContainText(name);
  }

  test("a Focus category's page lists its apps, installed or not, and its websites", async ({ page }) => {
    await openCategory(page, "Messaging");
    const apps = page.locator("section", { has: page.getByRole("heading", { name: "Apps" }) });
    await expect(apps.getByText("Discord", { exact: true })).toBeVisible();
    await expect(apps.getByText("Installed", { exact: true })).toHaveCount(1);
    await expect(apps.getByText("Not installed")).toHaveCount(2);
    await expect(page).toHaveScreenshot("settings-focus-category.png", { fullPage: true });
  });

  test("a built-in takes additions and removals, and Reset undoes both", async ({ page }) => {
    await openCategory(page, "Messaging");
    const adder = page.getByRole("textbox", { name: "Add apps or websites" });
    await adder.fill("teams.microsoft.com");
    await adder.press("Enter");
    const sites = page.locator("section", { has: page.getByRole("heading", { name: "Websites" }) });
    await expect(sites.getByText("teams.microsoft.com").first()).toBeVisible();
    await expect(sites.getByText("Added")).toBeVisible();
    await page.getByRole("button", { name: "Remove Slack" }).click();
    const out = page.locator("section", { has: page.getByRole("heading", { name: "Taken out" }) });
    await expect(out.getByText("Slack")).toBeVisible();
    await expect(page).toHaveScreenshot("settings-focus-category-edited.png", { fullPage: true });
    await out.getByRole("button", { name: "Put back" }).click();
    await expect(page.getByRole("button", { name: "Remove Slack" })).toBeAttached();
    await page.getByRole("button", { name: "Reset to default" }).click();
    await page.getByRole("alertdialog").getByRole("button", { name: "Reset to default" }).click();
    await expect(page.getByText("teams.microsoft.com")).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Reset to default" })).toHaveCount(0);
  });

  test("Games lists the games launchers found, and one can be taken out", async ({ page }) => {
    await openCategory(page, "Games");
    const games = page.locator("section", { has: page.getByRole("heading", { name: "Installed games" }) });
    await expect(games.getByText("Counter-Strike 2")).toBeVisible();
    await page.getByRole("button", { name: "Remove Dota 2" }).click();
    await expect(games.getByText("Dota 2")).toHaveCount(0);
    await expect(page.getByText("Taken out")).toBeVisible();
  });

  test("a built-in duplicates into a custom one with a name to edit", async ({ page }) => {
    await openCategory(page, "Social");
    await page.getByRole("button", { name: "Duplicate" }).click();
    const name = page.getByRole("textbox", { name: "Focus category name" });
    await expect(name).toHaveValue("Social (copy)");
    await name.fill("Doomscrolling");
    await name.press("Enter");
    await expect(page.locator("main h1")).toContainText("Doomscrolling");
    await page.locator("main h1").getByRole("button", { name: "Focus" }).click();
    await expect(page.getByRole("button", { name: /^Doomscrolling/ })).toBeVisible();
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
