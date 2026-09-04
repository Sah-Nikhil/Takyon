/**
 * One Entry in the Palette's list.
 *
 * Row height is `ROW_HEIGHT` from `@takyon/shared`, which is the same number Rust
 * sizes the window from (TBC-0006). It is applied as an inline style rather than a
 * Tailwind class deliberately: a class would be a second place the number is
 * written down, and the two would disagree the first time one of them was tuned.
 */

import { ROW_HEIGHT, type Entry, type EntryKind } from "@takyon/shared";
import { AppIcon } from "@/components/AppIcon";

/**
 * What each Kind is called on the right of its row (v0.4.5 task 3).
 *
 * UI copy keyed on the wire enum, so it lives here rather than in Rust. The
 * words follow CONTEXT.md: a settings page is a destination, not a "result".
 * `calc` is absent because a calculation is a card and has no row.
 */
const KIND_LABEL: Partial<Record<EntryKind, string>> = {
  app: "Application",
  file: "File",
  folder: "Folder",
  recent: "Recent",
  system: "Settings",
  systemTask: "Task",
  clip: "Clip",
  command: "Command",
};

export function EntryRow({ entry, selected }: { entry: Entry; selected: boolean }) {
  return (
    <div
      // `px-2`, not `px-3`: the list adds 8px of its own, and 8 + 8 matches the
      // input row's `px-4` so the icon sits directly under the mark.
      className="flex items-center gap-3 px-2"
      style={{ height: ROW_HEIGHT }}
      data-selected={selected || undefined}
    >
      <AppIcon icon={entry.icon} title={entry.title} />

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
            dir="rtl"
            className="truncate text-left text-[11px] leading-tight text-fg/40"
          >
            {entry.subtitle}
          </div>
        )}
      </div>

      {/*
        Always drawn, not only on the selected row. Revealing it on selection
        would reflow every row on every arrow key, and a column that moves is
        harder to read than one that is simply there.
       */}
      {KIND_LABEL[entry.kind] && (
        <span className="shrink-0 text-[11px] leading-tight text-fg/30">
          {KIND_LABEL[entry.kind]}
        </span>
      )}
    </div>
  );
}
