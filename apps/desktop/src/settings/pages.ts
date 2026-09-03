/**
 * The pages this build has, and every control the search box can find.
 *
 * **This is the only file a new Source or Mode edits.** Appending an entry puts
 * it in the sidebar, in the right tier, in the right alphabetical position, and
 * makes its controls searchable — `nav.ts` does the rest and a test in
 * `nav.test.ts` holds that promise.
 *
 * Pages for features that do not exist yet are deliberately absent. File Search
 * arrives with v0.7 and AI with v0.9; shipping them now as disabled rows would
 * put dead controls in a window whose whole point is that every control does
 * something.
 */

import { About } from "./pages/About";
import { Calculator } from "./pages/Calculator";
import { General } from "./pages/General";
import type { SettingsPage } from "./nav";

export const PAGES: SettingsPage[] = [
  {
    id: "general",
    title: "General",
    tier: "app",
    Component: General,
    controls: [
      {
        id: "autostart",
        label: "Start Takyon when I log in",
        keywords: ["autostart", "startup", "login", "boot"],
      },
      {
        id: "motion",
        label: "Turn off animations",
        keywords: ["appearance", "motion", "reduce motion", "idle beat"],
      },
    ],
  },
  {
    id: "about",
    title: "About",
    tier: "app",
    Component: About,
    controls: [
      { id: "hotkey-status", label: "Global hotkey", keywords: ["alt space", "shortcut"] },
      { id: "identity", label: "Package identity", keywords: ["version", "slug", "data folder"] },
    ],
  },
  {
    id: "calculator",
    title: "Calculator",
    tier: "feature",
    Component: Calculator,
    controls: [
      {
        id: "calc-policy",
        label: "Answer arithmetic",
        keywords: ["calculator", "maths", "equals", "convert"],
      },
    ],
  },
];
