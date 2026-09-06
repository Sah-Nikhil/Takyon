/**
 * The preferences a window holds in memory, and how they get there.
 *
 * Through v0.5 this was `localStorage`, because no `settings.db` existed. v0.6
 * moved the storage into Rust (task 8b) and left this module as the cache: read
 * the snapshot once on mount, migrate any legacy key across, and push the
 * preference onto `<html>` where one CSS rule can act on it.
 *
 * The legacy keys are still *read*, once, and then deleted. Dropping them without
 * carrying the value across would silently switch someone's animations back on.
 */

import type { CalcPolicy, SettingsSnapshot } from "@takyon/shared";
import * as api from "@/api";

/**
 * The v0.1 `localStorage` keys. Read once by [`migrate`], then removed.
 *
 * Still spelled `launcher` deliberately: these name entries already written to
 * disk under the pre-ADR-0020 slug. Renaming them here would not rename them in
 * anyone's browser profile, it would just stop finding them.
 */
const LEGACY_MOTION = "com.v3sper.launcher.reduce-motion";
const LEGACY_CALC = "com.v3sper.launcher.calc-policy";

/** Defaults, matching Rust's. A window that cannot reach Rust still behaves. */
let current: SettingsSnapshot = {
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
  filesRoots: [],
  filesExcludes: [],
};

/** Whether Windows itself is asking for less motion. Independent of our switch. */
export function systemReducesMotion(): boolean {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** The preferences as last read. Synchronous, so render paths can use it. */
export function preferences(): SettingsSnapshot {
  return current;
}

export function reduceMotion(): boolean {
  return current.reduceMotion;
}

export function calcPolicy(): CalcPolicy {
  return current.calcPolicy;
}

/**
 * Whatever v0.1 left in `localStorage`, as a partial snapshot.
 *
 * Absent keys stay absent rather than becoming defaults: Rust only writes a key
 * it is actually sent, and sending a default would seed storage with a choice
 * nobody made.
 */
function legacy(): Partial<SettingsSnapshot> {
  const found: Partial<SettingsSnapshot> = {};
  try {
    const motion = window.localStorage.getItem(LEGACY_MOTION);
    if (motion !== null) found.reduceMotion = motion === "true";
    const calc = window.localStorage.getItem(LEGACY_CALC);
    if (calc !== null) found.calcPolicy = calc === "explicit" ? "explicit" : "automatic";
  } catch {
    // A WebView with storage blocked has nothing to migrate, which is fine.
  }
  return found;
}

function forgetLegacy(): void {
  try {
    window.localStorage.removeItem(LEGACY_MOTION);
    window.localStorage.removeItem(LEGACY_CALC);
  } catch {
    // Nothing to do. Rust ignores a key it already holds, so a second migration
    // from a window that could not clear these is harmless.
  }
}

/**
 * Read the stored preferences, carrying any v0.1 key across on the way.
 *
 * Called on every mount. Both windows do it, and Rust keeps the value it already
 * has, so two windows racing cannot undo each other.
 */
export async function load(): Promise<SettingsSnapshot> {
  const stale = legacy();
  current =
    Object.keys(stale).length > 0
      ? await api.migrateLocalPrefs(stale)
      : await api.settingsSnapshot();
  if (Object.keys(stale).length > 0) forgetLegacy();
  applyMotionPreference();
  applyAppearance();
  return current;
}

/** Re-read without migrating. The Palette's per-show sync point. */
export async function refresh(): Promise<SettingsSnapshot> {
  current = await api.settingsSnapshot();
  applyMotionPreference();
  applyAppearance();
  return current;
}

/*
  Both writers reach Rust *first*, then update the cache. Guessing first leaves
  the cache holding the optimistic value after a rejected write, and `useApplied`
  reads it to decide where a control settles. Showing the new state early is the
  caller's job, not storage's.
*/
export async function setReduceMotion(on: boolean): Promise<void> {
  await api.setReduceMotion(on);
  current = { ...current, reduceMotion: on };
  applyMotionPreference();
}

export async function setCalcPolicy(mode: CalcPolicy): Promise<void> {
  await api.setCalcPolicy(mode);
  current = { ...current, calcPolicy: mode };
}

export async function setRecents(on: boolean): Promise<void> {
  await api.setRecents(on);
  current = { ...current, recents: on };
}

export async function setTray(on: boolean): Promise<void> {
  await api.setTray(on);
  current = { ...current, tray: on };
}

export async function setPlacement(value: SettingsSnapshot["placement"]): Promise<void> {
  await api.setPlacement(value);
  current = { ...current, placement: value };
}

export async function setClipBang(on: boolean): Promise<void> {
  await api.setClipBang(on);
  current = { ...current, clipBang: on };
}

export async function setFilesBangless(on: boolean): Promise<void> {
  await api.setFilesBangless(on);
  current = { ...current, filesBangless: on };
}

export async function setFilesFallback(on: boolean): Promise<void> {
  await api.setFilesFallback(on);
  current = { ...current, filesFallback: on };
}

export async function setFilesRoots(roots: string[], excludes: string[]): Promise<void> {
  await api.setFilesRoots(roots, excludes);
  current = { ...current, filesRoots: roots, filesExcludes: excludes };
}

/**
 * Push the preference onto the document. Idempotent, and cheap enough to call on
 * every show — which is what the Palette does, because that is the guaranteed
 * sync point between a write in the Settings window and the next summon.
 */
export function applyMotionPreference(): void {
  document.documentElement.toggleAttribute("data-reduce-motion", current.reduceMotion);
}

/**
 * Push appearance and interface size onto `<html>`.
 *
 * `system` removes the attribute rather than setting it, so the stylesheet's
 * `prefers-color-scheme` query is what decides — an override that has been
 * turned off has to stop overriding, not pick a side.
 */
export function applyAppearance(): void {
  const root = document.documentElement;
  if (current.theme === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", current.theme);

  if (current.uiSize === "default") root.removeAttribute("data-ui-size");
  else root.setAttribute("data-ui-size", current.uiSize);
}

export async function setTheme(value: SettingsSnapshot["theme"]): Promise<void> {
  await api.setTheme(value);
  current = { ...current, theme: value };
  applyAppearance();
}

export async function setUiSize(value: SettingsSnapshot["uiSize"]): Promise<void> {
  await api.setUiSize(value);
  current = { ...current, uiSize: value };
  applyAppearance();
}
