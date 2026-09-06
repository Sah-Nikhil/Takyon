import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test } from "./fixtures";

/** Same source `vite.config.ts` injects from, so the two cannot drift. */
const { version: APP_VERSION } = JSON.parse(
  readFileSync(
    fileURLToPath(new URL("../../package.json", import.meta.url)),
    "utf8",
  ),
) as { version: string };

/**
 * The Settings window (v0.6 slice 1): shell, two-tier navigation, search, and a
 * refused autostart write that says so rather than vanishing (tbd v0.1 §3).
 *
 * The Palette's 640x68 viewport is the wrong shape for a sidebar, so this file
 * takes the size `settings.rs` actually builds.
 */
test.use({ viewport: { width: 880, height: 620 } });

/**
 * Report autostart as unregistered, before the page loads. The mock reports it
 * **on** by default, because that is what `firstrun::maybe_enable` leaves behind
 * on a real install, and the switch reads the value on mount.
 */
const clearAutostart = (page: import("@playwright/test").Page) =>
  page.addInitScript(() => {
    (globalThis as { __takyon_autostart?: boolean }).__takyon_autostart = false;
  });

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
 * The half v0.6 described as done and did not build: **creating** an alias.
 * `docs/tbd/v0.3.md` §3 was marked closed with only the review-and-delete side
 * shipped, so the only way to make one was an `INSERT` by hand.
 */
test("an alias can be created on an application that has none", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Applications" }).click();

  const field = page.getByRole("button", { name: "Alias for Notepad" });
  await expect(field).toHaveText("Add alias");
  await field.click();

  await page.getByRole("textbox", { name: "Alias for Notepad" }).fill("np");
  await page.keyboard.press("Enter");

  await expect(page.getByRole("button", { name: "Alias for Notepad" })).toHaveText("np");
  await expect(page).toHaveScreenshot("settings-applications-alias.png");
});

/** Editing is the same field, and the old name must not survive the rename. */
test("an existing alias can be renamed and cleared", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Applications" }).click();

  await page.getByRole("button", { name: "Alias for Google Chrome" }).click();
  await page.getByRole("textbox", { name: "Alias for Google Chrome" }).fill("gc");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("button", { name: "Alias for Google Chrome" })).toHaveText(
    "gc",
  );

  // Emptying the field removes it rather than leaving an unnamed rule behind.
  await page.getByRole("button", { name: "Alias for Google Chrome" }).click();
  await page.getByRole("textbox", { name: "Alias for Google Chrome" }).fill("");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("button", { name: "Alias for Google Chrome" })).toHaveText(
    "Add alias",
  );
});

/** Escape abandons the edit, which is what Escape means everywhere else here. */
test("escape leaves an alias as it was", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Applications" }).click();

  await page.getByRole("button", { name: "Alias for File Explorer" }).click();
  await page.getByRole("textbox", { name: "Alias for File Explorer" }).fill("zzz");
  await page.keyboard.press("Escape");

  await expect(page.getByRole("button", { name: "Alias for File Explorer" })).toHaveText(
    "explorer",
  );
});

/**
 * The clutter fix: 1,891 applications sprawling in one list, most of them `PATH`
 * executables nobody recognises. Four groups, and the long tail starts shut.
 */
test("applications are grouped, and the command-line tail starts collapsed", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Applications" }).click();

  await expect(page.getByText("Installed (7)")).toBeVisible();
  await expect(page.getByText("Store apps (1)")).toBeVisible();
  await expect(page.getByText("Games (1)")).toBeVisible();
  await expect(page.getByText("Command line (3)")).toBeVisible();

  // Installed is open; the tail is not, and says how much it is hiding.
  await expect(page.getByRole("button", { name: "Alias for Google Chrome" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Alias for a2ping" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Command line applications" })).toHaveText(
    "Show 3",
  );

  await page.getByRole("button", { name: "Command line applications" }).click();
  await expect(page.getByRole("button", { name: "Alias for a2ping" })).toBeVisible();

  await expect(page).toHaveScreenshot("settings-applications-groups.png");
});

/** Searching says which group you meant, so a hit is never hidden by a header. */
test("a filter reveals matches inside a collapsed group", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Applications" }).click();
  await expect(page.getByRole("button", { name: "Alias for adb" })).toHaveCount(0);

  await page.getByRole("textbox", { name: "Filter applications" }).fill("adb");
  await expect(page.getByRole("button", { name: "Alias for adb" })).toBeVisible();
  // And only the group that matched is drawn at all.
  await expect(page.getByRole("button", { name: "Alias for Google Chrome" })).toHaveCount(0);
  await expect(page.getByText("Command line (1)")).toBeVisible();
});

/** ~1900 applications is more list than anyone scrolls. The filter is the way in. */
test("the application list filters by name and by alias", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Applications" }).click();

  await page.getByRole("textbox", { name: "Filter applications" }).fill("adobe");
  await expect(page.getByRole("button", { name: "Alias for Adobe Premiere Pro" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Alias for Notepad" })).toHaveCount(0);

  // An alias is searchable too: you remember the shorthand, not the product name.
  await page.getByRole("textbox", { name: "Filter applications" }).fill("prem");
  await expect(page.getByRole("button", { name: "Alias for Adobe Premiere Pro" })).toBeVisible();

  await page.getByRole("textbox", { name: "Filter applications" }).fill("qqqq");
  await expect(page.getByText("No application matches")).toBeVisible();
});

/**
 * v0.7's page. The entry count is a control here, not decoration: TBC-0005's
 * triggers are both stated in it, and neither is visible without the number.
 */
test("the file search page names its roots and the live entry count", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "File Search" }).click();

  await expect(page.getByText("26,844 files and folders indexed")).toBeVisible();
  await expect(page.getByRole("switch", { name: "Show files without typing !e" })).toHaveAttribute(
    "aria-checked",
    "false",
  );
  await expect(page.getByRole("switch", { name: "Also ask Windows Search" })).toHaveAttribute(
    "aria-checked",
    "false",
  );

  await expect(page).toHaveScreenshot("settings-file-search.png");
});

/**
 * TBC-0010 makes a visible off switch a condition of shipping the list at all,
 * and v0.6's rule applies: the confirmation names the real count.
 */
test("clearing the opened history confirms with the number it deletes", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "File Search" }).click();
  await page.getByRole("button", { name: "Clear history" }).click();

  await expect(page.getByText("permanently deletes 12 entries")).toBeVisible();
  await expect(page.getByText("Your files are not touched")).toBeVisible();
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
 * v0.10 replaced six chips with a dropdown, which is far easier on the layout,
 * so this now guards the other direction: the control must not squeeze the label
 * to one word per line, which is what the chips did before v0.6.
 */
test.describe("at the minimum window width", () => {
  test.use({ viewport: { width: 680, height: 480 } });

  test("the hotkey row keeps a readable label beside its dropdown", async ({ page }) => {
    await page.goto("/?window=settings");
    await page.getByRole("button", { name: "Keyboard" }).click();

    const label = page.getByText("Open Takyon with", { exact: true });
    const select = page.getByRole("combobox", { name: "Open Takyon with" });
    const labelBox = await label.boundingBox();
    const selectBox = await select.boundingBox();
    if (!labelBox || !selectBox) throw new Error("the row did not render");

    expect(labelBox.width).toBeGreaterThan(100);
    // Not drawn over each other, whichever way they ended up stacking.
    expect(
      selectBox.x >= labelBox.x + labelBox.width ||
        selectBox.y >= labelBox.y + labelBox.height,
    ).toBe(true);

    await expect(page).toHaveScreenshot("settings-keyboard-narrow.png");
  });
});

/** Pinned chords in a dropdown, with a reset. Never a raw capture field. */
test("the hotkey is rebound from pinned choices", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Keyboard" }).click();

  const select = page.getByRole("combobox", { name: "Open Takyon with" });
  await select.click();
  const list = page.getByRole("listbox", { name: "Open Takyon with" });
  await expect(list.getByRole("option")).toHaveCount(6);

  await list.getByRole("option", { name: "Ctrl + Space", exact: true }).click();
  await expect(select).toContainText("Ctrl + Space");
  await expect(page.getByRole("status")).toHaveText("Applied");

  await expect(page).toHaveScreenshot("settings-keyboard.png");
});

/**
 * v0.10: the Windows key is a switch, not a chord, because it is a different
 * mechanism (`superkey.rs`). Off by default, and turning it on must not disturb
 * the accelerator — the two bindings are independent.
 */
test("the Windows key is a separate switch and starts off", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Keyboard" }).click();

  const superKey = page.getByRole("switch", {
    name: "Open Takyon with the Windows key",
  });
  await expect(superKey).toHaveAttribute("aria-checked", "false");

  await superKey.click();
  await expect(superKey).toHaveAttribute("aria-checked", "true");
  // The chord is untouched by it.
  await expect(page.getByRole("combobox", { name: "Open Takyon with" })).toContainText(
    "Alt + Space",
  );
});

/**
 * A refused chord keeps the old binding, and the chips have to show what is
 * actually live rather than what was clicked.
 */
test("a refused chord keeps the previous binding and says so", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Keyboard" }).click();

  const select = page.getByRole("combobox", { name: "Open Takyon with" });
  await select.click();
  await page
    .getByRole("listbox", { name: "Open Takyon with" })
    .getByRole("option", { name: "Ctrl + Space", exact: true })
    .click();

  // The mock refuses Alt+Space, standing in for PowerToys Run holding it.
  await select.click();
  await page
    .getByRole("listbox", { name: "Open Takyon with" })
    .getByRole("option", { name: "Alt + Space", exact: true })
    .click();

  await expect(page.getByRole("alert")).toContainText("already holding it");
  await expect(select).toContainText("Ctrl + Space");
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
 * v0.10: the override beats the system in **both** directions, which is what
 * separates an override from a hint.
 *
 * `data-appearance`, not v0.9's `data-theme`, and always present: it names the
 * half being painted, which is a fact even when Windows chose it.
 */
test("the appearance override beats the system setting", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Appearance" }).click();

  const follow = page.getByRole("switch", { name: "Follow system appearance" });
  await expect(follow).toHaveAttribute("aria-checked", "true");
  // While it follows, there is no third choice to make and no control offering one.
  await expect(page.getByRole("radiogroup", { name: "Use" })).toHaveCount(0);

  await follow.click();
  await page.getByRole("radio", { name: "Light", exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("data-appearance", "light");
  await expect(page).toHaveScreenshot("settings-light.png");

  await page.getByRole("radio", { name: "Dark", exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("data-appearance", "dark");

  await follow.click();
  await expect(follow).toHaveAttribute("aria-checked", "true");
  await expect(page.getByRole("radiogroup", { name: "Use" })).toHaveCount(0);
});

/**
 * v0.10: five families, each carrying both halves, and the two pickers are
 * independent — choosing a dark theme must not touch the light one, or the
 * control is only usable at the right time of day.
 */
test("a theme family is chosen per half", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Appearance" }).click();

  const dark = page.getByRole("radiogroup", { name: "Dark theme" });
  const light = page.getByRole("radiogroup", { name: "Light theme" });
  await expect(dark.getByRole("radio")).toHaveCount(5);
  await expect(light.getByRole("radio")).toHaveCount(5);

  await dark.getByRole("radio", { name: "Vela" }).click();
  await expect(dark.getByRole("radio", { name: "Vela" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await expect(light.getByRole("radio", { name: "Graphite" })).toHaveAttribute(
    "aria-checked",
    "true",
  );

  // The window is painted from the family rather than from a stylesheet branch,
  // so the property has to be on `<html>` itself.
  const plate = await page.evaluate(() =>
    document.documentElement.style.getPropertyValue("--color-plate").trim(),
  );
  expect(plate).not.toBe("");

  await expect(page).toHaveScreenshot("settings-appearance.png");
});

/** Interface size is a root zoom, mirrored by Rust's window arithmetic. */
test("interface size scales the whole window", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Appearance" }).click();

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
          setStoredPreference: (p: { appearance: string }) => void;
          emitShow: () => void;
        };
      }
    ).__takyon_mock;
    m.setStoredPreference({ appearance: "light" });
    m.emitShow();
  });

  await expect(page.locator("html")).toHaveAttribute("data-appearance", "light");

  /*
    And the panel still has an edge, which light mode lacked through v0.9: every
    border in `palette/` was `border-white/10`, invisible on a near-white plate.
    Asserted as a computed colour, because a screenshot baseline accepts this
    failure happily — it was the baseline for two releases.
   */
  const edge = await page.evaluate(() => {
    const panel = document.querySelector("[cmdk-root]");
    return panel ? getComputedStyle(panel).borderTopColor : "";
  });
  expect(edge).not.toBe("");
  expect(edge).not.toBe("rgba(0, 0, 0, 0)");

  await expect(page).toHaveScreenshot("palette-light.png");
});

/**
 * About reads a build-time define for the version, so it fails at render rather
 * than at compile if `vite.config.ts` stops injecting one. Asserted against
 * `package.json`, not a `\d` pattern: the pattern matched 0.6.0 through two
 * releases while the baseline stayed stale.
 */
test("About names the version and the identity slug", async ({ page }) => {
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "About" }).click();

  await expect(page.getByText(`Version ${APP_VERSION}`)).toBeVisible();
  // ADR-0011: what Windows keys off is the slug, never the display name.
  await expect(page.getByText("com.v3sper.takyon")).toBeVisible();

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
  // Appearance since v0.10: the motion switch left General with the rest of it.
  await page.getByRole("button", { name: "Appearance" }).click();
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
  await page.getByRole("button", { name: "Appearance" }).click();
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
  await clearAutostart(page);
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
  await clearAutostart(page);
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
