/**
 * The Focus view inside the Palette (v0.16): the start form when nothing
 * runs, the controls when a session does. Every Focus command Entry opens
 * it; `intent` says which one.
 */

import { useEffect, useState } from "react";
import type { FocusCategory, FocusView as View } from "@takyon/shared";
import * as api from "@/api";
import { FocusForm } from "./FocusForm";
import { FocusRunning, type Intent } from "./FocusRunning";

export function FocusView({
  intent,
  initialGoal,
  onClose,
}: {
  intent: Intent;
  initialGoal?: string;
  onClose: () => void;
}) {
  const [view, setView] = useState<View | null>(null);
  const [categories, setCategories] = useState<FocusCategory[]>([]);
  const [ended, setEnded] = useState(false);

  useEffect(() => {
    void api.focusView().then(setView);
    void api.focusCategories().then((c) => setCategories([...c.builtins, ...c.custom]));
    return api.onFocus((signal) => {
      if (signal.kind === "view") setView(signal.view);
      if (signal.kind === "ended") setEnded(true);
    });
  }, []);

  const title = view?.running ? "Focus Session" : ended ? "Focus" : "Start Focus Session";

  return (
    <div
      className="flex h-full w-full flex-col overflow-hidden rounded-xl border border-edge bg-plate shadow-panel"
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          onClose();
        }
      }}
    >
      <div className="flex items-center gap-3 border-b border-seam px-3 py-2">
        <button
          type="button"
          onClick={onClose}
          aria-label="Back"
          className="grid size-7 shrink-0 place-items-center rounded-md text-fg/64 hover:bg-row-selected hover:text-fg"
        >
          ←
        </button>
        <span className="text-[14px] text-fg">{title}</span>
      </div>

      {view === null ? null : view.running ? (
        <FocusRunning view={view} intent={intent} />
      ) : ended && intent !== "start" ? (
        <div className="flex flex-1 flex-col items-start justify-center gap-2 px-10">
          <p className="text-[22px] font-semibold text-fg">Session complete.</p>
          <p className="text-[13px] text-fg/60">Everything you blocked is back.</p>
        </div>
      ) : (
        <FocusForm categories={categories} initialGoal={initialGoal} onStarted={() => setEnded(false)} />
      )}

    </div>
  );
}
