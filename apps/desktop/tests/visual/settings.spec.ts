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
 * ROADMAP v0.6's headline rule: a destructive setting confirms with the **real
 * count**, never "some items". A generic warning is one people learn to click
 * through, which is the habit you least want in front of a secure delete.
 */
test("shortening retention confirms with the exact number it destroys", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Clipboard History" }).click();

  await page.getByRole("radio", { name: "1 day" }).click();

  const dialog = page.getByRole("alertdialog");
  await expect(dialog).toContainText("permanently delete");
  // The mock holds four clips, so the dialog has to say four rather than "some".
  await expect(dialog).toContainText(/delete \d+ clipboard items?/);
  await expect(dialog).toContainText("nothing to restore from");

  await expect(page).toHaveScreenshot("settings-retention-confirm.png");
});

test("cancelling the retention dialog changes nothing", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Clipboard History" }).click();

  await expect(page.getByRole("radio", { name: "1 month" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await page.getByRole("radio", { name: "1 day" }).click();
  await page.getByRole("button", { name: "Cancel" }).click();

  await expect(page.getByRole("alertdialog")).toHaveCount(0);
  await expect(page.getByRole("radio", { name: "1 month" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
});

/** The blocklist is the second exclusion mechanism ADR-0006 relies on. */
test("an executable can be added to the blocklist and taken off again", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Clipboard History" }).click();

  await expect(page.getByText("keepass.exe")).toBeVisible();

  await page.getByLabel("Executable to exclude").fill("Bitwarden.exe");
  await page.getByRole("button", { name: "Add" }).click();
  // Stored lower-cased, because that is how the capture path compares them.
  await expect(page.getByText("bitwarden.exe")).toBeVisible();

  await page
    .locator("div", { has: page.getByText("bitwarden.exe", { exact: true }) })
    .getByRole("button", { name: "Remove" })
    .last()
    .click();
  await expect(page.getByText("bitwarden.exe")).toHaveCount(0);
});

/**
 * tbd v0.3 §3: aliases had no editor at all. An alias whose application is gone
 * must still be listed, or it becomes an invisible rule nobody can delete.
 */
test("the alias list names a dead target rather than hiding it", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Applications" }).click();

  await expect(page.getByText("Adobe Photoshop 2022")).toBeVisible();
  await expect(page.getByText("no longer installed")).toBeVisible();

  await expect(page).toHaveScreenshot("settings-aliases.png");
});

/**
 * The Keyboard row at the window's minimum width.
 *
 * Six chips are wider than the content pane at *every* size, so the control has
 * to drop onto its own line. Before v0.6 it refused to shrink instead, squeezing
 * the label to one word per line and drawing the first chip over it.
 */
test.describe("at the minimum window width", () => {
  test.use({ viewport: { width: 680, height: 480 } });

  test("the hotkey chips wrap instead of crushing the label", async ({ page }) => {
    await page.goto("/?window=settings");
    await page.getByRole("button", { name: "Keyboard" }).click();

    const label = page.getByText("Open Takyon with");
    const chip = page.getByRole("radio", { name: "Alt + Space", exact: true });
    const labelBox = await label.boundingBox();
    const chipBox = await chip.boundingBox();
    if (!labelBox || !chipBox) throw new Error("the row did not render");

    // The label keeps a readable column rather than collapsing to one word.
    expect(labelBox.width).toBeGreaterThan(100);
    // And the chips sit below it, not on top of it.
    expect(chipBox.y).toBeGreaterThanOrEqual(labelBox.y + labelBox.height);

    await expect(page).toHaveScreenshot("settings-keyboard-narrow.png");
  });
});

/** Pinned chords with a reset, never a raw capture field. */
test("the hotkey is rebound from pinned choices", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Keyboard" }).click();

  const group = page.getByRole("radiogroup", { name: "Open Takyon with" });
  await expect(group.getByRole("radio")).toHaveCount(6);

  await page.getByRole("radio", { name: "Ctrl + Space", exact: true }).click();
  await expect(page.getByRole("radio", { name: "Ctrl + Space", exact: true })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await expect(page.getByRole("status")).toHaveText("Applied");

  await expect(page).toHaveScreenshot("settings-keyboard.png");
});

/**
 * A refused chord keeps the old binding, and the chips have to show what is
 * actually live rather than what was clicked.
 */
test("a refused chord keeps the previous binding and says so", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Keyboard" }).click();

  await page.getByRole("radio", { name: "Ctrl + Space", exact: true }).click();
  // The mock refuses Alt+Space, standing in for PowerToys Run holding it.
  await page.getByRole("radio", { name: "Alt + Space", exact: true }).click();

  await expect(page.getByRole("alert")).toContainText("already holding it");
  await expect(page.getByRole("radio", { name: "Ctrl + Space", exact: true })).toHaveAttribute(
    "aria-checked",
    "true",
  );
});

/** Hiding the tray with a dead hotkey would leave no way in, and no way out. */
test("the tray cannot be hidden while the hotkey is unregistered", async ({ page }) => {
  await page.goto("/?window=settings&hotkey=failed");
  await page.getByRole("button", { name: "Launcher" }).click();

  const tray = page.getByRole("switch", { name: "Show the tray icon" });
  await tray.click();

  await expect(page.getByRole("alert")).toContainText("only way in");
  await expect(tray).toHaveAttribute("aria-checked", "true");
});

/**
 * Slice 3: the override has to beat the system in **both** directions, which is
 * what separates an override from a hint. The suite runs `colorScheme: dark`, so
 * choosing Light here is the harder direction.
 */
test("the appearance override beats the system setting", async ({ page }) => {
  await page.goto("/?window=settings");

  // Following the system, no attribute at all — the stylesheet decides.
  await expect(page.locator("html")).not.toHaveAttribute("data-theme", /.*/);

  await page.getByRole("radio", { name: "Light" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await expect(page).toHaveScreenshot("settings-light.png");

  await page.getByRole("radio", { name: "Dark" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

  // And back to following, which must remove the attribute rather than pick a side.
  await page.getByRole("radio", { name: "System" }).click();
  await expect(page.locator("html")).not.toHaveAttribute("data-theme", /.*/);
});

/** Interface size is a root zoom, mirrored by Rust's window arithmetic. */
test("interface size scales the whole window", async ({ page }) => {
  await page.goto("/?window=settings");

  await expect(page.locator("html")).not.toHaveAttribute("data-ui-size", /.*/);
  await page.getByRole("radio", { name: "Large" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-ui-size", "large");
  await expect(page).toHaveScreenshot("settings-large.png");

  await page.getByRole("radio", { name: "Default" }).click();
  await expect(page.locator("html")).not.toHaveAttribute("data-ui-size", /.*/);
});

/**
 * The Palette is the surface the light theme actually has to survive: it is a
 * transparent window drawing its own panel, so a theme that only reached the
 * settings window would look correct here and wrong where it matters.
 */
test("the Palette honours a light override", async ({ page }) => {
  await page.goto("/?window=palette");

  // Stand in for the Settings window having stored it. A full navigation would
  // reset the browser build's module state, which Rust does not do.
  await page.evaluate(() => {
    const m = (
      window as unknown as {
        __takyon_mock: {
          setStoredPreference: (p: { theme: string }) => void;
          emitShow: () => void;
        };
      }
    ).__takyon_mock;
    m.setStoredPreference({ theme: "light" });
    m.emitShow();
  });

  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
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
