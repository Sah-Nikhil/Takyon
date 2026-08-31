/**
 * The browser-side stand-in for the Tauri bridge.
 *
 * This is what makes TBC-0007's visual layer possible: with these
 * implementations, the whole React app runs in the ordinary Vite dev server and
 * Playwright can drive it deterministically, with no Tauri, no hotkey and no
 * focus rules to fight.
 *
 * The exposure is worth restating because it is silent: **nothing here can catch
 * a bug where the UI is right and the Rust behind it is wrong.** Fixture data
 * always renders beautifully. Contract tests on the real serialised output are
 * the answer to that, and TBC-0007 already names them as the first thing to add.
 */

import type { Action, Entry, HotkeyStatus, QueryResult, ShowPayload } from "@takyon/shared";

type ShowListener = (payload: ShowPayload) => void;

const showListeners = new Set<ShowListener>();
const hideListeners = new Set<() => void>();
let nextShowId = 1;

/**
 * Drive a show from a test or from the browser console. Exposed on `window` in
 * `main.tsx` when running outside Tauri — Playwright has no hotkey to press, so
 * without this the Palette in the dev server would never receive the event that
 * clears and focuses it.
 */
export function emitShow(noFocusSteal = true) {
  const payload: ShowPayload = { showId: nextShowId++, noFocusSteal };
  for (const l of showListeners) l(payload);
}

export function emitHide() {
  for (const l of hideListeners) l();
}

/**
 * A small, fixed set of applications the browser build matches against.
 *
 * Covers the verification script's three matching cases plus a packaged app and a
 * Steam game, so every Entry shape the row renderer draws is exercised. `actions`
 * differs between them, or the `Ctrl+K` menu could never be wrong.
 */
const FIXTURES: Entry[] = [
  {
    id: "c:\\program files\\adobe\\photoshop.exe",
    title: "Adobe Photoshop",
    subtitle: "C:\\Program Files\\Adobe\\Photoshop.exe",
    kind: "app",
    icon: "0000000000000001",
    score: 700,
    actions: ["open", "run_as_admin", "reveal", "copy_path"],
  },
  {
    id: "c:\\program files\\microsoft vs code\\code.exe",
    title: "Visual Studio Code",
    subtitle: "C:\\Program Files\\Microsoft VS Code\\Code.exe",
    kind: "app",
    icon: "0000000000000002",
    score: 700,
    actions: ["open", "run_as_admin", "reveal", "copy_path"],
  },
  {
    id: "c:\\windows\\system32\\notepad.exe",
    title: "Notepad",
    subtitle: "C:\\Windows\\System32\\notepad.exe",
    kind: "app",
    icon: "0000000000000003",
    score: 800,
    actions: ["open", "run_as_admin", "reveal", "copy_path"],
  },
  {
    id: "aumid:Microsoft.WindowsCalculator_8wekyb3d8bbwe!App",
    title: "Calculator",
    subtitle: "Store app",
    kind: "app",
    icon: "0000000000000004",
    score: 800,
    actions: ["open"],
  },
  /*
    Two installs of one tool, which is the only case that carries a version. The
    titles differ and the paths differ, so nothing else on the row tells them
    apart — see `version.rs`.
   */
  {
    id: "c:\\nvm4w\\nodejs\\node.exe",
    title: "node",
    subtitle: "C:\\nvm4w\\nodejs\\node.exe",
    kind: "app",
    score: 700,
    actions: ["open", "run_as_admin", "reveal", "copy_path"],
    version: "24.14.1",
  },
  {
    id: "c:\\program files\\nodejs\\node.exe",
    title: "Node.js",
    subtitle: "C:\\Program Files\\nodejs\\node.exe",
    kind: "app",
    score: 700,
    actions: ["open", "run_as_admin", "reveal", "copy_path"],
    version: "26.7",
  },
  {
    id: "steam:440",
    title: "Team Fortress 2",
    subtitle: "Steam",
    kind: "app",
    score: 700,
    actions: ["open"],
  },
  // A settings page (task 8). Its own kind, Open only, no icon and no path —
  // it sorts below applications and never carries a version.
  {
    id: "ms-settings:bluetooth",
    title: "Bluetooth",
    kind: "system",
    score: 900,
    actions: ["open"],
  },
  {
    id: "system:change how your keyboard works",
    title: "Change how your keyboard works",
    kind: "system",
    score: 700,
    actions: ["open"],
  },
];

const ACTION_LABELS: Record<string, Action> = {
  open: { id: "open", label: "Open", accelerator: "Enter" },
  run_as_admin: {
    id: "run_as_admin",
    label: "Run as administrator",
    accelerator: "Ctrl+Enter",
  },
  reveal: { id: "reveal", label: "Open file location", accelerator: "Ctrl+Shift+Enter" },
  copy_path: { id: "copy_path", label: "Copy path", accelerator: "Ctrl+Shift+C" },
};

/**
 * A deliberately crude stand-in for `rank.rs`.
 *
 * Substring, not the six-rung ladder. Reimplementing it here would be a second
 * matcher agreeing with the real one only until someone changed one. Being
 * obviously not the real thing is the property that matters.
 */
function matches(entry: Entry, needle: string): boolean {
  const q = needle.trim().toLowerCase();
  if (!q) return false;
  const words = entry.title.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean);
  const acronym = words.map((w) => w[0]).join("");
  return (
    entry.title.toLowerCase().startsWith(q) ||
    words.some((w) => w.startsWith(q)) ||
    (q.length >= 2 && acronym.startsWith(q)) ||
    (entry.subtitle ?? "").toLowerCase().includes(`\\${q}`)
  );
}

/**
 * Force the browser build to report an in-progress application walk.
 *
 * Exposed on `window` alongside `emitShow` so Playwright can screenshot the
 * indexing state, which is otherwise a sub-second window that no test could catch
 * reliably.
 */
let indexing = false;
export function setIndexing(on: boolean) {
  indexing = on;
}

/**
 * Whether the browser build reports the hotkey as registered.
 *
 * From `?hotkey=failed` on the URL, not a setter: the Palette asks in a mount
 * effect, so a setter would lose that race. Not hypothetical — Raycast holds
 * `Alt+Space` here, so the banner is the default view.
 */
const hotkeyRegistered =
  typeof window === "undefined" ||
  new URLSearchParams(window.location.search).get("hotkey") !== "failed";

/** The last value passed to `setActionMenu`, for the visual layer to assert on. */
let lastMenuRequest: number | null = null;
export function menuRequest() {
  return lastMenuRequest;
}

/** The last value passed to `setBannerHeight`, likewise. */
let lastBannerHeight = 0;
export function bannerRequest() {
  return lastBannerHeight;
}

export const mock = {
  dismiss: async () => {
    emitHide();
  },
  openSettings: async () => {},
  query: async (q: string, seq: number): Promise<QueryResult> => ({
    seq,
    entries: q.trim() ? FIXTURES.filter((e) => matches(e, q)) : [],
    indexing: q.trim() ? indexing : false,
  }),
  actionsFor: async (entryId: string): Promise<Action[]> => {
    const entry = FIXTURES.find((e) => e.id === entryId);
    // `flatMap`, not `map().filter(Boolean)`: with `noUncheckedIndexedAccess`
    // the lookup is `Action | undefined` and `filter` does not narrow it.
    // Returning `[]` for an unknown id also mirrors Rust.
    return (entry?.actions ?? []).flatMap((id) => {
      const action = ACTION_LABELS[id];
      return action ? [action] : [];
    });
  },
  /**
   * No window to resize outside Tauri. Recorded rather than ignored so a test can
   * at least assert the Palette *told* the window — the resize itself is Rust's,
   * and its arithmetic is unit-tested there.
   */
  setActionMenu: async (actions: number | null) => {
    lastMenuRequest = actions;
  },
  /** Also recorded rather than acted on: there is no window here to grow. */
  setBannerHeight: async (height: number) => {
    lastBannerHeight = Math.ceil(height);
  },
  activate: async (_entryId: string, _actionId: string) => {
    // Launching is the one thing the browser build genuinely cannot do. Hiding is
    // what the real path does first, so the mock does that much and stops.
    emitHide();
  },
  /**
   * No protocol handler outside Tauri, so no icon. The empty string makes the row
   * render its placeholder, which is what keeps the screenshots identical on every
   * machine regardless of what is installed on it.
   */
  iconUrl: (_key: string) => "",
  hotkeyStatus: async (): Promise<HotkeyStatus> => ({
    accelerator: "Alt+Space",
    registered: hotkeyRegistered,
    ...(hotkeyRegistered
      ? {}
      : { error: "Another application is already holding it." }),
  }),
  reportFirstPixel: async (_showId: number) => {},
  reportFirstEntry: async (_seq: number) => {},
  autostartIsEnabled: async () => false,
  autostartSetEnabled: async (_on: boolean) => {},
  onShow: (cb: ShowListener) => {
    showListeners.add(cb);
    return () => {
      showListeners.delete(cb);
    };
  },
  onHide: (cb: () => void) => {
    hideListeners.add(cb);
    return () => {
      hideListeners.delete(cb);
    };
  },
};
