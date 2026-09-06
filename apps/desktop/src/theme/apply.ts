/**
 * Putting a theme on the document, and keeping it there.
 *
 * Seven custom properties on `<html>`, and the stylesheet derives the rest
 * from them. Inline style beats Tailwind's `@theme` block at `:root`, so every
 * `bg-plate` and `text-fg/60` in the app follows without knowing a theme exists.
 *
 * The subtlety is the first frame. Preferences live in Rust and arrive
 * asynchronously, so waiting would paint the default and repaint — a flash, on
 * the surface whose whole claim is one frame. The last applied pair is mirrored
 * into `localStorage` and read **synchronously at module load**; the stored
 * snapshot still wins when it lands.
 */

import {
  DEFAULT_THEME,
  half,
  type Appearance,
  type AppearanceMode,
} from "./themes";

/**
 * The guess cache, and only the guess cache. Per-window, disposable, and
 * correctness never depends on it — Rust owns the real answer.
 *
 * Spelled `takyon`, unlike `prefs.ts`'s `LEGACY_*` keys: nothing was ever
 * written under the old slug here, so there is nothing to be compatible with.
 */
const CACHE = "com.v3sper.takyon.appearance";

interface Choice {
  mode: AppearanceMode;
  dark: string;
  light: string;
}

const FALLBACK: Choice = { mode: "system", dark: DEFAULT_THEME, light: DEFAULT_THEME };

let current: Choice = FALLBACK;

/**
 * What the document is actually painted in, as `appearance:familyId`.
 *
 * Empty until the first paint, so bootstrap always writes once. Separate from
 * [`current`] because that holds the *preference*, and under `system` one
 * preference can paint either half.
 */
let painted = "";

/** Whether two choices would store the same thing. */
function sameChoice(a: Choice, b: Choice): boolean {
  return a.mode === b.mode && a.dark === b.dark && a.light === b.light;
}

/** Whether Windows is asking for a dark interface right now. */
export function systemAppearance(): Appearance {
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

/** Which half is live, given the mode and what Windows says. */
export function liveAppearance(mode: AppearanceMode = current.mode): Appearance {
  return mode === "system" ? systemAppearance() : mode;
}

/**
 * Paint a choice onto `<html>`.
 *
 * `data-appearance` is set as well as the properties: two rules cannot be
 * derived from the roles — the scrim, which darkens in *both* appearances, and
 * `color-scheme`, which fixes the caret's polarity — and both branch on it.
 */
export function applyTheme(choice: Choice): void {
  const appearance = liveAppearance(choice.mode);
  /*
    Nothing to do is the common case, and it is on the show path: the Palette
    re-reads preferences every summon. Seven custom properties on `<html>`
    invalidate computed style for the whole document and the `localStorage`
    write is synchronous, both to repaint the colour already painted.
   */
  const next = `${appearance}:${appearance === "dark" ? choice.dark : choice.light}`;
  if (next === painted && sameChoice(choice, current)) return;

  current = choice;
  painted = next;
  const colors = half(appearance === "dark" ? choice.dark : choice.light, appearance);
  const root = document.documentElement;

  root.style.setProperty("--color-plate", colors.plate);
  root.style.setProperty("--color-fg", colors.fg);
  root.style.setProperty("--color-accent", colors.accent);
  root.style.setProperty("--color-outbound", colors.outbound);
  root.style.setProperty("--color-warning", colors.warning);
  root.style.setProperty("--color-card", colors.card);
  root.style.setProperty("--color-sidebar", colors.sidebar);
  root.setAttribute("data-appearance", appearance);

  try {
    window.localStorage.setItem(CACHE, JSON.stringify(choice));
  } catch {
    // Storage blocked. The theme is still correct for this window; only the
    // next window's first frame loses its head start.
  }
}

/** Re-apply the current choice. Called when Windows flips while `mode` is `system`. */
function repaint(): void {
  applyTheme(current);
}

/**
 * Follow Windows for as long as the window lives.
 *
 * Registered once at module load rather than from a React effect: both windows
 * want it, neither ever wants to stop wanting it, and the Palette outlives every
 * component it mounts (ADR-0003).
 */
function watchSystem(): void {
  const query = window.matchMedia("(prefers-color-scheme: dark)");
  query.addEventListener("change", () => {
    // Only `system` is following. An explicit override wins in both directions,
    // which is what makes it an override.
    if (current.mode === "system") repaint();
  });
}

/** Read the guess and paint it, before anything renders. */
function bootstrap(): void {
  let guess = FALLBACK;
  try {
    const raw = window.localStorage.getItem(CACHE);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<Choice>;
      guess = {
        mode: parsed.mode ?? FALLBACK.mode,
        dark: parsed.dark ?? FALLBACK.dark,
        light: parsed.light ?? FALLBACK.light,
      };
    }
  } catch {
    // A corrupt cache is not worth reporting. `family()` falls back per id and
    // the stored snapshot overwrites this within a frame of mount anyway.
  }
  applyTheme(guess);
  watchSystem();
}

bootstrap();
