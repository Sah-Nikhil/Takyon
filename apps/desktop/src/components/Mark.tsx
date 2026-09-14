/**
 * The Takyon mark: a Cherenkov wavefront. The cone is the light a particle drags
 * behind it; the dot is the particle, already past its own wake.
 *
 * Geometry locked (docs/brand.md) and generated into every icon by
 * `bun run --cwd brand build`: change `brand/geometry.js`, never the path here.
 *
 * Cone paints `currentColor`; particle paints `--mark-particle`, its own token since
 * a hover-surface token like `--accent` sits near 1.1:1 and the dot would vanish.
 * Falls back to `currentColor`: one flat colour, never an invisible dot.
 */

type MarkProps = {
  /** Rendered edge length in CSS pixels. Legible down to 16. */
  size?: number;
  /**
   * Paint the particle in `currentColor` too. For places that are already
   * monochrome by definition — a greyed-out state, a print stylesheet.
   */
  monochrome?: boolean;
  /**
   * Bring the mark to life: the particle breathes and the cone sweeps its tip
   * through five degrees either side of level, on one shared beat. Reserved for
   * genuinely idle states — motion that runs while something is happening reads
   * as a spinner and means the opposite thing.
   */
  pulse?: boolean;
  className?: string;
  /**
   * Decorative by default: the mark never carries meaning a sighted user gets
   * and a screen-reader user does not. Pass a label only where it is the sole
   * content of a control.
   */
  label?: string;
};

export function Mark({
  size = 16,
  monochrome = false,
  pulse = false,
  className,
  label,
}: MarkProps) {
  return (
    <svg
      viewBox="0 0 64 64"
      width={size}
      height={size}
      className={className}
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
      focusable="false"
    >
      <path
        d="M46,32 L12,16.5 Q23,32 12,47.5 Z"
        fill="currentColor"
        data-cone-sweep={pulse ? "true" : undefined}
      />
      <circle
        cx="56"
        cy="32"
        r="3.9"
        fill={monochrome ? "currentColor" : "var(--mark-particle, currentColor)"}
        data-particle-pulse={pulse ? "true" : undefined}
      />
    </svg>
  );
}

/**
 * The mark in the Palette's input, in the slot a search icon would take.
 *
 * 24px against 15px text: the glyph is mostly negative space, so at a search icon's
 * 16-17px it reads as a smudge. `pulse` while open with nothing typed yet.
 */
export function InputMark({ pulse = false, className }: { pulse?: boolean; className?: string }) {
  return <Mark size={24} pulse={pulse} className={className} />;
}
