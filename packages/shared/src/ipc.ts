/**
 * The IPC contract, mirroring the Rust structs on the other side of the seam.
 *
 * `IMPLEMENTATION_PLAN.md` §8 specifies the full V1 contract — `query`,
 * `activate`, `actions_for`, `index_status`. Only the commands v0.1 actually
 * implements are declared here. Declaring the rest ahead of time would give the
 * contract tests (TBC-0007's answer to fixture drift) nothing to check against,
 * and a type with no Rust behind it is a fixture that can never drift *into*
 * correctness.
 *
 * These types are `camelCase` because the Rust side serialises with
 * `#[serde(rename_all = "camelCase")]`. If a field here is `snake_case`, either
 * this file or that attribute is wrong.
 */

/**
 * The product name, for UI copy only (ADR-0020).
 *
 * **Never key anything off this.** Windows keys off the slug `com.v3sper.takyon`,
 * which reads alike but is a separate literal. A Rust test asserts this string
 * matches `identity::DISPLAY_NAME`.
 */
export const DISPLAY_NAME = "Takyon";

/** Which window a React root is being mounted into. Chosen by `?window=` on the URL. */
export type WindowKind = "palette" | "settings";

/**
 * Why the Palette became visible. The Palette always opens empty (ROADMAP v0.1),
 * so this carries no query — it exists so the bench harness can tell a measured
 * show from an incidental one, and so a debug show can skip the focus rules.
 */
export interface ShowPayload {
  /**
   * Monotonic id for this show, minted in Rust. The frontend echoes it back via
   * `reportFirstPixel` once the frame has actually been presented; Rust owns both
   * timestamps so the two clocks never have to be reconciled.
   */
  showId: number;
  /**
   * True when the window was shown by the debug no-steal-focus path
   * (`TAKYON_NO_FOCUS_STEAL=1`). Dismiss-on-focus-loss is suppressed for these,
   * or inspecting the Palette in devtools destroys it every time.
   */
  noFocusSteal: boolean;
}

/** Whether the global hotkey is live, and if not, why not. */
export interface HotkeyStatus {
  /** The binding as accelerator text, e.g. `Alt+Space`. */
  accelerator: string;
  registered: boolean;
  /**
   * Present exactly when `registered` is false. A taken hotkey must be reported,
   * never silently swallowed (IMPLEMENTATION_PLAN §7) — this is the string the
   * user is shown.
   */
  error?: string;
}

/**
 * What the file index can currently promise (§5 task 7).
 *
 * `stale` means events were dropped and a rescan is running, so results may be
 * missing. It must reach the user: an index that quietly misses files teaches
 * them not to trust the feature, which is worse than not having it (ADR-0007).
 */
export type FileIndexState = "ready" | "building" | "stale";

/** The file index's state, plus the numbers TBC-0005's triggers are stated in. */
export interface FileIndexReport {
  state: FileIndexState;
  /** Present only while `state` is `building`. A progress row, not a promise. */
  pct?: number;
  /** Entries in the mapped file. Settings shows this live. */
  entries: number;
  /** Bumped by every rescan, so two results can be told apart. */
  generation: number;
}

/**
 * What an Entry is. Decides its icon fallback and, in Rust, where it sorts: apps
 * always rank above documents (§3).
 *
 * Only `app` is produced at v0.2. The rest are declared so adding one later is a
 * compile error at every site that has to care.
 */
export type EntryKind =
  | "app"
  | "file"
  | "folder"
  | "clip"
  | "calc"
  | "recent"
  | "system"
  | "systemTask"
  | "command";

/**
 * When the calculator is allowed to answer (v0.4).
 *
 * `automatic` matches Raycast: any unambiguous expression answers Bangless.
 * `explicit` answers only input starting with `=`. The spellings are the wire
 * format `sources/calc` parses, so renaming one breaks a saved setting.
 */
export type CalcPolicy = "automatic" | "explicit";

/**
 * Every stored preference a window reads on mount (v0.6).
 *
 * One response rather than one `invoke` per control, because the Palette reads
 * it too and reads it on the startup path. **Autostart is deliberately absent**:
 * the OS owns that answer and it is re-read every mount (ADR-0015).
 */
export interface SettingsSnapshot {
  reduceMotion: boolean;
  calcPolicy: CalcPolicy;
  recents: boolean;
  tray: boolean;
  placement: Placement;
  clipRetention: ClipRetention;
  clipBang: boolean;
  theme: Theme;
  uiSize: UiSize;
  /**
   * Whether file Entries join Bangless results (v0.7 task 11). Default off —
   * `!e` is the door, this is the setting, and when on they sort below apps.
   */
  filesBangless: boolean;
  /**
   * Whether Windows Search answers for locations outside the indexed roots.
   * Default off: its coverage cannot be relied on and its queries cost 10-72 ms
   * against a 20 ms budget (TBC-0005).
   */
  filesFallback: boolean;
  /** Indexed roots, and the names skipped inside them (TBC-0005). */
  filesRoots: string[];
  filesExcludes: string[];
}

/**
 * Appearance (v0.6). `system` follows Windows; the other two override it in
 * both directions, which is what makes it an override rather than a hint.
 */
export type Theme = "system" | "light" | "dark";

/**
 * Interface size (v0.6). Applied as a root `zoom`, and **mirrored in Rust**,
 * which scales the Palette's window height by the same percentages. If the two
 * disagree the Palette is exactly the difference too short.
 */
export type UiSize = "small" | "default" | "large";

/**
 * Which monitor the Palette opens on (v0.6).
 *
 * Two pinned choices rather than a monitor list: a saved monitor index is wrong
 * the moment a display is unplugged, and it is wrong silently.
 */
export type Placement = "cursor" | "primary";

/**
 * Where an application was discovered (v0.7).
 *
 * The list groups on it. `commandLine` is the long tail — `a2ping`, `addr2line`,
 * `agentactivationruntimestarter` — real and launchable and never what anyone is
 * scrolling for, which is why it is collapsed by default.
 */
export type AppOrigin = "installed" | "store" | "game" | "commandLine";

/**
 * One application as the Applications page lists it (v0.7).
 *
 * Keyed by application rather than by alias, unlike `AliasRow`: an alias is
 * created *on* an application, so the list has to show the ones without any.
 */
export interface AppAliasRow {
  /** The Entry id. Opaque; the UI passes it back and never parses it. */
  id: string;
  title: string;
  /** Path or store, shown to tell two same-named applications apart. */
  subtitle?: string;
  /**
   * Icon key for `takyon-icon://`, absent where the shell had none. The row
   * draws an initial instead and never waits for one (§6).
   */
  icon?: string;
  /** Which discovery path found it, so the list can group rather than sprawl. */
  origin: AppOrigin;
  /** Every alias pointing here. Usually zero or one; the store allows more. */
  aliases: string[];
}

/** One alias and what it points at, for the Applications page (v0.6). */
export interface AliasRow {
  alias: string;
  /** The target Entry's id. Opaque; the UI never parses it. */
  target: string;
  /**
   * The target's title today. **Absent when the alias outlived its
   * application** — an uninstall, or a rename. The row still lists, so it can
   * be deleted rather than becoming an invisible rule.
   */
  title?: string;
}

/**
 * How long clipboard history is kept (v0.5, ADR-0006).
 *
 * A fixed list, not a duration: expiry **deletes** rather than hides. The
 * spellings are the wire format `clips::Retention` parses, so renaming one
 * resets a saved choice back to the default.
 */
export type ClipRetention = "forever" | "6-months" | "1-month" | "1-week" | "1-day";

/**
 * A full-window surface the Palette navigates into (v0.5).
 *
 * Not a second window: a third WebView2 would cost the login budget and a large
 * share of the 150 MB ceiling. The warm Palette grows, and Escape goes back.
 */
export type ViewKind = "clipboard-history" | "ask" | "web";

/**
 * How tall a full-window View is, logical pixels. Mirrors `VIEW_HEIGHT` in
 * `window.rs`; a Rust test asserts they agree.
 */
export const VIEW_HEIGHT = 560;

/** One clipboard row as the history surface draws it. */
export interface ClipRow {
  id: number;
  /** Unix seconds. The surface groups by day from this. */
  createdAt: number;
  kind: "text";
  /**
   * The executable that owned the clipboard, or the foreground window when
   * Windows reports no owner. Plaintext, and a known metadata leak (ADR-0008).
   */
  sourceExe?: string;
  /** Characters of the full content, not of the preview. */
  len: number;
  /**
   * One line, capped at 160 characters. **The full content never travels with a
   * list** — a search response would otherwise ship every matching secret into
   * the webview.
   */
  preview: string;
}

/** A single actionable row in the Palette (CONTEXT.md: Entry, never "result"). */
export interface Entry {
  /**
   * Stable across restarts, and the Frecency key from v0.3. Derived from the
   * resolved target path, never from the display name — see `entry.rs`.
   */
  id: string;
  title: string;
  subtitle?: string;
  kind: EntryKind;
  /**
   * An opaque key into the icon blob, **not** a path and not bytes. `api.ts`
   * turns it into a `takyon-icon://` URL for the browser to fetch; the bytes
   * never travel in this response, because doing so would put every icon through
   * the IPC serialiser on every keystroke (`icons.rs`).
   */
  icon?: string;
  score: number;
  /** Ids only. The labels live in Rust's `actions.rs` and arrive via `actionsFor`. */
  actions: string[];
  /**
   * Shown beside the title, and only where two same-named executables disagree
   * about theirs — two Node installs, two R installs. Absent on almost every
   * row: `version.rs` explains why it is not read for everything.
   */
  version?: string;
}

/** One row of the `Ctrl+K` action menu. */
export interface Action {
  id: string;
  label: string;
  /**
   * Accelerator text such as `Ctrl+Enter`, shown in the menu so the shortcut is
   * discoverable rather than folklore (ROADMAP v0.2).
   */
  accelerator?: string;
}

/** What one keystroke gets back. */
export interface QueryResult {
  /**
   * Echoed from the request. **Discard any response whose `seq` is lower than the
   * newest already seen** (§3): without that, a slow keystroke's results overwrite
   * a fast one's and the Palette shows answers for a prefix of what is now in the
   * input.
   */
  seq: number;
  entries: Entry[];
  /**
   * Reserve one row in the window for a status line.
   *
   * **Not "the application walk is running".** It meant both until v0.9, and
   * `!s` inherited the second meaning: two rows rendered in a window sized for
   * one, so the list scrolled and the scrollbar covered the message. The walk
   * reports in Settings and the tray now.
   */
  statusRow: boolean;
  /**
   * Present exactly when the line is `!c` (v0.8). The Ask Mode has no Entries to
   * rank — the answer streams over `EVENT_TURN` — so `entries` stays empty and
   * this carries the question plus which Agent would answer it.
   */
  ask?: Ask;
  /**
   * Present exactly when the line is `!s` (v0.9). Same shape and same reason as
   * `ask`: the answer streams, so there is nothing to rank.
   */
  web?: Web;
}

/** The `!s` Mode's state for one keystroke. */
export interface Web {
  /** The question, trimmed. Empty means the Bang alone was typed. */
  query: string;
  /** Which service answers. One today (ADR-0005), named so the row can say it. */
  provider: string;
  /**
   * Whether a key is stored. False is a state with its own copy, not an error:
   * the fix is Settings → Web search, and the row says so.
   */
  hasKey: boolean;
}

/** The `!c` Mode's state for one keystroke. */
export interface Ask {
  /** The question, trimmed. Empty means the Bang alone was typed. */
  query: string;
  /** Which Agent answers. Null when every Agent is switched off. */
  agent: AgentKind | null;
  /**
   * The switched-on Agents in preference order. No Sign-in state in it: knowing
   * costs three process spawns and this is the keystroke path, so the Palette
   * refines the choice once its own probe lands.
   */
  order: AgentKind[];
}

/**
 * Row geometry, logical pixels.
 *
 * Shared because Rust sizes the window from these and the CSS draws rows with
 * them (TBC-0006). Disagreement clips the last row or leaves a shadowed empty
 * strip. A Rust test asserts the values match.
 */
export const ROW_HEIGHT = 44;
/** Rows visible before the list scrolls instead of the window growing. */
export const MAX_VISIBLE_ROWS = 8;
/** The Palette with nothing in it: gutter, border, input row, and the same below. */
export const EMPTY_HEIGHT = 68;
/**
 * Space the Entry list adds beyond its rows: 8px padding plus a 1px hairline.
 *
 * The border counts — border-box with an explicit height puts padding and border
 * inside it, so reserving only the padding grows a scrollbar on a list that fits.
 */
export const LIST_CHROME = 9;

/**
 * The "Calculator" caption above the card, including the gap under it (v0.4.5).
 */
export const CALC_CAPTION_HEIGHT = 22;
/**
 * The calculator card: expression, arrow, result, and a label under each.
 *
 * A calculation is drawn as this instead of a `ROW_HEIGHT` row, so it is the one
 * Entry whose height is not uniform. `window.rs` mirrors both constants.
 */
export const CALC_CARD_HEIGHT = 116;

/**
 * The footer strip naming what Enter does (v0.4.5 task 4).
 *
 * Drawn only when the list is, so it is added in the same branch. Raycast shows
 * no footer over an empty Palette either — there is no selected row to describe.
 */
export const FOOTER_HEIGHT = 34;

/**
 * One row of the `Ctrl+K` menu, and the chrome around its list.
 *
 * **Measured from the rendered menu, not chosen.** A Playwright test measures the
 * real menu and asserts these still describe it.
 */
export const ACTION_ROW_HEIGHT = 33;
export const MENU_CHROME = 51;
export const MENU_MARGIN = 16;

/**
 * The banner's 8px top margin, which a bounding box excludes.
 *
 * Its height is **measured at runtime**, never a constant: wrapping text, so the
 * layout engine decides from font, DPI and width. A constant measured at 100%
 * was 16px short at 150% and the flex column clipped the list's last row.
 */
export const BANNER_MARGIN = 8;

/**
 * How tall the Palette must be to hold an `actions`-row menu.
 *
 * The menu overlays the Palette but is taller than a one-row one, so opening it
 * has to grow the window or the last actions are cut off.
 */
export function menuHeight(actions: number): number {
  return MENU_CHROME + actions * ACTION_ROW_HEIGHT + MENU_MARGIN;
}

/**
 * How tall the Palette is, for rows plus an optional open menu and banner
 * (TBC-0006).
 *
 * Mirrors Rust's `window::content_height`; a Rust test asserts the constants
 * above agree.
 */
export function paletteHeight(
  rows: number,
  indexing = false,
  menuActions: number | null = null,
  bannerHeight = 0,
  calcCard = false,
): number {
  // The card replaces a row rather than joining it, and the cap applies to what
  // is left: eight rows *plus* a card is taller than the shape TBC-0006 chose.
  const card = calcCard ? CALC_CAPTION_HEIGHT + CALC_CARD_HEIGHT : 0;
  const listRows =
    rows === 0 && indexing
      ? 1
      : Math.min(Math.max(rows - (calcCard ? 1 : 0), 0), MAX_VISIBLE_ROWS);
  const content =
    card === 0 && listRows === 0
      ? EMPTY_HEIGHT
      : EMPTY_HEIGHT + card + listRows * ROW_HEIGHT + LIST_CHROME + FOOTER_HEIGHT;
  // `max`, never a sum: the menu sits on top of the list rather than below it, so
  // a tall list already has the room and only a short one has to grow.
  const withMenu =
    menuActions === null ? content : Math.max(content, menuHeight(menuActions));
  // The banner, by contrast, *is* a sum: it sits below everything else rather
  // than over it, so it always needs its own space.
  return bannerHeight > 0 ? withMenu + bannerHeight + BANNER_MARGIN : withMenu;
}

/** Event names Rust emits. String constants so a rename is a compile error on both sides. */
export const EVENT_SHOW = "takyon://show";
export const EVENT_HIDE = "takyon://hide";
/** Every Turn streams over this one channel; `turnId` says which. */
export const EVENT_TURN = "takyon://turn";

// ── Agents (v0.8) ───────────────────────────────────────────────────

/**
 * Which Agent. The spellings are stored preferences on the Rust side, so
 * renaming one breaks a saved setting rather than only a type.
 */
export type AgentKind = "claude" | "codex" | "opencode";

/**
 * What Takyon can say about an Agent's credentials (`CONTEXT.md` §Agents).
 *
 * `unknown` is not a failure: installed but would not answer, which is a
 * different sentence from `out` and must stay one (ADR-0017).
 */
export type SignInStatus = "in" | "out" | "unknown";

export interface SignIn {
  status: SignInStatus;
  /** The Agent's own words — "Claude Pro Subscription", "2 providers connected". */
  label?: string;
  /** The account the Agent named, where it names one. */
  account?: string;
}

/** How usable an Agent is right now. T3 Code's states, same meanings. */
export type AgentHealth = "ready" | "warning" | "error";

/**
 * Everything a Settings card and the `!c` empty state need.
 *
 * Facts only. The headline is assembled in `agents/status.ts`, the way T3 Code's
 * `providerStatus.ts` does it.
 */
export interface AgentSnapshot {
  kind: AgentKind;
  label: string;
  /** The command the user would type, and the "not found" sentence's subject. */
  binary: string;
  installed: boolean;
  version?: string;
  health: AgentHealth;
  signIn: SignIn;
  /** The Agent's own sentence, carrying the command to run when signed out. */
  message?: string;
  /**
   * Effort levels this Agent accepts, weakest first. Each CLI spells effort
   * differently, so the list is the Agent's own vocabulary rather than ours.
   */
  efforts: string[];
}

/** Agent preferences, in one response for the same reason as `SettingsSnapshot`. */
export interface AgentSettings {
  /** The preference order, first to last. Every Agent appears once. */
  order: AgentKind[];
  /** Which Agents are switched on. `!c` walks the order and skips the rest. */
  enabled: Partial<Record<AgentKind, boolean>>;
  /** Empty means the Scratch directory below, never the process cwd. */
  cwd: string;
  scratch: string;
  /**
   * The locked-in model per Agent. Chosen in Settings, used for **every** Turn,
   * and there is no per-query override anywhere. Absent is the Agent's default.
   */
  models: Partial<Record<AgentKind, string>>;
  /** The locked-in effort level per Agent. Same rule as the model. */
  efforts: Partial<Record<AgentKind, string>>;
}

/** One thing that happened during a Turn, as Rust tags it. */
export type TurnEvent =
  | { kind: "started"; session?: string; model?: string }
  | { kind: "text"; delta: string }
  | { kind: "done"; session?: string }
  | { kind: "failed"; message: string };

/** A `TurnEvent` with the Turn it belongs to. Rust flattens the two together. */
export type TurnMessage = TurnEvent & { turnId: number };

// ── Web search (v0.9) ───────────────────────────────────────────────

/** The channel `!s` reports progress on. The answer itself is a Turn. */
export const EVENT_SEARCH = "takyon://search";

/** One search result, before its page has been read. */
export interface SearchHit {
  title: string;
  url: string;
  /** The provider's own snippet, with its highlight markup stripped. */
  description: string;
}

/**
 * One thing that happened during a search.
 *
 * `answering` carries a `turnId`: the answer arrives on `EVENT_TURN` like any
 * other Turn, so streaming and cancellation are not written twice.
 */
export type SearchEvent =
  | { kind: "searching"; provider: string }
  | { kind: "reading"; sources: SearchHit[] }
  | { kind: "answering"; turnId: number; agent: string }
  | { kind: "failed"; message: string };

/** A `SearchEvent` with the search it belongs to. Rust flattens the two. */
export type SearchMessage = SearchEvent & { searchId: number };

/** What Settings shows for web search. The key itself never crosses IPC. */
export interface WebSettings {
  provider: string;
  hasKey: boolean;
  /** Last four characters of the stored key, so a wrong paste is visible. */
  hint?: string;
  /** Where a key comes from. In the response so provider and page move together. */
  signupUrl: string;
}
