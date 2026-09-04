/**
 * The pages this build has, and every control the search box can find.
 *
 * **This is the only file a new Source or Mode edits.** Appending an entry puts
 * it in the sidebar, in the right tier, in the right alphabetical position, and
 * makes its controls searchable — `nav.ts` does the rest and a test in
 * `nav.test.ts` holds that promise.
 *
 * Pages for features that do not exist yet are deliberately absent. AI arrives
 * with v0.9; shipping it now as a disabled row would put dead controls in a
 * window whose whole point is that every control does something. File Search
 * landed at v0.7 and its page came with it.
 */

import { About } from "./pages/About";
import { Advanced } from "./pages/Advanced";
import { Applications } from "./pages/Applications";
import { Calculator } from "./pages/Calculator";
import { ClipboardHistory } from "./pages/ClipboardHistory";
import { FileSearch } from "./pages/FileSearch";
import { General } from "./pages/General";
import { Keyboard } from "./pages/Keyboard";
import { Launcher } from "./pages/Launcher";
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
        id: "theme",
        label: "Appearance",
        keywords: ["theme", "dark", "light", "colour", "color"],
      },
      {
        id: "ui-size",
        label: "Interface size",
        keywords: ["zoom", "scale", "bigger", "smaller", "text size"],
      },
      {
        id: "motion",
        label: "Turn off animations",
        keywords: ["appearance", "motion", "reduce motion", "idle beat"],
      },
    ],
  },
  {
    id: "launcher",
    title: "Launcher",
    tier: "app",
    Component: Launcher,
    controls: [
      {
        id: "placement",
        label: "Open the Palette on",
        keywords: ["monitor", "screen", "placement", "multi-monitor"],
      },
      { id: "tray", label: "Show the tray icon", keywords: ["tray", "notification area"] },
      {
        id: "recents",
        label: "Include recent files",
        keywords: ["recents", "documents", "sources"],
      },
    ],
  },
  {
    id: "keyboard",
    title: "Keyboard",
    tier: "app",
    Component: Keyboard,
    controls: [
      {
        id: "hotkey",
        label: "Open Takyon with",
        keywords: ["hotkey", "shortcut", "alt space", "rebind", "chord"],
      },
    ],
  },
  {
    id: "advanced",
    title: "Advanced",
    tier: "app",
    Component: Advanced,
    controls: [
      {
        id: "crash-logs",
        label: "Crash logs",
        keywords: ["diagnostics", "panic", "logs", "telemetry"],
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
    id: "applications",
    title: "Applications",
    tier: "feature",
    Component: Applications,
    controls: [
      {
        id: "aliases",
        label: "Type a short name, get the application",
        keywords: ["alias", "aliases", "shorthand", "rename"],
      },
    ],
  },
  {
    id: "clipboard",
    title: "Clipboard History",
    tier: "feature",
    Component: ClipboardHistory,
    controls: [
      {
        id: "retention",
        label: "Keep history for",
        keywords: ["retention", "delete", "expiry", "how long"],
      },
      { id: "clip-bang", label: "Reach history with !v", keywords: ["bang", "!v"] },
      {
        id: "blocklist",
        label: "Excluded applications",
        keywords: ["blocklist", "password manager", "never record", "exclude"],
      },
    ],
  },
  {
    id: "files",
    title: "File Search",
    tier: "feature",
    Component: FileSearch,
    controls: [
      {
        id: "index-roots",
        label: "Indexed folders",
        keywords: ["roots", "folders", "index", "where", "scope", "add folder"],
      },
      {
        id: "index-excludes",
        label: "Skipped folder names",
        keywords: ["exclude", "skip", "node_modules", "ignore", "target"],
      },
      {
        id: "files-bangless",
        label: "Show files without typing !e",
        keywords: ["bang", "!e", "bangless", "results"],
      },
      {
        id: "files-fallback",
        label: "Also ask Windows Search",
        keywords: ["windows search", "fallback", "outside", "everywhere"],
      },
      {
        id: "clear-opened",
        label: "Files you opened through Takyon",
        keywords: ["history", "recents", "clear", "forget", "privacy"],
      },
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
