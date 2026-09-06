/**
 * The bundled themes, and nothing else. Pure data, no React, no DOM.
 *
 * A theme is a **family carrying both appearances**, not a single palette — the
 * model is t3code's `themePalettes.ts`. That is what makes Settings coherent:
 * Dark theme and Light theme are two independent picks over one list, and Follow
 * system appearance decides which half is live. A family with one half would
 * appear in one list and not the other, which reads as a bug.
 *
 * Colours are oklch because the discipline this set needs is *equal lightness
 * across families* — every dark plate at L≈0.19, every dark accent at L≈0.78 —
 * and oklch states that where hex hides it. Reasoning in `docs/adr/0023`.
 */

/** Which half of a family is being asked for. */
export type Appearance = "light" | "dark";

/**
 * How appearance is chosen. `system` follows Windows; the other two override it
 * in both directions, which is what makes it an override rather than a hint.
 */
export type AppearanceMode = "system" | Appearance;

/**
 * One half of a family: the seven roles a theme owns.
 *
 * Everything else is derived from `plate` and `fg` by an oklab mix (v0.6's
 * rule), which is why a theme is seven numbers rather than fifty. `card` and
 * `sidebar` are stated because the surface order inverts between the halves.
 */
export interface ThemeHalf {
  /** The window canvas. */
  plate: string;
  /** Text, and the source of every derived separation. */
  fg: string;
  /**
   * Something local: selection ring, tick, "Applied", the mark's particle.
   *
   * Cool in four of five. `docs/brand.md`'s cool-means-contained rule governs
   * *Bang* surfaces, which `outbound` carries; Halide takes gold because
   * obeying it literally cost that family its identity and bought nothing.
   */
  accent: string;
  /**
   * The warm counterpoint, and *only* the network one: the `!s` row, the
   * outbound header, the reading dot. Warm means it left the machine.
   */
  outbound: string;
  /**
   * A refused write, a dead hotkey, an alias whose application is gone.
   *
   * Not `outbound`, because nothing here left the machine — through v0.9 both
   * were `amber-*` and the distinction had nowhere to live. Same value as
   * `outbound` in four families; Halide is where they part.
   */
  warning: string;
  /** The raised surface: settings cards, dropdown popups. */
  card: string;
  /** The settings sidebar. Sits *under* the plate on dark, over it on light. */
  sidebar: string;
}

export interface ThemeFamily {
  id: ThemeId;
  label: string;
  /** One sentence, shown under the card in Settings. */
  note: string;
  dark: ThemeHalf;
  light: ThemeHalf;
}

export type ThemeId = "graphite" | "vela" | "cherenkov" | "aurora" | "halide";

/** Both halves of the family a fresh install gets. */
export const DEFAULT_THEME: ThemeId = "graphite";

/**
 * The warm signal, shared by four of the five families.
 *
 * One value rather than per-family: `!s` means the same thing everywhere, so it
 * should look like it. Halide is the exception — its plate, accent and this are
 * all amber, so the one meaning "this left" moves until it is unmistakable.
 */
const OUTBOUND_DARK = "oklch(0.805 0.138 72)";
const OUTBOUND_LIGHT = "oklch(0.572 0.145 55)";

export const THEMES: readonly ThemeFamily[] = [
  {
    id: "graphite",
    label: "Graphite",
    note: "Neutral. Sits over any wallpaper without arguing with it.",
    dark: {
      plate: "oklch(0.1905 0.0045 286)",
      fg: "oklch(0.966 0.0025 286)",
      accent: "oklch(0.762 0.118 251)",
      outbound: OUTBOUND_DARK,
      warning: OUTBOUND_DARK,
      card: "oklch(0.225 0.0055 286)",
      sidebar: "oklch(0.2075 0.005 286)",
    },
    light: {
      plate: "oklch(0.974 0.0018 286)",
      fg: "oklch(0.236 0.009 286)",
      accent: "oklch(0.535 0.156 254)",
      outbound: OUTBOUND_LIGHT,
      warning: OUTBOUND_LIGHT,
      card: "oklch(1 0 0)",
      sidebar: "oklch(0.9455 0.0026 286)",
    },
  },
  {
    id: "vela",
    label: "Vela",
    note: "Indigo. The plate carries the hue and the accent lifts out of it.",
    dark: {
      plate: "oklch(0.196 0.0255 293)",
      fg: "oklch(0.967 0.006 293)",
      accent: "oklch(0.745 0.142 297)",
      outbound: OUTBOUND_DARK,
      warning: OUTBOUND_DARK,
      card: "oklch(0.232 0.029 293)",
      sidebar: "oklch(0.214 0.027 293)",
    },
    light: {
      plate: "oklch(0.9745 0.0075 296)",
      fg: "oklch(0.242 0.033 295)",
      accent: "oklch(0.53 0.183 296)",
      outbound: OUTBOUND_LIGHT,
      warning: OUTBOUND_LIGHT,
      card: "oklch(0.996 0.0022 296)",
      sidebar: "oklch(0.945 0.013 296)",
    },
  },
  {
    id: "cherenkov",
    label: "Cherenkov",
    // Named rather than described: it is the mark's own hue, and it shipped as
    // the only theme through v0.9. Demoting it silently would be worse than
    // demoting it in public.
    note: "The original. Cherenkov cyan on the plate the mark was drawn against.",
    dark: {
      plate: "oklch(0.176 0.0125 245)",
      fg: "oklch(0.928 0.014 233)",
      accent: "oklch(0.775 0.113 223)",
      outbound: OUTBOUND_DARK,
      warning: OUTBOUND_DARK,
      card: "oklch(0.211 0.013 245)",
      sidebar: "oklch(0.193 0.0128 245)",
    },
    light: {
      plate: "oklch(0.964 0.005 240)",
      fg: "oklch(0.22 0.023 250)",
      accent: "oklch(0.548 0.116 234)",
      outbound: OUTBOUND_LIGHT,
      warning: OUTBOUND_LIGHT,
      card: "oklch(1 0 0)",
      sidebar: "oklch(0.937 0.0075 240)",
    },
  },
  {
    id: "aurora",
    label: "Aurora",
    note: "Green, and cool with it. The quietest of the tinted plates.",
    dark: {
      plate: "oklch(0.1935 0.0195 163)",
      fg: "oklch(0.9665 0.0055 163)",
      accent: "oklch(0.798 0.132 160)",
      outbound: OUTBOUND_DARK,
      warning: OUTBOUND_DARK,
      card: "oklch(0.229 0.0215 163)",
      sidebar: "oklch(0.211 0.0205 163)",
    },
    light: {
      plate: "oklch(0.9735 0.0065 160)",
      fg: "oklch(0.234 0.025 165)",
      accent: "oklch(0.525 0.118 158)",
      outbound: OUTBOUND_LIGHT,
      warning: OUTBOUND_LIGHT,
      card: "oklch(0.9975 0.0015 160)",
      sidebar: "oklch(0.943 0.0125 160)",
    },
  },
  {
    id: "halide",
    label: "Halide",
    note: "Warm throughout. The one family whose outbound signal has to shout.",
    dark: {
      plate: "oklch(0.1965 0.0295 63)",
      fg: "oklch(0.967 0.006 72)",
      /*
        Gold, and the one accent in the set that is not cool. `docs/brand.md`'s
        rule governs *Bang* surfaces, which `outbound` carries. The first version
        obeyed it literally with a teal, and Halide read as Aurora under another
        name — an amber plate at L 0.19 shows almost no hue of its own.
       */
      accent: "oklch(0.812 0.129 88)",
      // Red-orange, 56 degrees off the accent above. Every other family separates
      // these two by being cool and warm; this one has to do it inside the warm
      // half, and 56 degrees is what that costs.
      outbound: "oklch(0.742 0.176 32)",
      // The quieter of the two warm signals, which is the right way round: a
      // refused registry write is not a thing that left the machine.
      warning: "oklch(0.79 0.126 58)",
      card: "oklch(0.2325 0.0315 63)",
      sidebar: "oklch(0.2145 0.0305 63)",
    },
    light: {
      plate: "oklch(0.9755 0.0115 68)",
      fg: "oklch(0.238 0.023 62)",
      accent: "oklch(0.552 0.116 78)",
      outbound: "oklch(0.541 0.184 30)",
      warning: "oklch(0.564 0.138 52)",
      card: "oklch(0.9985 0.001 70)",
      sidebar: "oklch(0.9455 0.0205 68)",
    },
  },
];

/** A family by id, falling back to the default rather than throwing. */
export function family(id: string): ThemeFamily {
  return (
    THEMES.find((theme) => theme.id === id) ??
    THEMES.find((theme) => theme.id === DEFAULT_THEME)!
  );
}

/** One half of one family, by id and appearance. */
export function half(id: string, appearance: Appearance): ThemeHalf {
  return family(id)[appearance];
}
