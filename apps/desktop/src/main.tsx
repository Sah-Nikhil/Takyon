/**
 * One bundle, two windows. Which one this root is depends on `?window=` in the
 * URL, set by the Tauri window definitions in `tauri.conf.json`. A second HTML
 * entry point would mean a second Vite build and a second cold start; the Palette
 * is warm precisely so nothing pays a cold start.
 */

import React from "react";
import ReactDOM from "react-dom/client";
import { Palette } from "./palette/Palette";
import { Settings } from "./settings/Settings";
import { inTauri } from "./api";
import { emitHide, emitShow } from "./api.mock";
import { applyMotionPreference } from "./prefs";
import "./styles.css";

const kind = new URLSearchParams(window.location.search).get("window") ?? "palette";

// Before the first render, not in an effect. Applied afterwards, a Palette shown
// on the very first summon would play one frame of an animation the user has
// switched off.
applyMotionPreference();

// Outside Tauri there is no hotkey to press, so the show event would never fire
// and the Palette would sit unfocused forever. Playwright and the browser console
// drive it through here instead. Never exposed in the real app.
if (!inTauri) {
  (window as unknown as Record<string, unknown>).__takyon_mock = { emitShow, emitHide };
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{kind === "settings" ? <Settings /> : <Palette />}</React.StrictMode>,
);
