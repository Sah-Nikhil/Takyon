/**
 * The floating Focus bar (v0.16): goal and time left; Pause and Complete on
 * hover. A block pulses it in `accent` with the target's name and, in a Soft
 * session, Snooze. The window never takes focus (`bar.rs`), so nothing here
 * types: Strict exits go through the Palette's friction.
 */

import { useEffect, useState } from "react";
import { CheckCircle, Pause, Play } from "@phosphor-icons/react";
import type { FocusView } from "@takyon/shared";
import * as api from "@/api";
import { clock } from "./format";
import { FocusIcon } from "./icons";

type Notice = { key: string; label: string; snoozable: boolean; at: number };

const NOTICE_MS = 5_000;

export function FocusBar() {
  const [view, setView] = useState<FocusView | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [hint, setHint] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    void api.focusView().then(setView);
    const t = setInterval(() => setNow(Date.now()), 250);
    const off = api.onFocus((signal) => {
      if (signal.kind === "view") setView(signal.view);
      if (signal.kind === "blocked") setNotice({ ...signal, at: Date.now() });
    });
    return () => {
      clearInterval(t);
      off();
    };
  }, []);

  if (!view?.running) return null;

  const live = notice && now - notice.at < NOTICE_MS ? notice : null;
  const remaining = view.paused ? view.remainingMs : Math.max(0, view.endsAt - now);

  const exit = (call: Promise<unknown>) =>
    call.catch((e) => {
      if ((e as { kind?: string })?.kind === "needsFriction") {
        setHint("Strict: end it from Takyon → Complete Focus Session");
        setTimeout(() => setHint(null), 4_000);
      }
    });

  const icon = "grid size-7 place-items-center rounded-md text-fg/70 hover:bg-row-selected hover:text-fg";

  return (
    <div
      data-tauri-drag-region
      data-pulse={live ? true : undefined}
      className="group flex h-full w-full items-center gap-2.5 rounded-xl border border-edge bg-plate px-3 shadow-panel data-[pulse=true]:animate-[focus-pulse_1.2s_ease-out_2] data-[pulse=true]:border-accent/70"
    >
      <FocusIcon name="Target" className="pointer-events-none size-4 shrink-0 text-accent" />
      <span data-tauri-drag-region className="min-w-0 flex-1 truncate text-[13px] text-fg">
        {hint ?? (live ? `${live.label} blocked` : view.goal)}
      </span>

      {live?.snoozable ? (
        <button
          type="button"
          onClick={() => {
            void api.focusSnooze(live.key);
            setNotice(null);
          }}
          className="h-7 rounded-md border border-edge bg-control px-2 text-[12px] text-fg hover:bg-row-selected"
        >
          Snooze
        </button>
      ) : (
        <>
          <span
            data-tauri-drag-region
            className={`font-mono text-[13px] tabular-nums group-hover:hidden ${view.paused ? "text-fg/50" : "text-fg/80"}`}
          >
            {view.paused ? "Paused" : clock(remaining)}
          </span>
          <div className="hidden items-center gap-0.5 group-hover:flex">
            {view.paused ? (
              <button type="button" aria-label="Resume" className={icon} onClick={() => void api.focusResume()}>
                <Play weight="duotone" className="size-4" />
              </button>
            ) : (
              <button type="button" aria-label="Pause" className={icon} onClick={() => void exit(api.focusPause(null))}>
                <Pause weight="duotone" className="size-4" />
              </button>
            )}
            <button type="button" aria-label="Complete" className={icon} onClick={() => void exit(api.focusComplete())}>
              <CheckCircle weight="duotone" className="size-4" />
            </button>
          </div>
        </>
      )}
    </div>
  );
}
