import { defineConfig, devices } from "@playwright/test";

/**
 * TBC-0007's layer: the React UI in the ordinary Vite dev server with the Tauri
 * seam mocked, screenshotted for regressions.
 *
 * It cannot catch a bug where the UI is right and the Rust behind it is wrong —
 * fixture data always renders beautifully. That exposure is accepted here and
 * answered by contract tests on the real serialised output, which TBC-0007 names
 * as the first thing to add when it starts to bite.
 */
export default defineConfig({
  testDir: "./tests/visual",
  snapshotPathTemplate: "{testDir}/__screenshots__/{arg}{ext}",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: "http://localhost:1420",
    // Screenshot diffs are only useful if the renderer is deterministic.
    colorScheme: "dark",
  },
  expect: {
    toHaveScreenshot: {
      animations: "disabled",
      caret: "hide",
      // Font rasterisation differs a little between machines; zero tolerance
      // makes the suite fail for reasons that are not regressions.
      maxDiffPixelRatio: 0.01,
    },
  },
  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        // After the device spread, not before: `Desktop Chrome` carries its own
        // 1280x720 viewport, and a baseline taken at that size is a picture of a
        // window shape that does not exist. These are the Palette's real
        // dimensions from `tauri.conf.json`.
        viewport: { width: 640, height: 68 },
        deviceScaleFactor: 1,
      },
    },
  ],
  webServer: {
    command: "bun run dev",
    url: "http://localhost:1420",
    reuseExistingServer: !process.env.CI,
    stdout: "ignore",
  },
});
