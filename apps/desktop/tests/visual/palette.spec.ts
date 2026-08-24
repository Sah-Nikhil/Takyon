import { expect, test } from "@playwright/test";

/**
 * v0.1 has one screen worth a baseline: the Palette as it looks the instant it
 * opens. That is also the only state the phase promises — it always opens empty
 * (ROADMAP v0.1), so a diff here means either the shell changed or the "opens
 * empty" guarantee broke.
 */
test("the Palette opens empty", async ({ page }) => {
  await page.goto("/?window=palette");

  // No hotkey exists in a browser, so the show event is driven through the mock
  // seam. This is the same event Rust emits, carrying the same payload shape.
  await page.evaluate(() => {
    (window as unknown as { __takyon_mock: { emitShow: () => void } }).__takyon_mock.emitShow();
  });

  const input = page.getByPlaceholder("Search");
  await expect(input).toBeFocused();
  await expect(input).toHaveValue("");

  await expect(page).toHaveScreenshot("palette-empty.png");
});

/**
 * The Palette must come back empty every time. If state ever leaks between shows
 * — a remembered query, a remembered selection — it happens here first, and it
 * contradicts ADR-0001's "the Palette remembers nothing" directly.
 */
test("a second show clears whatever was typed", async ({ page }) => {
  await page.goto("/?window=palette");
  const mock = () =>
    page.evaluate(() => {
      (window as unknown as { __takyon_mock: { emitShow: () => void } }).__takyon_mock.emitShow();
    });

  await mock();
  await page.getByPlaceholder("Search").fill("photoshop");
  await expect(page.getByPlaceholder("Search")).toHaveValue("photoshop");

  await mock();
  await expect(page.getByPlaceholder("Search")).toHaveValue("");
});

/**
 * The idle pulse is state, not decoration: it claims the Palette is open and
 * waiting on the user. Assert the claim rather than the pixels — `toHaveScreenshot`
 * cancels infinite animations back to their initial frame, so a screenshot of a
 * breathing particle and a dead one are the same image.
 */
test("the mark moves only while the Palette is open and empty", async ({ page }) => {
  await page.goto("/?window=palette");
  const moving = page.locator("[data-particle-pulse], [data-cone-sweep]");
  const input = page.getByPlaceholder("Search");

  await page.evaluate(() => {
    (window as unknown as { __takyon_mock: { emitShow: () => void } }).__takyon_mock.emitShow();
  });
  await expect(moving).toHaveCount(2);

  // The first keystroke hands the work to the Palette. Motion past this point
  // would read as a spinner for a search that is not running.
  await input.fill("p");
  await expect(moving).toHaveCount(0);

  // Clearing the field is the idle state again, not a new one.
  await input.fill("");
  await expect(moving).toHaveCount(2);

  // Nothing animates against a hidden window.
  await page.evaluate(() => {
    (window as unknown as { __takyon_mock: { emitHide: () => void } }).__takyon_mock.emitHide();
  });
  await expect(moving).toHaveCount(0);
});

/**
 * The particle and the cone are two animations that have to look like one thing.
 * Equal duration and a shared start time is the whole trick, and it is exactly
 * the kind of detail a later edit breaks silently — a screenshot cannot see it.
 */
test("the two animations run on one beat", async ({ page }) => {
  await page.goto("/?window=palette");
  await page.evaluate(() => {
    (window as unknown as { __takyon_mock: { emitShow: () => void } }).__takyon_mock.emitShow();
  });

  const beat = await page.evaluate(() => {
    const read = (sel: string) => {
      const a = document.querySelector(sel)!.getAnimations()[0] as Animation;
      const timing = (a as unknown as { effect: AnimationEffect }).effect.getTiming();
      return { start: a.startTime, duration: timing.duration as number };
    };
    return { particle: read("[data-particle-pulse]"), cone: read("[data-cone-sweep]") };
  });

  // The particle alternates, so one pass is half a breath; the cone runs the
  // whole sweep in one pass. Two to one is what keeps them locked.
  expect(beat.cone.duration).toBe(beat.particle.duration * 2);
  expect(beat.particle.start).toBe(beat.cone.start);
});

/**
 * The mark's geometry is locked (docs/brand.md). Whatever the animation does in
 * the middle, its first frame — the frame every screenshot and every
 * reduced-motion user sees — must be the mark exactly as drawn.
 */
test("the mark rests in its locked geometry", async ({ page }) => {
  await page.goto("/?window=palette");
  await page.evaluate(() => {
    (window as unknown as { __takyon_mock: { emitShow: () => void } }).__takyon_mock.emitShow();
  });

  const resting = await page.evaluate(() => {
    const cone = document.querySelector("[data-cone-sweep]")!;
    // `noUncheckedIndexedAccess` is on: an element with no animations yields
    // undefined here, and that is a real case (reduced motion), not a type
    // technicality. Assert rather than `!`, so the failure names itself.
    const a = cone.getAnimations()[0];
    if (!a) throw new Error("the cone has no animation to rest");
    a.pause();
    a.currentTime = 0;
    return getComputedStyle(cone).transform;
  });
  expect(["none", "matrix(1, 0, 0, 1, 0, 0)"]).toContain(resting);
});

/**
 * The switch in Settings. Without it the animation is not a setting, it is a
 * decision made for the user — and the two windows have no live channel between
 * them in v0.1, so the Palette re-reads the preference on every show.
 */
test("the Settings switch stops the mark", async ({ page }) => {
  await page.goto("/?window=palette");
  const show = () =>
    page.evaluate(() => {
      (window as unknown as { __takyon_mock: { emitShow: () => void } }).__takyon_mock.emitShow();
    });
  const running = () =>
    page.evaluate(
      () =>
        document.querySelector("[data-particle-pulse]")!.getAnimations().length +
        document.querySelector("[data-cone-sweep]")!.getAnimations().length,
    );

  await show();
  expect(await running()).toBe(2);

  // Stand in for the other window writing the key, then summon again.
  await page.evaluate(() => {
    window.localStorage.setItem("com.v3sper.launcher.reduce-motion", "true");
  });
  await show();
  expect(await running()).toBe(0);
  await expect(page.locator("html")).toHaveAttribute("data-reduce-motion", "");

  // And back. A preference that cannot be undone is a bug, not a preference.
  await page.evaluate(() => {
    window.localStorage.setItem("com.v3sper.launcher.reduce-motion", "false");
  });
  await show();
  expect(await running()).toBe(2);
});

/**
 * The control itself, in the window that owns it.
 */
test("the Settings window drives the motion preference", async ({ page }) => {
  await page.goto("/?window=settings");
  const box = page.getByRole("checkbox", { name: "Turn off animations" });

  await expect(box).not.toBeChecked();
  await box.check();
  await expect(page.locator("html")).toHaveAttribute("data-reduce-motion", "");
  expect(
    await page.evaluate(() =>
      window.localStorage.getItem("com.v3sper.launcher.reduce-motion"),
    ),
  ).toBe("true");

  // It survives the window being closed and reopened, which is the only thing a
  // stored preference is for.
  await page.reload();
  await expect(page.getByRole("checkbox", { name: "Turn off animations" })).toBeChecked();
});
