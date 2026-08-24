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

import type { HotkeyStatus, ShowPayload } from "@takyon/shared";

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

export const mock = {
  dismiss: async () => {
    emitHide();
  },
  openSettings: async () => {},
  hotkeyStatus: async (): Promise<HotkeyStatus> => ({
    accelerator: "Alt+Space",
    registered: true,
  }),
  reportFirstPixel: async (_showId: number) => {},
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
