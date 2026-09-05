import { expect, test, type Page } from "@playwright/test";
import { paletteHeight, VIEW_HEIGHT } from "@takyon/shared";

/**
 * v0.9's visual layer: the `!c` row, the conversation, and the Agents page.
 *
 * TBC-0007's exposure is total — **every Agent, Sign-in state and token is a
 * fixture in `api.mock.ts`.** Whether a real `claude auth status --json` parses
 * is answered in `agents/claude.rs`. This catches whether the surfaces draw.
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
  page.setViewportSize({ width: 640, height: paletteHeight(rows, true) });

test("!c alone names the agent that would answer", async ({ page }) => {
  const input = await open(page);
  await input.fill("!c");

  // The default fixture is Claude, signed in on Pro.
  await expect(page.getByText("Claude Code · Authenticated · Claude Pro Subscription")).toBeVisible();
  // No Entries at all: `!c` has nothing to rank.
  await expect(page.getByRole("option")).toHaveCount(0);

  await fitTo(page, 0);
  await expect(page).toHaveScreenshot("palette-ask-empty.png");
});

test("!c with a question offers Enter rather than answering as you type", async ({ page }) => {
  const input = await open(page);
  await input.fill("!c why is the sky blue");
  await expect(page.getByText("Ask Claude Code — press Enter")).toBeVisible();

  await fitTo(page, 0);
  await expect(page).toHaveScreenshot("palette-ask-ready.png");
});

test("Enter streams the answer into the palette", async ({ page }) => {
  const input = await open(page);
  await input.fill("!c why is the sky blue");
  await input.press("Enter");

  await expect(page.getByRole("status")).toHaveText("Answering…");
  await expect(page.getByText("The sky is blue because of Rayleigh scattering.")).toBeVisible();
  // The question stays on screen: an answer with no question above it is a
  // paragraph from nowhere.
  await expect(page.getByText("why is the sky blue")).toBeVisible();

  await page.setViewportSize({ width: 640, height: VIEW_HEIGHT });
  await expect(page).toHaveScreenshot("palette-ask-answered.png");
});

/**
 * The whole conversation stays in this window. A follow-up used to open a second
 * one; now it appends here, and the first exchange is still on screen.
 */
test("a follow-up continues in the same window", async ({ page }) => {
  const input = await open(page);
  await input.fill("!c why is the sky blue");
  await input.press("Enter");
  await expect(page.getByText("The sky is blue because of Rayleigh scattering.")).toBeVisible();

  const followUp = page.getByPlaceholder("Ask a follow-up");
  await followUp.fill("and why is that");
  await followUp.press("Enter");

  // Both questions and the first answer are still here.
  await expect(page.getByText("why is the sky blue")).toBeVisible();
  await expect(page.getByText("and why is that")).toBeVisible();
  await expect(page.getByText("The sky is blue because of Rayleigh scattering.")).toHaveCount(2);

  await page.setViewportSize({ width: 640, height: VIEW_HEIGHT });
  await expect(page).toHaveScreenshot("palette-ask-followup.png");
});

/**
 * The preference order falling through. First choice is signed out, so `!c`
 * reaches the next one that can answer, and names what it skipped.
 */
test("a signed-out first choice falls through to the next agent", async ({ page }) => {
  const input = await open(page);
  // Set here rather than in Settings: navigating between the two windows is a
  // page load, and the mock's copy of the preference dies with the page.
  await page.evaluate(() => {
    (
      window as unknown as { __takyon_mock: { setAskOrder: (o: string[]) => void } }
    ).__takyon_mock.setAskOrder(["opencode", "claude", "codex"]);
  });
  await input.fill("!c why is the sky blue");
  await expect(page.getByText("Ask Claude Code")).toBeVisible();
  await expect(page.getByText("opencode unavailable")).toBeVisible();

  await input.press("Enter");
  await expect(page.getByText("The sky is blue because of Rayleigh scattering.")).toBeVisible();
});

/**
 * ADR-0017's visible half. With nothing left to fall through to, `!c` gets one
 * sentence carrying the command, and no row to press.
 */
test("no agent left to ask says what to run instead of asking", async ({ page }) => {
  const input = await open(page);
  await page.evaluate(() => {
    const mock = (
      window as unknown as {
        __takyon_mock: {
          setAskOrder: (o: string[]) => void;
          setAgentSignedOut: (k: string) => void;
        };
      }
    ).__takyon_mock;
    mock.setAgentSignedOut("claude");
    mock.setAskOrder(["opencode", "claude", "codex"]);
  });
  await input.fill("!c why is the sky blue");
  await expect(page.getByRole("alert")).toContainText("opencode providers login");

  await input.press("Enter");
  // Still the palette, not the answer view: the Turn never started.
  await expect(page.getByRole("alert")).toBeVisible();
});

test("the agents page shows one card per agent", async ({ page }) => {
  await page.setViewportSize({ width: 880, height: 620 });
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Agents" }).click();

  await expect(page.getByText("Authenticated · Claude Pro Subscription")).toBeVisible();
  // Exact: the headline and the sentence under it both contain these words.
  await expect(page.getByText("Not found", { exact: true })).toBeVisible();
  await expect(page.getByText("Not authenticated", { exact: true })).toBeVisible();
  // No Sign in button anywhere on the page (ADR-0017).
  await expect(page.getByRole("button", { name: /sign in/i })).toHaveCount(0);

  await expect(page).toHaveScreenshot("settings-agents.png");
});

/**
 * The switch is what lets `!c` name its Agent on the first keystroke: a
 * switched-off Agent is skipped without being probed.
 */
test("an agent can be switched off, and !c stops reaching it", async ({ page }) => {
  await page.setViewportSize({ width: 880, height: 620 });
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Agents" }).click();

  const claude = page.getByRole("switch", { name: "Use Claude Code for !c" });
  await expect(claude).toHaveAttribute("aria-checked", "true");
  await claude.click();
  await expect(claude).toHaveAttribute("aria-checked", "false");
  // Off replaces the whole status line, and takes the pickers with it: a model
  // locked to an Agent `!c` will never reach means nothing.
  await expect(page.getByText("Off", { exact: true })).toBeVisible();
  await expect(page.getByLabel("Model for Claude Code")).toHaveCount(0);

  await expect(page).toHaveScreenshot("settings-agents-off.png");
});

/** Ranking is buttons, not a drag, so the order is reachable from the keyboard. */
test("the preference order can be reordered from the keyboard", async ({ page }) => {
  await page.setViewportSize({ width: 880, height: 620 });
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Agents" }).click();

  // Claude leads, so it cannot go up; opencode is last and cannot go down.
  await expect(page.getByRole("button", { name: "Move Claude Code up" })).toBeDisabled();
  await expect(page.getByRole("button", { name: "Move opencode down" })).toBeDisabled();

  await page.getByRole("button", { name: "Move Codex up" }).click();
  await expect(page.getByRole("button", { name: "Move Codex up" })).toBeDisabled();
  await expect(page.getByRole("button", { name: "Move Claude Code up" })).toBeEnabled();
});

/**
 * The model is a list, never free text, and it only exists for an Agent that can
 * answer. Picking one locks it: it is the only model a Turn can use.
 */
test("model and effort are locked from a list, per authenticated agent", async ({ page }) => {
  await page.setViewportSize({ width: 880, height: 620 });
  await page.goto("/?window=settings");
  await page.getByRole("button", { name: "Agents" }).click();

  const model = page.getByLabel("Model for Claude Code");
  await expect(model).toBeEnabled();
  await expect(model.locator("option")).toHaveText([
    "Agent default",
    "opus",
    "sonnet",
    "haiku",
    "fable",
  ]);
  await model.selectOption("opus");
  await expect(model).toHaveValue("opus");

  // Effort comes from the agent's own vocabulary, which differs per agent.
  const effort = page.getByLabel("Effort for Claude Code");
  await expect(effort.locator("option")).toHaveText([
    "Agent default",
    "low",
    "medium",
    "high",
    "xhigh",
    "max",
  ]);
  await effort.selectOption("high");
  await expect(effort).toHaveValue("high");

  // Codex is not installed and opencode is signed out: neither gets a picker.
  await expect(page.getByLabel("Model for Codex")).toHaveCount(0);
  await expect(page.getByText("Sign in to choose a model")).toHaveCount(2);

  await expect(page).toHaveScreenshot("settings-agents-locked.png");
});
