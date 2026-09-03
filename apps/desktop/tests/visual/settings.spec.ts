import { expect, test } from "@playwright/test";

/**
 * The Settings window (v0.6 slice 1): shell, two-tier navigation, search, and a
 * refused autostart write that says so rather than vanishing (tbd v0.1 §3).
 *
 * The Palette's 640x68 viewport is the wrong shape for a sidebar, so this file
 * takes the size `settings.rs` actually builds.
 */
test.use({ viewport: { width: 880, height: 620 } });

/** Make the next autostart write refuse, or allow it again with `null`. */
const failAutostart = (page: import("@playwright/test").Page, message: string | null) =>
  page.evaluate((msg) => {
    (
      window as unknown as {
        __takyon_mock: { failAutostart: (m: string | null) => void };
      }
    ).__takyon_mock.failAutostart(msg);
  }, message);

test("the window opens on General with both tiers in the sidebar", async ({ page }) => {
  await page.goto("/?window=settings");

  // Tier one keeps its declared order; tier two sits below the divider. The
  // ordering itself is unit-tested — this asserts the sidebar renders it.
  await expect(page.getByRole("button", { name: "General" })).toHaveAttribute(
    "aria-current",
    "page",
  );
  await expect(page.getByRole("button", { name: "About" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Calculator" })).toBeVisible();

  await expect(page).toHaveScreenshot("settings-general.png");
});

test("a tier-two page is one click away and renders its own controls", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Calculator" }).click();

  await expect(page.getByRole("radiogroup", { name: "Answer arithmetic" })).toBeVisible();
  await expect(page.getByRole("radio", { name: "As I type" })).toHaveAttribute(
    "aria-checked",
    "true",
  );

  await expect(page).toHaveScreenshot("settings-calculator.png");
});

/**
 * About reads a build-time define for the version, so it fails at render rather
 * than at compile if `vite.config.ts` stops injecting one.
 */
test("About names the version and the identity slug", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "About" }).click();

  await expect(page.getByText(/^Version \d/)).toBeVisible();
  // ADR-0011: what Windows keys off is the slug, never the display name.
  await expect(page.getByText("com.v3sper.launcher")).toBeVisible();

  await expect(page).toHaveScreenshot("settings-about.png");
});

/**
 * Task 4: the box searches *settings*, not page names. "retention" is what
 * someone types and no page is called that.
 */
test("search finds a control by keyword and names the page it lives on", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByLabel("Search settings").fill("arithmetic");

  const hit = page.getByRole("button", { name: /Answer arithmetic/ });
  await expect(hit).toBeVisible();
  // Results replace the nav, and each names the page its control lives on.
  await expect(page).toHaveScreenshot("settings-search.png");
  await hit.click();

  // Picking a result navigates to the page that owns the control.
  await expect(page.getByRole("radiogroup", { name: "Answer arithmetic" })).toBeVisible();
});

test("a query matching nothing says so rather than emptying the sidebar", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByLabel("Search settings").fill("qwertyuiop");

  await expect(page.getByText("Nothing matches.")).toBeVisible();
});

/**
 * Apply-on-change: no save button anywhere, and the confirmation appears after
 * the write lands rather than when it was clicked.
 */
test("a switch applies on change and confirms", async ({ page }) => {
  await page.goto("/?window=settings");
  const motion = page.getByRole("switch", { name: "Turn off animations" });

  await expect(motion).toHaveAttribute("aria-checked", "false");
  await motion.click();

  await expect(motion).toHaveAttribute("aria-checked", "true");
  await expect(page.getByRole("status")).toHaveText("Applied");
  // The preference reaches the document, which is what the CSS acts on.
  await expect(page.locator("html")).toHaveAttribute("data-reduce-motion", "");

  // A lit switch and the confirmation beside its label, both at once.
  await expect(page).toHaveScreenshot("settings-applied.png");
});

/**
 * The same rule for an ordinary preference, which is where it was first wrong.
 *
 * `prefs.ts` updated its cache before awaiting the write, so a rejection left the
 * cache holding the optimistic value and the refetch handed back what had been
 * clicked. Storage writes first now; showing the new state early is the caller's.
 */
test("a refused preference write leaves the switch on what is stored", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.evaluate(() => {
    (
      window as unknown as {
        __takyon_mock: { failPreferenceWrite: (m: string | null) => void };
      }
    ).__takyon_mock.failPreferenceWrite("settings.db is read-only");
  });

  const motion = page.getByRole("switch", { name: "Turn off animations" });
  await motion.click();

  await expect(page.getByRole("alert")).toContainText("read-only");
  await expect(motion).toHaveAttribute("aria-checked", "false");
  // And the document is not left claiming a preference that was never stored.
  await expect(page.locator("html")).not.toHaveAttribute("data-reduce-motion", "");
});

/**
 * tbd v0.1 §3, the fix this slice owed.
 *
 * The old write was awaited with no `try`/`catch` from a `void toggle()`, so a
 * rejection skipped the re-read and vanished into the floating promise. Forcing
 * that on a real machine takes a group policy; the mock refuses on demand.
 */
test("a refused autostart write shows its error and leaves the switch where the OS has it", async ({
  page,
}) => {
  await page.goto("/?window=settings");
  await failAutostart(page, "Access is denied. (os error 5)");

  const startup = page.getByRole("switch", { name: "Start Takyon when I log in" });
  await expect(startup).toHaveAttribute("aria-checked", "false");

  await startup.click();

  // The message is beside the control, not in a toast that has already gone.
  await expect(page.getByRole("alert")).toContainText("Access is denied");
  // And the switch settles on what the OS says, not on what was clicked.
  await expect(startup).toHaveAttribute("aria-checked", "false");
  await expect(page.getByRole("status")).toHaveCount(0);

  await expect(page).toHaveScreenshot("settings-autostart-refused.png");
});

test("the same switch works once the write is allowed again", async ({ page }) => {
  await page.goto("/?window=settings");
  const startup = page.getByRole("switch", { name: "Start Takyon when I log in" });

  await failAutostart(page, "Access is denied. (os error 5)");
  await startup.click();
  await expect(page.getByRole("alert")).toBeVisible();

  await failAutostart(page, null);
  await startup.click();

  await expect(startup).toHaveAttribute("aria-checked", "true");
  // The stale error must clear, or it reads as describing the write that worked.
  await expect(page.getByRole("alert")).toHaveCount(0);
});
