/**
 * The floating Focus bar (v0.16): goal centred, time left on the right;
 * Pause and Complete on hover. A block pulses it in `accent` with the target's
 * name and, in a Soft session, Snooze. The window never takes focus (`bar.rs`),
 * so nothing here types: Strict exits go through the Palette's friction.
 * The panel is its own layer so Settings' opacity fades it, never the text.
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
  const [opacity, setOpacity] = useState(90);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    void api.focusView().then(setView);
    void api.focusSettings().then((s) => setOpacity(s.barOpacity));
    const t = setInterval(() => setNow(Date.now()), 250);
    const off = api.onFocus((signal) => {
      if (signal.kind === "view") setView(signal.view);
      if (signal.kind === "blocked") setNotice({ ...signal, at: Date.now() });
      if (signal.kind === "bar") setOpacity(signal.opacity);
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
        setHint("Strict: end it from Takyon");
        setTimeout(() => setHint(null), 4_000);
      }
    });

  const icon = "grid size-6 place-items-center rounded-md text-fg/70 hover:bg-row-selected hover:text-fg";

  return (
    <div
      data-tauri-drag-region
      data-pulse={live ? true : undefined}
      className="group relative grid h-full w-full grid-cols-[4rem_minmax(0,1fr)_4rem] items-center px-2.5"
    >
      <div
        aria-hidden
        style={{ opacity: opacity / 100 }}
        className="pointer-events-none absolute inset-0 rounded-xl border border-edge bg-plate group-data-[pulse=true]:animate-[focus-pulse_1.2s_ease-out_2] group-data-[pulse=true]:border-accent/70"
      />
      <FocusIcon name="Target" className="pointer-events-none relative size-4 justify-self-start text-accent" />
      <span data-tauri-drag-region className="relative truncate text-center text-[13px] font-medium text-fg">
        {hint ?? (live ? `${live.label} blocked` : view.goal)}
      </span>

      <div className="relative flex items-center justify-self-end">
        {live?.snoozable ? (
          <button
            type="button"
            onClick={() => {
              void api.focusSnooze(live.key);
              setNotice(null);
            }}
            className="h-6 rounded-md border border-edge bg-control px-2 text-[12px] text-fg hover:bg-row-selected"
          >
            Snooze
          </button>
        ) : (
          <>
            <span
              data-tauri-drag-region
              className={`font-mono text-[12.5px] tabular-nums group-hover:hidden ${view.paused ? "text-fg/50" : "text-fg/80"}`}
            >
              {view.paused ? "Paused" : clock(remaining)}
            </span>
            <div className="hidden items-center gap-0.5 group-hover:flex">
              {view.paused ? (
                <button type="button" aria-label="Resume" className={icon} onClick={() => void api.focusResume()}>
                  <Play weight="duotone" className="size-3.5" />
                </button>
              ) : (
                <button type="button" aria-label="Pause" className={icon} onClick={() => void exit(api.focusPause(null))}>
                  <Pause weight="duotone" className="size-3.5" />
                </button>
              )}
              <button type="button" aria-label="Complete" className={icon} onClick={() => void exit(api.focusComplete())}>
                <CheckCircle weight="duotone" className="size-3.5" />
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
