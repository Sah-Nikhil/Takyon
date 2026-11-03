import { test as base } from "@playwright/test";

/**
 * Wall clock every visual spec renders against.
 *
 * Clip rows in `api.mock.ts` are offsets from `Date.now()`, so a baseline that
 * shows a weekday or a timestamp drifts every day. `setFixedTime` pins Date and
 * leaves timers alone, unlike `clock.install`, which stops debounces firing.
 */
export const FIXED_NOW = new Date("2026-01-15T14:30:00Z");

// Playwright's second argument is `use` by convention; named `run` here because
// `react-hooks/rules-of-hooks` reads a bare `use(...)` call as a React hook.
export const test = base.extend({
  page: async ({ page }, run) => {
    await page.clock.setFixedTime(FIXED_NOW);
    // `goto` resolves on `load`, which on a cold CI Vite can precede `main.tsx`
    // installing the mock: `__takyon_mock` read as undefined once on PR #22.
    const goto = page.goto.bind(page);
    page.goto = async (url, options) => {
      const response = await goto(url, options);
      await page.waitForFunction(() => "__takyon_mock" in window);
      return response;
    };
    await run(page);
  },
});

export { expect, type Page } from "@playwright/test";
