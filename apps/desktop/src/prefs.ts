/**
 * The one user preference v0.1 owns, and where it lives until it doesn't.
 *
 * There is no `settings.db` yet — that is v0.6, deliberately (docs/plans/v0.6).
 * So this sits in `localStorage`, which the WebView keeps in the app's own user
 * data folder and which both windows share because they are one origin. When
 * v0.6 lands, this module is the only thing that has to move; nothing else reads
 * the key.
 *
 * The preference is applied as an attribute on `<html>` rather than threaded
 * through props, so a single CSS rule can switch off every animation the app
 * will ever grow, not just the two that exist today.
 */

/**
 * Namespaced with the identity slug, never the display name (ADR-0011).
 * Renaming the product must not orphan the user's setting.
 */
const KEY = "com.v3sper.launcher.reduce-motion";

/** Whether Windows itself is asking for less motion. Independent of our switch. */
export function systemReducesMotion(): boolean {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** Whether the user has switched our animations off. Only our switch, not the OS. */
export function reduceMotion(): boolean {
  try {
    return window.localStorage.getItem(KEY) === "true";
  } catch {
    // A WebView with storage blocked should still run. Motion on is the default.
    return false;
  }
}

export function setReduceMotion(on: boolean): void {
  try {
    window.localStorage.setItem(KEY, String(on));
  } catch {
    // Nothing to do but keep the current session honest.
  }
  applyMotionPreference();
}

/**
 * Push the preference onto the document. Idempotent, and cheap enough to call on
 * every show — which is exactly what the Palette does, because that is the
 * guaranteed sync point: the Settings window writes the key, and the next summon
 * of the Palette reads it. The `storage` event below is the nicer path when the
 * WebView delivers it, not the one correctness rests on.
 */
export function applyMotionPreference(): void {
  document.documentElement.toggleAttribute("data-reduce-motion", reduceMotion());
}

/** Keep a live window in step with a write from the other one. */
export function watchMotionPreference(): () => void {
  const onStorage = (e: StorageEvent) => {
    if (e.key === null || e.key === KEY) applyMotionPreference();
  };
  window.addEventListener("storage", onStorage);
  return () => window.removeEventListener("storage", onStorage);
}
