/**
 * Mark plus wordmark, for the Settings window header, the About pane and the
 * first-run screen.
 *
 * There is deliberately no vector logotype. `docs/brand.md` locks the mark but
 * not the typeface, so the wordmark is set in the app's own UI font and inherits
 * whatever type scale the theme lands on. When a typeface is chosen, this is the
 * one place that has to change.
 *
 * The wordmark is lowercase at every size. Never title case — see docs/brand.md.
 */

import { Mark } from "./Mark";

type LockupProps = {
  /** Height of the mark in CSS pixels; the wordmark is sized from it. */
  size?: number;
  className?: string;
};

export function Lockup({ size = 24, className }: LockupProps) {
  return (
    <span
      className={className}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: size * 0.5,
        lineHeight: 1,
      }}
    >
      <Mark size={size} />
      <span
        style={{
          fontSize: size * 0.92,
          fontWeight: 500,
          // The mark is wide and open; a little tracking keeps the wordmark from
          // reading tighter than the shape next to it.
          letterSpacing: "0.01em",
        }}
      >
        takyon
      </span>
    </span>
  );
}
