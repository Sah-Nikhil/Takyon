/**
 * One bundle, two windows. Which one this root is depends on `?window=` in the
 * URL, set by the Tauri window definitions in `tauri.conf.json`. A second HTML
 * entry point would mean a second Vite build and a second cold start; the Palette
 * is warm precisely so nothing pays a cold start.
 */

import React from "react";
import ReactDOM from "react-dom/client";
import { ErrorBoundary } from "./ErrorBoundary";
import { Palette } from "./palette/Palette";
import { Settings } from "./settings/Settings";
import { inTauri } from "./api";
import {
  bannerRequest,
  calcPolicyRequest,
  emitHide,
  emitShow,
  failAutostart,
  failPreferenceWrite,
  menuRequest,
  failWebSearch,
  holdSearchAtReading,
  openedUrls,
  setAnswer,
  setAutostart,
  setAgentSignedOut,
  setAskOrder,
  setWebKeyStored,
  setIndexing,
  setStoredPreference,
} from "./api.mock";
import { load } from "./prefs";
import "./styles.css";

const kind = new URLSearchParams(window.location.search).get("window") ?? "palette";

// Fired before the first render but not awaited: since v0.6 the value lives in
// `settings.db`, so reading it is an `invoke` rather than a synchronous
// `localStorage` hit. The Palette can afford that — it mounts at startup while
// hidden, so the read lands long before any show could paint a frame.
void load();

// Outside Tauri there is no hotkey to press, so the show event would never fire
// and the Palette would sit unfocused forever. Playwright and the browser console
// drive it through here instead. Never exposed in the real app.
// `setIndexing` joins them for the same reason: the window between login and the
// application walk finishing is a few hundred milliseconds long, which no test
// could catch by timing, and it has its own row in the Palette.
if (!inTauri) {
  (window as unknown as Record<string, unknown>).__takyon_mock = {
    emitShow,
    emitHide,
    setIndexing,
    menuRequest,
    bannerRequest,
    calcPolicyRequest,
    // The only way to make the autostart write refuse on demand. On a real
    // machine it takes a group policy or a locked hive (tbd v0.1 §3).
    failAutostart,
    // Autostart reads as on in the browser because that is what a real install
    // has. A test that needs the unregistered state asks for it.
    setAutostart,
    failPreferenceWrite,
    // What "Settings wrote a preference while the Palette was hidden" looks like
    // when there is no settings.db to write to.
    setStoredPreference,
    // The order `!c` tries Agents in. Rust persists it; the mock's copy dies
    // with the page, so a visual test that reordered in Settings would find the
    // default order again after navigating to the Palette.
    setAskOrder,
    // Every Agent signed out is the only state that blocks `!c` now that the
    // preference is an order, and no fixture is in it.
    setAgentSignedOut,
    // Whether `!s` holds a key. Rust keeps it DPAPI-wrapped on disk, which a
    // browser cannot reach, so the no-key state is unreachable without this.
    setWebKeyStored,
    failWebSearch,
    // Hold a search at its reading phase, which otherwise lasts 20ms.
    holdSearchAtReading,
    // What the mock Agent answers, so the renderer can be driven with markdown.
    setAnswer,
    // What the mock was asked to open. Enter on `!s` and a source row both end
    // in the shell, which a browser has no way to observe.
    openedUrls,
  };
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ErrorBoundary>
      {kind === "settings" ? <Settings /> : <Palette />}
    </ErrorBoundary>
  </React.StrictMode>,
);
