/**
 * One Entry in the Palette's list.
 *
 * Row height is `ROW_HEIGHT` from `@takyon/shared`, which is the same number Rust
 * sizes the window from (TBC-0006). It is applied as an inline style rather than a
 * Tailwind class deliberately: a class would be a second place the number is
 * written down, and the two would disagree the first time one of them was tuned.
 */

import { ROW_HEIGHT, type Entry } from "@takyon/shared";
import * as api from "@/api";

/**
 * What is drawn while an icon is missing.
 *
 * Three cases the row cannot tell apart: not extracted yet, the shell had none,
 * or no protocol handler. §6 requires it never block a row. The initial, not a
 * generic glyph, which at 24px makes every unresolved row identical.
 */
function Placeholder({ entry }: { entry: Entry }) {
  // A calculation has a glyph rather than an initial. The initial of "14.16" is
  // "1", which reads as an unresolved app icon — the one thing the placeholder
  // exists to be distinguishable from.
  const glyph =
    entry.kind === "calc" ? "=" : entry.title.trim().charAt(0).toUpperCase() || "?";

  return (
    <div
      aria-hidden
      className="grid size-6 shrink-0 place-items-center rounded-[5px] bg-fg/10 text-[11px] font-medium text-fg/50"
    >
      {glyph}
    </div>
  );
}

export function EntryRow({ entry, selected }: { entry: Entry; selected: boolean }) {
  const src = api.iconUrl(entry.icon);

  return (
    <div
      // `px-2`, not `px-3`: the list adds 8px of its own, and 8 + 8 matches the
      // input row's `px-4` so the icon sits directly under the mark.
      className="flex items-center gap-3 px-2"
      style={{ height: ROW_HEIGHT }}
      data-selected={selected || undefined}
    >
      {src ? (
        <img
          src={src}
          alt=""
          width={24}
          height={24}
          className="size-6 shrink-0"
          /*
            Fixed width and height, because the fetch resolves after this row has
            painted. Without them the text shifts sideways when an icon arrives
            late — for a list being arrowed through, the difference between
            "loading" and "flickering".
           */
          loading="eager"
          decoding="async"
        />
      ) : (
        <Placeholder entry={entry} />
      )}

      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-1.5">
          <span className="truncate text-[14px] leading-tight text-fg">{entry.title}</span>
          {entry.version && (
            /*
              Only present where two same-named executables disagree, so it is
              never decoration — it is the only thing separating two Node
              installs. `shrink-0` because the title truncates first: losing the
              version turns two identical rows back into two identical rows.
             */
            <span className="shrink-0 text-[11px] leading-tight tabular-nums text-fg/40">
              {entry.version}
            </span>
          )}
        </div>
        {entry.subtitle && (
          /*
            Truncated from the *left* for paths, because the informative end of
            `C:\Program Files\Vendor\Suite\Thing.exe` is the right-hand one. CSS has
            no left-truncation, so the text is reversed by direction and the
            ellipsis lands at the start.
           */
          <div
            // Left-truncation is a path affordance. An expression reads from the
            // left like ordinary text, so a calculation opts out of it.
            dir={entry.kind === "calc" ? undefined : "rtl"}
            className="truncate text-left text-[11px] leading-tight text-fg/40"
          >
            {entry.subtitle}
          </div>
        )}
      </div>

      {selected && (
        <kbd className="shrink-0 rounded border border-white/10 px-1.5 py-0.5 text-[10px] text-fg/40">
          ↵
        </kbd>
      )}
    </div>
  );
}
