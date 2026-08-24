/**
 * The Takyon mark: a Cherenkov wavefront. The cone is the light a particle drags
 * behind it; the dot is the particle, which has already outrun its own wake.
 *
 * The geometry is locked (docs/brand.md). The same two shapes are generated into
 * every icon in the repo by `bun run --cwd brand build`, so if this ever needs to
 * change, change `brand/geometry.js` and re-run — do not edit the path here.
 *
 * The cone paints in `currentColor`, so the mark inherits whatever colour its
 * container has. The particle paints in `--mark-particle`, which is what makes
 * the detached dot read as the live part of the mark.
 *
 * `--mark-particle` is deliberately its own token rather than one of shadcn's.
 * In that vocabulary `--accent` is a hover *surface* — it sits around 1.1:1
 * against the background by design, so a particle painted with it disappears in
 * both light and dark. Point `--mark-particle` at `--primary` (or at a dedicated
 * brand hue) in the theme layer; the fallback to `currentColor` means the mark
 * degrades to one flat colour rather than to an invisible dot.
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
 * The mark in the Palette's input field, in the slot where a search icon would
 * normally sit.
 *
 * 24px against 15px text. The mark is mostly negative space: the cone spans about
 * half the box and the particle an eighth of it, so the drawn glyph is roughly
 * half the nominal size. Set at the 16-17px a search icon would take here, it
 * reads as a smudge next to the placeholder.
 *
 * `pulse` is driven by the Palette being open with nothing typed yet, which is
 * the one moment the surface has nothing to say and is waiting on the user.
 */
export function InputMark({ pulse = false, className }: { pulse?: boolean; className?: string }) {
  return <Mark size={24} pulse={pulse} className={className} />;
}
