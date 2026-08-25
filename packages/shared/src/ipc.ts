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
 * What an Entry is. Decides its icon fallback and, in Rust, where it sorts: apps
 * always rank above documents (§3).
 *
 * Only `app` is produced at v0.2. The rest are declared so adding one later is a
 * compile error at every site that has to care.
 */
export type EntryKind = "app" | "file" | "folder" | "clip" | "calc" | "recent";

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
   * True while the first application walk is still running. The Palette says so
   * rather than drawing an empty list — an empty list means "no such app", which
   * in the first second after login is exactly wrong.
   */
  indexing: boolean;
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
): number {
  const visible = rows === 0 && indexing ? 1 : Math.min(rows, MAX_VISIBLE_ROWS);
  const content =
    visible === 0 ? EMPTY_HEIGHT : EMPTY_HEIGHT + visible * ROW_HEIGHT + LIST_CHROME;
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
