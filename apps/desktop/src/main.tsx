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
    failPreferenceWrite,
    // What "Settings wrote a preference while the Palette was hidden" looks like
    // when there is no settings.db to write to.
    setStoredPreference,
  };
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ErrorBoundary>{kind === "settings" ? <Settings /> : <Palette />}</ErrorBoundary>
  </React.StrictMode>,
);
