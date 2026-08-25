/**
 * The `Ctrl+K` action menu.
 *
 * Built as a shared primitive with one Source in existence, which is what the
 * phase plan asks for and worth restating: **every Source and every Mode
 * contributes actions to this menu**, so retrofitting it after three Sources exist
 * means touching all three. The component knows nothing about applications — it
 * renders whatever `actionsFor` returned.
 *
 * Accelerators are shown in the right-hand column rather than documented
 * elsewhere. A shortcut nobody can find is folklore, and the menu is where someone
 * goes to find out what is possible.
 */

import { useEffect, useRef, useState } from "react";
import { Command } from "cmdk";
import type { Action } from "@takyon/shared";

export function ActionMenu({
  actions,
  onRun,
  onClose,
}: {
  actions: Action[];
  onRun: (actionId: string) => void;
  onClose: () => void;
}) {
  const [value, setValue] = useState(actions[0]?.id ?? "");
  const inputRef = useRef<HTMLInputElement>(null);

  // Focus moves into the menu on open and back to the query input on close. Not
  // doing this leaves the arrow keys driving the Entry list behind the menu,
  // which changes what Enter will act on while the menu is open — the selection
  // moves under a menu that is still describing the old one.
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  return (
    <div
      /*
        `dialog` with a name of its own, not cmdk's `label` prop: that names the
        inner listbox, so "Actions" would land on the search input. Anything
        looking the menu up by name would be handed a text field with no items.
       */
      role="dialog"
      aria-label="Actions"
      aria-modal="true"
      /*
        A backdrop that closes on click, and stops the click reaching the Palette
        underneath. Without the second half, dismissing the menu also activates
        whichever row happened to be under the pointer.
       */
      className="absolute inset-0 z-10 flex items-end justify-end bg-black/40 p-2"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <Command
        value={value}
        onValueChange={setValue}
        className="w-64 overflow-hidden rounded-lg border border-white/10 bg-plate shadow-2xl"
        onKeyDown={(e) => {
          // Escape closes the menu rather than the Palette. The Palette's own
          // Escape handler is bound to the document, so without stopping
          // propagation here one keypress would do both — the menu would close
          // and the Palette would vanish behind it.
          if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            onClose();
          }
        }}
      >
        <div className="border-b border-white/10 px-3 py-2">
          <Command.Input
            ref={inputRef}
            placeholder="Search actions"
            className="w-full bg-transparent text-[13px] text-fg outline-none placeholder:text-fg/35"
          />
        </div>
        <Command.List className="max-h-64 overflow-y-auto p-1">
          <Command.Empty className="px-2 py-3 text-[12px] text-fg/40">
            No matching action.
          </Command.Empty>
          {actions.map((action) => (
            <Command.Item
              key={action.id}
              value={action.id}
              // cmdk filters on `value`, which is the id — so typing "run" would
              // match nothing. `keywords` is what makes the label searchable.
              keywords={[action.label]}
              onSelect={() => onRun(action.id)}
              className="flex cursor-default items-center justify-between gap-3 rounded px-2 py-1.5 text-[13px] text-fg/80 data-[selected=true]:bg-white/10 data-[selected=true]:text-fg"
            >
              <span className="truncate">{action.label}</span>
              {action.accelerator && (
                <kbd className="shrink-0 rounded border border-white/10 px-1.5 py-0.5 text-[10px] text-fg/40">
                  {action.accelerator}
                </kbd>
              )}
            </Command.Item>
          ))}
        </Command.List>
      </Command>
    </div>
  );
}
