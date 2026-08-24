/**
 * The IPC contract, mirroring the Rust structs on the other side of the seam.
 *
 * `IMPLEMENTATION_PLAN.md` §8 specifies the full V1 contract — `query`,
 * `activate`, `actions_for`, `index_status`. Only the commands v0.1 actually
 * implements are declared here. Declaring the rest ahead of time would give the
 * contract tests (TBC-0007's answer to fixture drift) nothing to check against,
 * and a type with no Rust behind it is a fixture that can never drift *into*
 * correctness.
 *
 * These types are `camelCase` because the Rust side serialises with
 * `#[serde(rename_all = "camelCase")]`. If a field here is `snake_case`, either
 * this file or that attribute is wrong.
 */

/** Which window a React root is being mounted into. Chosen by `?window=` on the URL. */
export type WindowKind = "palette" | "settings";

/**
 * Why the Palette became visible. The Palette always opens empty (ROADMAP v0.1),
 * so this carries no query — it exists so the bench harness can tell a measured
 * show from an incidental one, and so a debug show can skip the focus rules.
 */
export interface ShowPayload {
  /**
   * Monotonic id for this show, minted in Rust. The frontend echoes it back via
   * `reportFirstPixel` once the frame has actually been presented; Rust owns both
   * timestamps so the two clocks never have to be reconciled.
   */
  showId: number;
  /**
   * True when the window was shown by the debug no-steal-focus path
   * (`TAKYON_NO_FOCUS_STEAL=1`). Dismiss-on-focus-loss is suppressed for these,
   * or inspecting the Palette in devtools destroys it every time.
   */
  noFocusSteal: boolean;
}

/** Whether the global hotkey is live, and if not, why not. */
export interface HotkeyStatus {
  /** The binding as accelerator text, e.g. `Alt+Space`. */
  accelerator: string;
  registered: boolean;
  /**
   * Present exactly when `registered` is false. A taken hotkey must be reported,
   * never silently swallowed (IMPLEMENTATION_PLAN §7) — this is the string the
   * user is shown.
   */
  error?: string;
}

/** Event names Rust emits. String constants so a rename is a compile error on both sides. */
export const EVENT_SHOW = "takyon://show";
export const EVENT_HIDE = "takyon://hide";
