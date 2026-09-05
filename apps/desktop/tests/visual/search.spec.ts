import { expect, test, type Page } from "./fixtures";
import { paletteHeight, VIEW_HEIGHT } from "@takyon/shared";

/**
 * v0.9's visual layer: the `!s` row, the answer surface, and the key page.
 *
 * TBC-0007's exposure is total — **provider, pages and summariser are fixtures
 * in `api.mock.ts`.** The real response parses in `search/brave.rs`; the real
 * network is `tests/web_search.rs`. This catches whether the surfaces draw.
 */

type Mock = {
  emitShow: () => void;
  setWebKeyStored: (key: string | null) => void;
  failWebSearch: (message: string | null) => void;
  holdSearchAtReading: (on: boolean) => void;
  setIndexing: (on: boolean) => void;
  openedUrls: () => string[];
};

async function open(page: Page, key: string | null = "BSA-test-key-9876") {
  await page.goto("/?window=palette");
  await page.evaluate((stored) => {
    const m = (window as unknown as { __takyon_mock: Mock }).__takyon_mock;
    m.setWebKeyStored(stored);
    m.emitShow();
  }, key);
  return page.getByPlaceholder("Search");
}

const fitTo = (page: Page, rows: number) =>
  page.setViewportSize({ width: 640, height: paletteHeight(rows, true) });

test("!s alone names the provider and says the query will leave", async ({ page }) => {
  const input = await open(page);
  await input.fill("!s");

  await expect(page.getByTestId("web-note")).toHaveText(
    "Brave Search · your question leaves this machine",
  );
  // No Entries at all: `!s` has nothing to rank.
  await expect(page.getByRole("option")).toHaveCount(0);

  await fitTo(page, 0);
  await expect(page).toHaveScreenshot("palette-web-empty.png");
});

/**
 * The bug this covers: `!s` reserved a status row *and* drew the application
 * walk's notice, so two rows rendered in a window sized for one. The list
 * scrolled, and the scrollbar sat over the message.
 */
test("the bang shows exactly one row, whatever the walk is doing", async ({ page }) => {
  const input = await open(page);
  await page.evaluate(() =>
    (window as unknown as { __takyon_mock: Mock }).__takyon_mock.setIndexing(true),
  );
  await input.fill("!s ferrari in f1");

  await expect(page.getByTestId("web-note")).toBeVisible();
  await expect(page.getByText("Indexing applications…")).toHaveCount(0);

  // One row in a window sized for one, so nothing scrolls and nothing is hidden
  // behind a scrollbar.
  const list = page.locator("[cmdk-list]");
  const overflow = await list.evaluate((el) => el.scrollHeight - el.clientHeight);
  expect(overflow).toBeLessThanOrEqual(0);
});

test("a typed question offers Enter", async ({ page }) => {
  const input = await open(page);
  await input.fill("!s ferrari in f1");

  await expect(page.getByTestId("web-note")).toHaveText(
    "Search the web with Brave Search — press Enter",
  );
  await expect(page.getByText("Search", { exact: true })).toBeVisible();

  await fitTo(page, 0);
  await expect(page).toHaveScreenshot("palette-web-ready.png");
});

/**
 * The no-key state is not an error: it is what a fresh install is in, and its
 * fix is a Settings page rather than a retry.
 */
test("with no key stored the row says where to get one", async ({ page }) => {
  const input = await open(page, null);
  await input.fill("!s ferrari in f1");

  const note = page.getByTestId("web-note");
  await expect(note).toHaveText("No Brave Search key. Add one in Settings → Web search.");
  await expect(note).toHaveAttribute("role", "alert");
  // Nothing for Enter to do, so the footer offers nothing.
  await expect(page.getByText("Search", { exact: true })).toHaveCount(0);

  await fitTo(page, 0);
  await expect(page).toHaveScreenshot("palette-web-nokey.png");
});

/**
 * Arc Search's middle screen: the pages being read, by host, before a word of
 * the answer exists. This is what tells you whether to trust it.
 */
test("while it reads it names the pages by host", async ({ page }) => {
  const input = await open(page);
  // Held rather than raced: the mock answers in twenty milliseconds.
  await page.evaluate(() =>
    (window as unknown as { __takyon_mock: Mock }).__takyon_mock.holdSearchAtReading(true),
  );
  await page.setViewportSize({ width: 640, height: VIEW_HEIGHT });
  await input.fill("!s what happened in the chiefs game");
  await input.press("Enter");

  const reading = page.getByTestId("reading");
  await expect(reading).toContainText("Reading 6 web pages");
  await expect(reading).toContainText("espn.com");
  // The host, not the URL, and `www.` is dropped: it is noise on every row.
  await expect(reading).toContainText("theguardian.com");
  await expect(reading).not.toContainText("https://");
  await expect(page.getByRole("status")).toHaveText("Reading 6 web pages");

  await expect(page).toHaveScreenshot("palette-web-reading.png");
});

test("the answer is a headline and labelled findings, with its sources", async ({ page }) => {
  const input = await open(page);
  await page.setViewportSize({ width: 640, height: VIEW_HEIGHT });
  await input.fill("!s what happened in the chiefs game");
  await input.press("Enter");

  // The header is the outbound state: warm, and in words (v0.9 task 7).
  await expect(page.getByTestId("outbound")).toHaveText("Left this machine · Brave Search");

  const findings = page.getByTestId("findings");
  await expect(findings.getByRole("heading")).toHaveText(
    "Chiefs beat the Ravens to reach the Super Bowl",
  );
  await expect(findings).toContainText("Final score");
  await expect(findings).toContainText("Kansas City 17, Baltimore 10, at Baltimore.");
  // Reading across sources rather than summarising each: a disagreement between
  // two of them is a finding of its own.
  await expect(findings).toContainText("Sources disagree");
  // The citation numbers are gone from the prose and are buttons instead.
  await expect(findings).not.toContainText("[1][3]");
  await expect(findings.getByRole("button", { name: /^Source 1:/ })).toBeVisible();

  // The reading list has given way to the answer, and the sources sit below it.
  await expect(page.getByTestId("reading")).toHaveCount(0);
  await expect(page.getByTestId("sources")).toBeVisible();

  await expect(page).toHaveScreenshot("palette-web-answered.png");
});

/** A citation inside a finding opens the source it points at. */
test("a citation opens its own source", async ({ page }) => {
  const input = await open(page);
  await page.setViewportSize({ width: 640, height: VIEW_HEIGHT });
  await input.fill("!s what happened in the chiefs game");
  await input.press("Enter");
  await page.getByTestId("findings").getByRole("button", { name: /^Source 4:/ }).click();

  const opened = await page.evaluate(() =>
    (window as unknown as { __takyon_mock: Mock }).__takyon_mock.openedUrls(),
  );
  expect(opened).toContain("https://usatoday.com/sports/kelce");
});

/** A source row opens its URL, which is the only thing the list is for. */
test("a source opens in the browser", async ({ page }) => {
  const input = await open(page);
  await page.setViewportSize({ width: 640, height: VIEW_HEIGHT });
  await input.fill("!s what happened in the chiefs game");
  await input.press("Enter");
  await page.getByTestId("sources").getByRole("button", { name: /Chiefs beat Ravens/ }).click();

  const opened = await page.evaluate(() =>
    (window as unknown as { __takyon_mock: Mock }).__takyon_mock.openedUrls(),
  );
  expect(opened).toContain("https://espn.com/nfl/recap");
});

/**
 * A failure is shown in the words it arrived in. Rust's sentences carry the fix,
 * and replacing them with a generic one is how a fixable state becomes a dead end.
 */
test("a failed search says why in its own words", async ({ page }) => {
  const input = await open(page);
  await page.evaluate(() =>
    (window as unknown as { __takyon_mock: Mock }).__takyon_mock.failWebSearch(
      "Brave Search is rate limiting. Wait a moment and ask again.",
    ),
  );
  await page.setViewportSize({ width: 640, height: VIEW_HEIGHT });
  await input.fill("!s ferrari in f1");
  await input.press("Enter");

  await expect(page.getByRole("alert")).toHaveText(
    "Brave Search is rate limiting. Wait a moment and ask again.",
  );
  await expect(page).toHaveScreenshot("palette-web-failed.png");
});

test.describe("the Web Search settings page", () => {
  test.use({ viewport: { width: 880, height: 620 } });

  test("with no key it explains what !s does without one", async ({ page }) => {
    await page.goto("/?window=settings");
    await page.evaluate(() =>
      (window as unknown as { __takyon_mock: Mock }).__takyon_mock.setWebKeyStored(null),
    );
    await page.getByRole("button", { name: "Web Search" }).click();

    await expect(page.getByText(/Without a key/)).toBeVisible();
    await expect(page.getByLabel("Brave Search key")).toHaveAttribute("type", "password");
    await expect(page).toHaveScreenshot("settings-web-search.png");
  });

  test("a stored key is shown as its last four characters and nothing more", async ({ page }) => {
    await page.goto("/?window=settings");
    await page.getByRole("button", { name: "Web Search" }).click();

    await page.getByLabel("Brave Search key").fill("BSA-secret-value-4321");
    await page.getByRole("button", { name: "Save" }).click();

    await expect(page.getByText(/A key is stored \(…4321\)/)).toBeVisible();
    // The value itself must never come back to the webview.
    await expect(page.getByText("BSA-secret-value-4321")).toHaveCount(0);
    await expect(page.getByLabel("Brave Search key")).toHaveValue("");
    await expect(page).toHaveScreenshot("settings-web-search-stored.png");
  });
});
