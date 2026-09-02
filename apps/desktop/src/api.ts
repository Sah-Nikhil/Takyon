/**
 * THE seam: the only file that talks to Tauri (ADR-0009). An ESLint rule enforces
 * it rather than trusting review.
 *
 * It buys two things. The UI runs outside Tauri, falling back to `api.mock.ts`,
 * which is what makes TBC-0007's visual layer possible at all. And every command
 * the frontend can issue sits in one reviewable place, which is how ADR-0002's
 * "no network on the Bangless path" stays checkable by reading.
 */

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  isEnabled as autostartIsEnabledPlugin,
  enable as autostartEnablePlugin,
  disable as autostartDisablePlugin,
} from "@tauri-apps/plugin-autostart";
import {
  EVENT_HIDE,
  EVENT_SHOW,
  type Action,
  type CalcPolicy,
  type HotkeyStatus,
  type QueryResult,
  type ShowPayload,
} from "@takyon/shared";
import { mock } from "./api.mock";

/**
 * Runtime detection rather than a build-time alias. A build flag would mean the
 * browser build and the Tauri build are different bundles, so a visual test could
 * pass against code that never ships.
 */
export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const dismiss = () => (inTauri ? invoke<void>("dismiss") : mock.dismiss());

export const openSettings = () =>
  inTauri ? invoke<void>("open_settings") : mock.openSettings();

export const hotkeyStatus = () =>
  inTauri ? invoke<HotkeyStatus>("hotkey_status") : mock.hotkeyStatus();

/**
 * One keystroke, one `invoke` — never one per Source (ADR-0009).
 *
 * `seq` is monotonic and echoed back. **Discard any response whose `seq` is lower
 * than the newest seen**, or a slow keystroke's results overwrite a fast one's
 * and the Palette answers a prefix of what is now typed.
 */
export const query = (q: string, seq: number) =>
  inTauri ? invoke<QueryResult>("query", { q, seq }) : mock.query(q, seq);

/**
 * Tell Rust the Entries for `seq` have been painted.
 *
 * §10's "hotkey to first Entry" budget, measurable from v0.2 because that is
 * when a Source exists to produce one. Rust holds both timestamps, as with
 * `reportFirstPixel`.
 */
export const reportFirstEntry = (seq: number) =>
  inTauri ? invoke<void>("report_first_entry", { seq }) : mock.reportFirstEntry(seq);

/**
 * Tell Rust when the calculator may answer (v0.4).
 *
 * Pushed rather than read: the rule is enforced inside the Source on the
 * keystroke path, so Rust has to hold it. `prefs.ts` remembers the choice, and
 * both windows push, so they cannot disagree.
 */
export const setCalcPolicy = (policy: CalcPolicy) =>
  inTauri ? invoke<void>("set_calc_policy", { policy }) : mock.setCalcPolicy(policy);

/**
 * Every action id and its label, for the footer (v0.4.5).
 *
 * Fetched once on mount. Labels live in Rust (ADR-0009), and the alternative is
 * an `invoke` on every arrow key.
 */
export const actionLabels = () =>
  inTauri ? invoke<Action[]>("action_labels") : mock.actionLabels();

/** The `Ctrl+K` menu for one Entry. Labels and accelerators live in Rust. */
export const actionsFor = (entryId: string) =>
  inTauri ? invoke<Action[]>("actions_for", { entryId }) : mock.actionsFor(entryId);

/**
 * Tell the window the action menu opened (`n` actions) or closed (`null`).
 *
 * The *native window* is too short for it, and nothing inside the webview can
 * make room. Exactly the class of bug the mocked visual layer cannot see
 * (TBC-0007); a Rust unit test covers the arithmetic instead.
 */
export const setActionMenu = (actions: number | null) =>
  inTauri
    ? invoke<void>("set_action_menu", { actions })
    : mock.setActionMenu(actions);

/**
 * Report the measured height of the hotkey-failure banner, or 0 if absent.
 *
 * Rust knows whether it is drawn, not how tall it came out — wrapping text, so
 * the layout engine decides. A constant on the Rust side was 16px short at 150%
 * and clipped the list's last row. The side that laid it out reports it.
 */
export const setBannerHeight = (height: number) =>
  inTauri
    ? invoke<void>("set_banner_height", { height: Math.ceil(height) })
    : mock.setBannerHeight(height);

/**
 * Perform an action on an Entry.
 *
 * Rust hides the Palette *before* launching (v0.2 task 7): `ShellExecuteW`
 * returns when the shell accepts the request, not when a window exists.
 */
export const activate = (entryId: string, actionId: string) =>
  inTauri
    ? invoke<void>("activate", { entryId, actionId })
    : mock.activate(entryId, actionId);

/**
 * The URL for an Entry's icon.
 *
 * The response carries an opaque key, not bytes. The key contains the source's
 * mtime, so the bytes can never change and it is cached immutably. Outside Tauri
 * it yields "", and the row keeps its placeholder.
 */
export const iconUrl = (key: string | undefined): string => {
  if (!key) return "";
  return inTauri ? convertFileSrc(key, "takyon-icon") : mock.iconUrl(key);
};

/**
 * Tell Rust this show's frame has been presented.
 *
 * Rust holds both timestamps, so the two clocks never have to be reconciled —
 * the usual way a latency number becomes fiction. The span includes one IPC hop
 * and excludes DWM's final present; `docs/tbc/0002` states both.
 */
export const reportFirstPixel = (showId: number) =>
  inTauri ? invoke<void>("report_first_pixel", { showId }) : mock.reportFirstPixel(showId);

/**
 * Autostart reads straight from the plugin and is deliberately NOT mirrored into
 * any settings store: Task Manager → Startup apps flips this behind the app's
 * back with no event to observe, so a cached copy would confidently display the
 * wrong state (ADR-0015).
 */
export const autostartIsEnabled = () =>
  inTauri ? autostartIsEnabledPlugin() : mock.autostartIsEnabled();

export const autostartSetEnabled = (on: boolean) => {
  if (!inTauri) return mock.autostartSetEnabled(on);
  return on ? autostartEnablePlugin() : autostartDisablePlugin();
};

/** Subscribe to the Palette being shown. Returns an unsubscribe function. */
export function onShow(cb: (payload: ShowPayload) => void): () => void {
  if (!inTauri) return mock.onShow(cb);
  const p = listen<ShowPayload>(EVENT_SHOW, (e) => cb(e.payload));
  return () => {
    void p.then((un) => un());
  };
}

export function onHide(cb: () => void): () => void {
  if (!inTauri) return mock.onHide(cb);
  const p = listen(EVENT_HIDE, () => cb());
  return () => {
    void p.then((un) => un());
  };
}
