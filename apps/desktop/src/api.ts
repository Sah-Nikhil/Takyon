/**
 * THE seam. This is the only file in the app that talks to Tauri (ADR-0009), and
 * an ESLint rule in `eslint.config.js` enforces it rather than trusting review.
 *
 * It buys two things that are worth a file of one-line wrappers:
 *
 * 1. The UI runs outside Tauri. Every export falls back to `api.mock.ts` when
 *    `__TAURI_INTERNALS__` is absent, so `bun --cwd apps/desktop run dev` opens a
 *    working Palette in an ordinary browser — which is what makes deterministic
 *    visual regression testing possible at all (TBC-0007).
 * 2. Every command the frontend can issue is visible in one reviewable place,
 *    which is how the ADR-0002 "no network on the Bangless path" guarantee stays
 *    checkable by reading rather than by trusting.
 *
 * The cost is one JIT-inlined call. The expensive part — serialising across the
 * WebView2↔Rust boundary — happens identically either way.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  isEnabled as autostartIsEnabledPlugin,
  enable as autostartEnablePlugin,
  disable as autostartDisablePlugin,
} from "@tauri-apps/plugin-autostart";
import { EVENT_HIDE, EVENT_SHOW, type HotkeyStatus, type ShowPayload } from "@takyon/shared";
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
 * Tell Rust this show's frame has been presented. Rust holds *both* timestamps —
 * it stamped the hotkey and it stamps this call — so the two clocks never have to
 * be reconciled, which is the usual way a latency number quietly becomes fiction.
 *
 * The measured span therefore includes one IPC hop and excludes DWM's final
 * present. Both are stated in `docs/tbc/0002` rather than pretended away.
 */
export const reportFirstPixel = (showId: number) =>
  inTauri ? invoke<void>("report_first_pixel", { showId }) : mock.reportFirstPixel(showId);

/**
 * Autostart reads straight from the plugin and is deliberately NOT mirrored into
 * any settings store: Task Manager → Startup apps flips this behind the app's
 * back with no event to observe, so a cached copy would confidently display the
 * wrong state (tesseract ADR-0026).
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
