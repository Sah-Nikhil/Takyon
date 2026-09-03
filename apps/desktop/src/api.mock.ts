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

import type {
  Action,
  CalcPolicy,
  ClipRetention,
  ClipRow,
  ViewKind,
  Entry,
  HotkeyStatus,
  QueryResult,
  ShowPayload,
} from "@takyon/shared";

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
  // The two system kinds (task 8). Open only, no icon, no path, no version. A
  // curated page shares the App tier; a control-panel task sits below every app.
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
    kind: "systemTask",
    score: 700,
    actions: ["open"],
  },
];

/**
 * Clipboard history for the `!v` view (v0.5).
 *
 * Separate from `FIXTURES` because that is the point: a Clip is unreachable from
 * a Bangless query (ADR-0006), so the two lists must not be one list that a
 * filter happens to split.
 */
const CLIP_FIXTURES: Entry[] = [
  {
    id: "clip:31",
    title: "https://github.com/tauri-apps/tauri/releases",
    kind: "clip",
    score: 0,
    actions: ["paste", "copy_clip", "delete_clip"],
  },
  {
    id: "clip:30",
    title: "SELECT id, created_at FROM clips ORDER BY created_at DESC",
    kind: "clip",
    score: 0,
    actions: ["paste", "copy_clip", "delete_clip"],
  },
  {
    id: "clip:29",
    title: "com.v3sper.launcher",
    kind: "clip",
    score: 0,
    actions: ["paste", "copy_clip", "delete_clip"],
  },
];

/**
 * The Clipboard History command, as a Bangless row (v0.5).
 *
 * In `FIXTURES` rather than `CLIP_FIXTURES` on purpose: a command is reachable
 * Bangless and a clip never is (ADR-0006). The row carries no clip content.
 */
const COMMAND_FIXTURE: Entry = {
  id: "command:clipboard-history",
  title: "Clipboard History",
  subtitle: "Takyon",
  kind: "command",
  score: 700,
  actions: ["open_command"],
};

/** Rows for the history surface. Day offsets so grouping has something to do. */
const now = Math.floor(Date.now() / 1000);
const CLIP_ROWS: ClipRow[] = [
  {
    id: 31,
    createdAt: now - 600,
    kind: "text",
    sourceExe: "C:\\Program Files\\Mozilla Firefox\\firefox.exe",
    len: 44,
    preview: "https://github.com/tauri-apps/tauri/releases",
  },
  {
    id: 30,
    createdAt: now - 90_000,
    kind: "text",
    sourceExe: "C:\\Windows\\System32\\notepad.exe",
    len: 56,
    preview: "SELECT id, created_at FROM clips ORDER BY created_at DESC",
  },
  {
    id: 29,
    createdAt: now - 200_000,
    kind: "text",
    sourceExe: "C:\\Program Files\\Microsoft VS Code\\Code.exe",
    len: 19,
    preview: "com.v3sper.launcher",
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
  copy_answer: { id: "copy_answer", label: "Copy answer", accelerator: "Enter" },
  paste: { id: "paste", label: "Paste", accelerator: "Enter" },
  copy_clip: { id: "copy_clip", label: "Copy to clipboard", accelerator: "Ctrl+Enter" },
  delete_clip: {
    id: "delete_clip",
    label: "Delete from history",
    accelerator: "Ctrl+Backspace",
  },
  open_command: { id: "open_command", label: "Open Command", accelerator: "Enter" },
};

/** The `!v` Bang, parsed the way `bang.rs` parses it: position 0, then the rest. */
function clipQuery(q: string): string | null {
  if (!q.startsWith("!v")) return null;
  const rest = q.slice(2);
  if (rest !== "" && !/^\s/.test(rest)) return null;
  return rest.trim().toLowerCase();
}

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
  // Bangless never sees a clip (ADR-0006). Asserted in the visual layer, so the
  // mock has to be incapable of it rather than merely not doing it.
  if (entry.kind === "clip") return false;
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

/** The last View pushed across the seam, for a test to assert against. */
let lastView: ViewKind | null = null;
export function viewRequest() {
  return lastView;
}

/** Whether the browser build reports `!v` as enabled. Default matches Rust's. */
let lastBang = true;

/** The retention the browser build reports. Default matches Rust's. */
let lastRetention: ClipRetention = "1-month";
export function retentionRequest() {
  return lastRetention;
}

/** The last mode pushed across the seam, for a test to assert against. */
let lastCalcPolicy: CalcPolicy = "automatic";
export function calcPolicyRequest() {
  return lastCalcPolicy;
}

/**
 * Calculator rows for the visual layer, as a lookup table.
 *
 * **A fixture, not an implementation.** Reimplementing the real rules here would
 * give the screenshots something to agree with that is not the product. Answers
 * are copied from the Rust unit tests, so drift shows as a stale screenshot.
 */
const CALC_FIXTURES: Record<string, string> = {
  "12*1.18": "14.16",
  "10+30%": "13",
  "40 kg to lb": "88.1849 lb",
  "2024": "2,024",
};

function calcFixture(q: string): Entry[] {
  const answer = CALC_FIXTURES[q.trim()];
  if (!answer) return [];
  return [
    {
      id: `calc:${answer}`,
      title: answer,
      subtitle: q.trim(),
      kind: "calc",
      score: 1000,
      actions: ["copy_answer"],
    },
  ];
}

export const mock = {
  dismiss: async () => {
    emitHide();
  },
  openSettings: async () => {},
  query: async (q: string, seq: number): Promise<QueryResult> => {
    // `!v` is its own view. Unlike a Bangless query, an empty one lists history
    // rather than nothing — the Mode *is* the list.
    const clips = clipQuery(q);
    if (clips !== null) {
      return {
        seq,
        entries: CLIP_FIXTURES.filter((e) => e.title.toLowerCase().includes(clips)),
        indexing: false,
      };
    }
    return {
      seq,
      entries: q.trim()
        ? [
            ...calcFixture(q),
            ...[...FIXTURES, COMMAND_FIXTURE].filter((e) => matches(e, q)),
          ]
        : [],
      indexing: q.trim() ? indexing : false,
    };
  },
  /** The same table Rust ships, so the footer draws in the browser build too. */
  actionLabels: async (): Promise<Action[]> => Object.values(ACTION_LABELS),
  actionsFor: async (entryId: string): Promise<Action[]> => {
    // A calc id is not in FIXTURES: it is minted per query, exactly as Rust mints
    // one per keystroke, so its menu comes from the id rather than a lookup.
    const entry = entryId.startsWith("calc:")
      ? { actions: ["copy_answer"] }
      : (CLIP_FIXTURES.find((e) => e.id === entryId) ??
        [...FIXTURES, COMMAND_FIXTURE].find((e) => e.id === entryId));
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
  /** Recorded, not enforced: the rule this sets lives in Rust (TBC-0007). */
  setCalcPolicy: async (mode: CalcPolicy) => {
    lastCalcPolicy = mode;
  },
  activate: async (_entryId: string, actionId: string) => {
    // Launching is the one thing the browser build genuinely cannot do. Hiding is
    // what the real path does first, so the mock does that much and stops —
    // except for the one action Rust does not hide for.
    if (actionId !== "delete_clip") emitHide();
  },
  /** Retention, recorded rather than applied: there is no history here to sweep. */
  clipRetention: async (): Promise<ClipRetention> => lastRetention,
  clipRetentionImpact: async (_value: ClipRetention) => CLIP_FIXTURES.length,
  setClipRetention: async (value: ClipRetention) => {
    lastRetention = value;
    return 0;
  },
  clipClear: async () => 0,
  /** Recorded, not applied: there is no native window here to resize. */
  setView: async (view: ViewKind | null) => {
    lastView = view;
  },
  clipPage: async (query: string, limit?: number) => {
    const q = query.trim().toLowerCase();
    const hits = q
      ? CLIP_ROWS.filter((c) => c.preview.toLowerCase().includes(q))
      : CLIP_ROWS;
    return hits.slice(0, limit ?? 200);
  },
  clipBang: async () => lastBang,
  setClipBang: async (on: boolean) => {
    lastBang = on;
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
