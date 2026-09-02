/**
 * The strip under the list, naming what Enter will do (v0.4.5 task 4).
 *
 * `actions::for_modifiers` has been kind-aware since v0.4 — Enter launches an
 * app and copies a calculation — and until now nothing in the UI said so. This
 * is the surface that says it.
 *
 * Labels come from Rust (ADR-0009), fetched once on mount rather than per arrow
 * key. Its height is `FOOTER_HEIGHT`, mirrored in `window.rs`, because the
 * native window is sized in Rust (TBC-0006).
 */

import { FOOTER_HEIGHT, type Action, type Entry } from "@takyon/shared";
import { Mark } from "@/components/Mark";

function Key({ children }: { children: React.ReactNode }) {
  return (
    <kbd className="rounded border border-white/10 bg-white/5 px-1.5 py-0.5 text-[10px] leading-none text-fg/50">
      {children}
    </kbd>
  );
}

export function Footer({
  entry,
  labels,
}: {
  entry: Entry | undefined;
  labels: Record<string, Action>;
}) {
  /*
    The Entry's *first* action is what plain Enter does, for every Kind — a Rust
    test asserts that, because the shortcut is only honest while it holds. If a
    Source ever reorders its actions this footer would name the wrong verb, and
    nothing else would notice.
   */
  const primary = entry?.actions[0];
  const label = primary ? labels[primary]?.label : undefined;

  return (
    <div
      className="flex shrink-0 items-center justify-between border-t border-white/5 px-3"
      style={{ height: FOOTER_HEIGHT }}
    >
      <Mark size={13} className="shrink-0 text-fg/25" />

      <div className="flex items-center gap-2 text-[11px] text-fg/45">
        {label && (
          <>
            <span>{label}</span>
            <Key>↵</Key>
            <span aria-hidden className="text-fg/15">
              |
            </span>
          </>
        )}
        <span>Actions</span>
        <Key>Ctrl K</Key>
      </div>
    </div>
  );
}
