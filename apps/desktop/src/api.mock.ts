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
  AgentKind,
  AgentSettings,
  AgentSnapshot,
  AliasRow,
  AppAliasRow,
  TurnMessage,
  CalcPolicy,
  ClipRetention,
  ClipRow,
  ViewKind,
  Entry,
  FileIndexReport,
  HotkeyStatus,
  QueryResult,
  SearchHit,
  SearchMessage,
  SettingsSnapshot,
  ShowPayload,
  WebSettings,
} from "@takyon/shared";

type ShowListener = (payload: ShowPayload) => void;

const showListeners = new Set<ShowListener>();
const hideListeners = new Set<() => void>();
const turnListeners = new Set<(message: TurnMessage) => void>();
let nextShowId = 1;
let nextTurnId = 1;

/**
 * One Agent of each state, so every card the Settings page can draw is on screen
 * at once: signed in, signed out, and not installed at all.
 */
let agentFixtures: AgentSnapshot[] = [
  {
    kind: "claude",
    label: "Claude Code",
    binary: "claude",
    installed: true,
    version: "2.1.261",
    health: "ready",
    signIn: {
      status: "in",
      label: "Claude Pro Subscription",
      account: "you@example.com",
    },
    efforts: ["low", "medium", "high", "xhigh", "max"],
  },
  {
    kind: "codex",
    label: "Codex",
    binary: "codex",
    installed: false,
    health: "error",
    signIn: { status: "unknown" },
    message: "Codex (`codex`) was not found on PATH.",
    efforts: [],
  },
  {
    kind: "opencode",
    label: "opencode",
    binary: "opencode",
    installed: true,
    version: "1.18.27",
    health: "warning",
    signIn: { status: "out" },
    message: "No providers are connected to opencode. Run `opencode providers login`.",
    efforts: ["minimal", "high", "max"],
  },
];

/** Split into deltas on purpose — see `agentAsk` for why one event is not enough. */
let MOCK_ANSWER = ["The ", "sky ", "is ", "blue ", "because ", "of ", "Rayleigh ", "scattering."];

/**
 * Replace what the mock Agent answers, so a test can drive the renderer with
 * markdown. Split on spaces for the same reason the default is split: one event
 * would never catch a renderer that overwrites instead of appending.
 */
export function setAnswer(text: string) {
  MOCK_ANSWER = text.split(/(?<=\s)/);
}

let askOrder: AgentKind[] = ["claude", "codex", "opencode"];
let askEnabled: Record<AgentKind, boolean> = { claude: true, codex: true, opencode: true };
let askCwd = "";
let askModels: Partial<Record<AgentKind, string>> = {};
let askEfforts: Partial<Record<AgentKind, string>> = {};

/** What each Agent would report from its own model listing. */
const AGENT_MODELS: Record<AgentKind, string[]> = {
  claude: ["opus", "sonnet", "haiku", "fable"],
  codex: ["gpt-5.3-codex", "gpt-5.3-codex-mini"],
  opencode: ["opencode/big-pickle", "opencode/nemotron-3-ultra-free"],
};

/** Hits the mock provider returns. Six, the number Arc Search reads. */
const SEARCH_HITS: SearchHit[] = [
  {
    title: "Chiefs beat Ravens in AFC Championship",
    url: "https://espn.com/nfl/recap",
    description: "Kansas City held on to win 17-10 at Baltimore.",
  },
  {
    title: "AFC Championship: as it happened",
    url: "https://www.theguardian.com/sport/live",
    description: "Minute by minute coverage of the game.",
  },
  {
    title: "Chiefs 17-10 Ravens: box score",
    url: "https://cnn.com/sport/box-score",
    description: "Scoring plays, turnovers and drive charts.",
  },
  {
    title: "Kelce's night in numbers",
    url: "https://usatoday.com/sports/kelce",
    description: "Eleven catches for 116 yards.",
  },
  {
    title: "What the win means for the Super Bowl",
    url: "https://today.com/sports/super-bowl",
    description: "Kansas City reach their fourth in five seasons.",
  },
  {
    title: "Reaction from both locker rooms",
    url: "https://twitter.com/nfl/status/1",
    description: "Players and coaches after the final whistle.",
  },
];

/**
 * Arc Search's answer shape: a headline, then labelled findings with the sources
 * behind each one. Split into deltas for the same reason `MOCK_ANSWER` is.
 */
const MOCK_SYNTHESIS = [
  "HEADLINE: Chiefs beat the Ravens to reach the Super Bowl\n",
  "- **Final score** — Kansas City 17, Baltimore 10, at Baltimore. [1][3]\n",
  "- **Key play** — An interception in the fourth quarter ended the last drive. [2]\n",
  "- **Standout** — Travis Kelce caught eleven passes for 116 yards. [4]\n",
  "- **What is next** — Kansas City reach their fourth Super Bowl in five seasons. [5]\n",
  "- **Sources disagree** — [2] calls the fumble a muffed catch; [3] scores it a fumble. [2][3]\n",
];

/** What the mock was asked to open, so a test can assert on it. */
const opened: string[] = [];

/** Everything opened since the page loaded. Exposed on `window` in `main.tsx`. */
export function openedUrls(): string[] {
  return [...opened];
}

let webKey: string | null = null;
let webFailure: string | null = null;
let nextSearchId = 1;
const searchListeners = new Set<(message: SearchMessage) => void>();

function emitSearch(message: SearchMessage) {
  for (const l of searchListeners) l(message);
}

/**
 * Store or clear the key without going through Settings, so a Palette test can
 * reach the no-key state. Rust keeps this DPAPI-wrapped on disk; the mock's copy
 * dies with the page.
 */
export function setWebKeyStored(key: string | null) {
  webKey = key;
}

/**
 * Hold a search at its reading phase until released. The mock answers in about
 * twenty milliseconds, so asserting on a phase means racing it; a test that
 * wants the reading list asks for it to stand still instead.
 */
let holdReading = false;
let releaseReading: (() => void) | null = null;

export function holdSearchAtReading(on: boolean) {
  holdReading = on;
  if (!on && releaseReading) {
    releaseReading();
    releaseReading = null;
  }
}

/** Make the next search fail with this message. Null restores success. */
export function failWebSearch(message: string | null) {
  webFailure = message;
}

function emitTurn(message: TurnMessage) {
  for (const l of turnListeners) l(message);
}

/**
 * Rank the Agents `!c` tries, without going through Settings.
 *
 * Exposed on `window` in `main.tsx` outside Tauri. Rust persists this in
 * `settings.db`; the mock's copy dies with the page, so a test that reordered
 * in the Settings window would find the default order again in the Palette.
 */
export function setAskOrder(order: AgentKind[]) {
  askOrder = [...order];
}

/**
 * Sign one Agent out, so a test can reach the state where nothing can answer.
 *
 * The fixtures leave one Agent signed in, which since the preference became an
 * order is what stops `!c` being blocked: it falls through instead. Blocked now
 * means every Agent is out, and this is how a test gets there.
 */
export function setAgentSignedOut(kind: AgentKind) {
  agentFixtures = agentFixtures.map((snapshot) =>
    snapshot.kind === kind
      ? { ...snapshot, health: "warning" as const, signIn: { status: "out" as const } }
      : snapshot,
  );
}

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
    title: "com.v3sper.takyon",
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
    preview: "com.v3sper.takyon",
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

/** The `!c` Bang, parsed the same way. Case is kept: it is a question, not a key. */
function askQuery(q: string): string | null {
  if (!/^!c/i.test(q)) return null;
  const rest = q.slice(2);
  if (rest !== "" && !/^\s/.test(rest)) return null;
  return rest.trim();
}

/** The `!s` Bang, parsed the same way. Case is kept: it is a question. */
function webQuery(q: string): string | null {
  if (!/^!s/i.test(q)) return null;
  const rest = q.slice(2);
  if (rest !== "" && !/^\s/.test(rest)) return null;
  return rest.trim();
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

/** What the browser build reports as bound. Moved by `setHotkey`. */
let liveHotkey = "Alt+Space";

/** Whether the browser build's title bar shows the restore glyph. */
let maximized = false;

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

/** The preferences the browser build reports. Defaults match Rust's. */
let snapshot: SettingsSnapshot = {
  reduceMotion: false,
  calcPolicy: "automatic",
  recents: true,
  tray: true,
  placement: "cursor",
  clipRetention: "1-month",
  clipBang: true,
  theme: "system",
  uiSize: "default",
  filesBangless: false,
  filesFallback: false,
  filesRoots: ["C:\\Users\\you\\Documents", "C:\\Programming"],
  filesExcludes: ["node_modules", ".git", "target"],
};

/** Rows in the owned recents list, for the clear-history confirmation. */
let openedRows = 12;

/** Executables excluded from clipboard capture, as the browser build reports. */
let blocked: string[] = ["keepass.exe", "1password.exe"];

/** Aliases the browser build reports. One points at nothing, deliberately. */
let aliasRows: AliasRow[] = [
  { alias: "ps", target: "app:photoshop", title: "Adobe Photoshop 2022" },
  { alias: "vpn", target: "app:gone" },
];

/**
 * Applications for the alias editor, title-sorted as Rust sorts them.
 *
 * Deliberately mixed: two with an alias, one with two, and several with none,
 * because the empty state is the row the editor exists to fill. Four carry an
 * icon key and four do not, so the initial-placeholder path is drawn too.
 */
let appRows: AppAliasRow[] = [
  { id: "app:photoshop", title: "Adobe Photoshop 2022", subtitle: "C:\\Program Files\\Adobe\\Photoshop.exe", icon: "0000000000000001", origin: "installed", aliases: ["ps"] },
  { id: "app:premiere", title: "Adobe Premiere Pro", subtitle: "C:\\Program Files\\Adobe\\Premiere.exe", icon: "0000000000000002", origin: "installed", aliases: ["prem", "pr"] },
  { id: "app:explorer", title: "File Explorer", subtitle: "C:\\Windows\\explorer.exe", icon: "0000000000000003", origin: "installed", aliases: ["explorer"] },
  { id: "app:firefox", title: "Firefox", subtitle: "C:\\Program Files\\Mozilla Firefox\\firefox.exe", origin: "installed", aliases: [] },
  { id: "app:chrome", title: "Google Chrome", subtitle: "C:\\Program Files\\Google\\Chrome\\chrome.exe", origin: "installed", aliases: ["chrome"] },
  { id: "app:notepad", title: "Notepad", subtitle: "C:\\Windows\\System32\\notepad.exe", origin: "installed", aliases: [] },
  { id: "app:code", title: "Visual Studio Code", subtitle: "C:\\Program Files\\Microsoft VS Code\\Code.exe", origin: "installed", aliases: [] },
  { id: "app:calculator", title: "Calculator", subtitle: "Store app", icon: "0000000000000004", origin: "store", aliases: [] },
  { id: "app:tf2", title: "Team Fortress 2", subtitle: "Steam", origin: "game", aliases: [] },
  // The long tail, and the reason the group is collapsed by default.
  { id: "app:a2ping", title: "a2ping", subtitle: "C:\\TeX\\miktex\\bin\\x64\\a2ping.exe", origin: "commandLine", aliases: [] },
  { id: "app:addr2line", title: "addr2line", subtitle: "C:\\MinGW\\bin\\addr2line.exe", origin: "commandLine", aliases: [] },
  { id: "app:adb", title: "adb", subtitle: "C:\\Users\\you\\AppData\\Local\\Android\\Sdk\\platform-tools\\adb.exe", origin: "commandLine", aliases: [] },
];

/**
 * Stand in for the other window having written a preference.
 *
 * The two windows share `settings.db` and nothing else, so this is what "Settings
 * changed it while the Palette was hidden" looks like from the browser build.
 */
export function setStoredPreference(patch: Partial<SettingsSnapshot>) {
  snapshot = { ...snapshot, ...patch };
}

/**
 * Whether the browser build reports autostart as registered.
 *
 * Seeded from `__takyon_autostart` where a test set one before the page loaded:
 * the switch reads this on mount, so a value set afterwards is a value the
 * mounted switch has already missed.
 *
 * **True by default, because that is what a real install has**: `firstrun::maybe_enable`
 * turns it on and it stopped being a question at v0.6. The OS owns the answer
 * (ADR-0015) and a browser has no OS, so a false default here drew every
 * baseline showing a switch the product ships turned on.
 */
let lastAutostart =
  (globalThis as { __takyon_autostart?: boolean }).__takyon_autostart ?? true;

/**
 * An error the next autostart write should reject with, or null to succeed.
 *
 * The one thing the mock can do that a real machine cannot do on demand: refuse
 * the registry write. tbd v0.1 §3's fix is a `try`/`catch`/`finally` that no
 * other test reaches, and forcing the failure by hand means a group policy.
 */
let autostartFailure: string | null = null;
/** Report autostart as unregistered, which no real first run leaves behind. */
export function setAutostart(on: boolean) {
  lastAutostart = on;
}

export function failAutostart(message: string | null) {
  autostartFailure = message;
}

/** The same, for a `settings.db` write. An unwritable database is the real case. */
let preferenceFailure: string | null = null;
export function failPreferenceWrite(message: string | null) {
  preferenceFailure = message;
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
    // `!c` carries no Entries at all: the answer streams, so the response says
    // only which Agent would answer and what was asked.
    const ask = askQuery(q);
    if (ask !== null) {
      const route = askOrder.filter((kind) => askEnabled[kind]);
      return {
        seq,
        entries: [],
        statusRow: false,
        ask: { query: ask, agent: route[0] ?? null, order: route },
      };
    }
    // `!s` carries no Entries either, and no request is made here: typing the
    // Bang sends nothing, exactly as `query.rs` does it (ADR-0002).
    const web = webQuery(q);
    if (web !== null) {
      return {
        seq,
        entries: [],
        statusRow: false,
        web: {
          query: web,
          provider: "Exa",
          keylessProvider: "DuckDuckGo",
          hasKey: webKey !== null,
        },
      };
    }
    // `!v` is its own view. Unlike a Bangless query, an empty one lists history
    // rather than nothing — the Mode *is* the list.
    const clips = clipQuery(q);
    if (clips !== null) {
      return {
        seq,
        entries: CLIP_FIXTURES.filter((e) => e.title.toLowerCase().includes(clips)),
        statusRow: false,
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
      // The walk reserves no row since v0.9: it reports in Settings and the
      // tray, and `indexing` here only drives those.
      statusRow: false,
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
  /**
   * Ready with a plausible count. The visual suite never walks a disk, so a
   * Building state here would be permanent rather than transient.
   */
  setFilesBangless: async (on: boolean) => {
    snapshot = { ...snapshot, filesBangless: on };
  },
  setFilesFallback: async (on: boolean) => {
    snapshot = { ...snapshot, filesFallback: on };
  },
  setFilesRoots: async (roots: string[], excludes: string[]) => {
    snapshot = { ...snapshot, filesRoots: roots, filesExcludes: excludes };
  },
  openedCount: async () => openedRows,
  clearOpened: async () => {
    const gone = openedRows;
    openedRows = 0;
    return gone;
  },
  fileIndexStatus: async (): Promise<FileIndexReport> => ({
    state: "ready",
    entries: 26844,
    generation: 1,
  }),
  hotkeyStatus: async (): Promise<HotkeyStatus> => ({
    accelerator: liveHotkey,
    registered: hotkeyRegistered,
    ...(hotkeyRegistered
      ? {}
      : { error: "Another application is already holding it." }),
  }),
  reportFirstPixel: async (_showId: number) => {},
  reportFirstEntry: async (_seq: number) => {},
  autostartIsEnabled: async () => lastAutostart,
  autostartSetEnabled: async (on: boolean) => {
    // Refused first, written second — the order the registry uses. A rejected
    // write must leave the reported state untouched, or the test would pass on a
    // mock that lies in the same direction as the bug.
    if (autostartFailure !== null) throw new Error(autostartFailure);
    lastAutostart = on;
  },
  settingsSnapshot: async (): Promise<SettingsSnapshot> => snapshot,
  setReduceMotion: async (on: boolean) => {
    // Same order as autostart: refused first, written second, so a test cannot
    // pass against a mock that lies in the same direction as the bug.
    if (preferenceFailure !== null) throw new Error(preferenceFailure);
    snapshot = { ...snapshot, reduceMotion: on };
  },
  migrateLocalPrefs: async (legacy: Partial<SettingsSnapshot>): Promise<SettingsSnapshot> => {
    // Same rule as Rust's: a value already held wins, so this is idempotent.
    snapshot = { ...legacy, ...snapshot };
    return snapshot;
  },
  setRecents: async (on: boolean) => {
    snapshot = { ...snapshot, recents: on };
  },
  setTray: async (on: boolean) => {
    // Mirrors the Rust rule: the tray cannot be hidden while the hotkey is dead.
    if (!on && !hotkeyRegistered) {
      throw new Error(
        "The tray icon is the only way in while the hotkey is unregistered. Rebind the hotkey first.",
      );
    }
    snapshot = { ...snapshot, tray: on };
  },
  setPlacement: async (value: SettingsSnapshot["placement"]) => {
    snapshot = { ...snapshot, placement: value };
  },
  hotkeyChoices: async () => [
    "Alt+Space",
    "Ctrl+Space",
    "Alt+Shift+Space",
    "Ctrl+Shift+Space",
    "Ctrl+Alt+Space",
    "Ctrl+Shift+P",
  ],
  setHotkey: async (accelerator: string): Promise<HotkeyStatus> => {
    // One chord stands in for "already held by something else", so the refusal
    // path has a way to be exercised without a second application.
    if (accelerator === "Alt+Space") {
      return {
        accelerator: liveHotkey,
        registered: true,
        error: "Another application is already holding it. Kept " + liveHotkey + ".",
      };
    }
    liveHotkey = accelerator;
    return { accelerator, registered: true };
  },
  clipBlocklist: async () => [...blocked],
  setClipBlocked: async (exe: string, block: boolean) => {
    const name = exe.trim().toLowerCase();
    if (!name) throw new Error("an executable name is required");
    blocked = block
      ? [...new Set([...blocked, name])]
      : blocked.filter((e) => e !== name);
    return [...blocked];
  },
  aliases: async () => [...aliasRows],
  applicationRows: async (): Promise<AppAliasRow[]> => appRows,
  setAliasesFor: async (target: string, next: string[]) => {
    const wanted = next.map((a) => a.trim().toLowerCase()).filter(Boolean);
    appRows = appRows.map((row) =>
      row.id === target ? { ...row, aliases: wanted } : row,
    );
  },
  setAlias: async (alias: string, target: string | null) => {
    const name = alias.trim();
    if (!name) throw new Error("an alias needs a name");
    aliasRows =
      target === null
        ? aliasRows.filter((r) => r.alias !== name)
        : [...aliasRows.filter((r) => r.alias !== name), { alias: name, target }];
    aliasRows.sort((a, b) => a.alias.localeCompare(b.alias));
  },
  setTheme: async (value: SettingsSnapshot["theme"]) => {
    snapshot = { ...snapshot, theme: value };
  },
  setUiSize: async (value: SettingsSnapshot["uiSize"]) => {
    snapshot = { ...snapshot, uiSize: value };
  },
  // The browser build has no window to drive, so these record intent and let the
  // title bar render and be asserted on like any other control.
  windowMinimize: async () => {},
  windowToggleMaximize: async () => {
    maximized = !maximized;
  },
  windowIsMaximized: async () => maximized,
  windowClose: async () => {},
  onWindowResized: (_cb: () => void) => () => {},
  openCrashLogs: async () => {},
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
  appsIndexing: async () => indexing,
  agentSnapshots: async (): Promise<AgentSnapshot[]> => agentFixtures,
  agentSettings: async (): Promise<AgentSettings> => ({
    order: [...askOrder],
    enabled: { ...askEnabled },
    cwd: askCwd,
    scratch: String.raw`C:\Users\you\AppData\Local\v3sper\takyon\scratch`,
    models: { ...askModels },
    efforts: { ...askEfforts },
  }),
  agentModels: async (agent: AgentKind) => [...AGENT_MODELS[agent]],
  setAskOrder: async (order: AgentKind[]) => {
    askOrder = [...order];
  },
  setAskEnabled: async (agent: AgentKind, enabled: boolean) => {
    askEnabled = { ...askEnabled, [agent]: enabled };
  },
  setAskCwd: async (path: string) => {
    askCwd = path.trim();
  },
  setAskModel: async (agent: AgentKind, model: string) => {
    askModels = { ...askModels, [agent]: model.trim() };
  },
  setAskEffort: async (agent: AgentKind, effort: string) => {
    askEfforts = { ...askEfforts, [agent]: effort.trim() };
  },
  agentAsk: async (args: { agent: AgentKind; prompt: string; session?: string }) => {
    const turnId = nextTurnId++;
    const session = args.session ?? `mock-session-${turnId}`;
    // Three ticks rather than one: a single event would never catch a renderer
    // that overwrites the answer instead of appending to it.
    emitTurn({ turnId, kind: "started", session, model: "mock-model" });
    for (const [i, delta] of MOCK_ANSWER.entries()) {
      setTimeout(() => emitTurn({ turnId, kind: "text", delta }), 10 * (i + 1));
    }
    setTimeout(
      () => emitTurn({ turnId, kind: "done", session }),
      10 * (MOCK_ANSWER.length + 1),
    );
    return turnId;
  },
  agentCancel: async (_turnId: number) => {},
  webSettings: async (): Promise<WebSettings> => ({
    provider: "Exa",
    keylessProvider: "DuckDuckGo",
    hasKey: webKey !== null,
    hint: webKey ? `…${webKey.slice(-4)}` : undefined,
    signupUrl: "https://dashboard.exa.ai/api-keys",
  }),
  setWebKey: async (keyValue: string) => {
    webKey = keyValue.trim() === "" ? null : keyValue.trim();
  },
  webSearch: async (query: string) => {
    const searchId = nextSearchId++;
    if (webFailure !== null) {
      setTimeout(() => emitSearch({ searchId, kind: "failed", message: webFailure! }), 10);
      return searchId;
    }
    if (webKey === null) {
      setTimeout(
        () =>
          emitSearch({
            searchId,
            kind: "failed",
            message: "No Exa key. Add one in Settings → Web search.",
          }),
        10,
      );
      return searchId;
    }
    // The same three phases Rust emits, in order and on separate ticks: a
    // renderer that skips straight to the answer would pass a single-event mock.
    emitSearch({ searchId, kind: "searching", provider: webKey !== null ? "Exa" : "DuckDuckGo" });
    const turnId = nextTurnId++;
    setTimeout(() => emitSearch({ searchId, kind: "reading", sources: SEARCH_HITS }), 10);
    const answering = () => {
      emitSearch({ searchId, kind: "answering", turnId, agent: "Claude Code" });
      emitTurn({ turnId, kind: "started", session: `mock-search-${turnId}` });
      for (const [i, delta] of MOCK_SYNTHESIS.entries()) {
        setTimeout(() => emitTurn({ turnId, kind: "text", delta }), 10 * (i + 1));
      }
      setTimeout(() => emitTurn({ turnId, kind: "done" }), 10 * (MOCK_SYNTHESIS.length + 1));
    };
    if (holdReading) {
      releaseReading = answering;
    } else {
      setTimeout(answering, 20);
    }
    void query;
    return searchId;
  },
  webCancel: async (_searchId: number) => {},
  openUrl: async (url: string) => {
    opened.push(url);
  },
  openWebQuery: async (query: string) => {
    opened.push(`search:${query}`);
  },
  onSearch: (cb: (message: SearchMessage) => void) => {
    searchListeners.add(cb);
    return () => {
      searchListeners.delete(cb);
    };
  },
  onTurn: (cb: (message: TurnMessage) => void) => {
    turnListeners.add(cb);
    return () => {
      turnListeners.delete(cb);
    };
  },
};
