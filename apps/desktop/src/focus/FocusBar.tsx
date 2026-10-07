/**
 * The floating Focus bar (v0.16): a pill that hugs its content, goal then time
 * left. On hover, Pause and Complete slide out from behind it into a dock
 * above it (below, near the top of the screen), and a goal too long for its
 * 320 px scrolls past like a departure board. A block pulses it in `accent`
 * and, in a Soft session, offers Snooze. The window never takes focus, is
 * sized to the pill and clipped to it at rest (`bar.rs` `fit`, `reveal`).
 */

import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { CheckCircle, Pause, Play } from "@phosphor-icons/react";
import type { FocusView } from "@takyon/shared";
import * as api from "@/api";
import { clock } from "./format";
import { FocusIcon } from "./icons";

type Notice = { key: string; label: string; snoozable: boolean; at: number };

const NOTICE_MS = 5_000;
/** Room above and below the pill, as `bar.rs` `DOCK`: 30 px button, 6 px gap. */
const DOCK = 36;
const PILL = 40;
/** Departure-board speed, px per second, and the gap before the text repeats. */
const SCROLL_PX_S = 40;
const SCROLL_GAP = 32;
/** Buttons in: slide out from behind the pill, settle with a slight overshoot,
 * the second 40 ms behind. Out: quicker, no overshoot. Reduced motion: fade. */
const IN = { ms: 220, ease: "cubic-bezier(0.34, 1.56, 0.64, 1)", stagger: 40 };
const OUT = { ms: 140, ease: "cubic-bezier(0.4, 0, 1, 1)" };

/** One hover button's motion, `i`th in line, tucked `hide` px behind the pill. */
function motion(out: boolean, i: number, hide: number, still: boolean): CSSProperties {
  if (still) return { opacity: out ? 1 : 0, transition: "opacity 120ms linear" };
  return {
    opacity: out ? 1 : 0,
    transform: out ? "none" : `translateY(${hide}px) scale(0.9)`,
    transition: out
      ? `transform ${IN.ms}ms ${IN.ease} ${i * IN.stagger}ms, opacity ${IN.ms / 2}ms ease-out ${i * IN.stagger}ms`
      : `transform ${OUT.ms}ms ${OUT.ease}, opacity ${OUT.ms}ms ease-in`,
  };
}

/** The goal, scrolling on hover when it does not fit its 320 px. */
function Goal({ text, moving }: { text: string; moving: boolean }) {
  const line = useRef<HTMLSpanElement>(null);
  const [full, setFull] = useState(0);
  const [fits, setFits] = useState(true);
  // Measured truncated, at rest: scrollWidth is the whole text, clientWidth the box.
  useLayoutEffect(() => {
    const l = line.current;
    if (!l) return;
    setFull(l.scrollWidth);
    setFits(l.scrollWidth <= l.clientWidth);
  }, [text]);
  const scrolls = moving && !fits;
  const shift = full + SCROLL_GAP;
  const style = { "--marquee-shift": `-${shift}px`, "--marquee-s": `${shift / SCROLL_PX_S}s` } as CSSProperties;
  return (
    <span
      data-tauri-drag-region
      className="relative block max-w-[20rem] overflow-hidden whitespace-nowrap text-[13px] font-medium text-fg"
    >
      {scrolls ? (
        <span
          data-tauri-drag-region
          style={style}
          className="inline-block motion-safe:animate-[focus-marquee_var(--marquee-s)_linear_infinite]"
        >
          {text}
          <span aria-hidden style={{ paddingLeft: SCROLL_GAP }}>
            {text}
          </span>
        </span>
      ) : (
        <span ref={line} data-tauri-drag-region className="block truncate">
          {text}
        </span>
      )}
    </span>
  );
}

export function FocusBar() {
  const [view, setView] = useState<FocusView | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [hint, setHint] = useState<string | null>(null);
  const [opacity, setOpacity] = useState(90);
  const [now, setNow] = useState(() => Date.now());
  /** Whether the buttons are out, and which dock they use. */
  const [out, setOut] = useState(false);
  const [side, setSide] = useState<"up" | "down">("up");
  const pill = useRef<HTMLDivElement>(null);
  const tuck = useRef<number | undefined>(undefined);

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

  // The window follows the pill: a new goal, a block line, Snooze appearing.
  const running = Boolean(view?.running);
  useEffect(() => {
    const el = pill.current;
    if (!el) return;
    let last = 0;
    const observer = new ResizeObserver(() => {
      const width = Math.ceil(el.getBoundingClientRect().width);
      if (width !== last) {
        last = width;
        void api.focusBarFit(width);
      }
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [running]);

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

  // Toward the middle of the screen, so the buttons never slide off it.
  const show = () => {
    window.clearTimeout(tuck.current);
    const middle = (screen.availHeight || window.innerHeight) / 2;
    const next = window.screenY + DOCK + PILL / 2 > middle ? "up" : "down";
    setSide(next);
    void api.focusBarReveal(next).then(() => requestAnimationFrame(() => setOut(true)));
  };
  // Slide back first, then give the dock's pixels back to what is underneath.
  const hide = () => {
    setOut(false);
    window.clearTimeout(tuck.current);
    tuck.current = window.setTimeout(() => void api.focusBarReveal(null), OUT.ms + 40);
  };
  const still = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
  const hide_ = side === "up" ? DOCK : -DOCK;

  const round =
    "grid size-[30px] place-items-center rounded-full border border-edge bg-plate text-fg/75 hover:bg-row-selected hover:text-fg";

  return (
    <div data-testid="focus-bar" className="relative h-full w-full overflow-hidden" onMouseLeave={hide}>
      <div
        data-testid="focus-bar-buttons"
        className="absolute left-1/2 z-0 flex -translate-x-1/2 gap-1.5"
        style={{ top: side === "up" ? 0 : DOCK + PILL + 6 }}
      >
        {view.paused ? (
          <button
            type="button"
            aria-label="Resume"
            className={round}
            style={motion(out, 0, hide_, still)}
            onClick={() => void api.focusResume()}
          >
            <Play weight="duotone" className="size-3.5" />
          </button>
        ) : (
          <button
            type="button"
            aria-label="Pause"
            className={round}
            style={motion(out, 0, hide_, still)}
            onClick={() => void exit(api.focusPause(null))}
          >
            <Pause weight="duotone" className="size-3.5" />
          </button>
        )}
        <button
          type="button"
          aria-label="Complete"
          className={round}
          style={motion(out, 1, hide_, still)}
          onClick={() => void exit(api.focusComplete())}
        >
          <CheckCircle weight="duotone" className="size-3.5" />
        </button>
      </div>

      <div
        ref={pill}
        data-tauri-drag-region
        onMouseEnter={show}
        className="absolute left-1/2 z-10 flex h-10 w-max -translate-x-1/2 items-center gap-2.5 pl-3.5 pr-3"
        style={{ top: DOCK }}
      >
        <div
          aria-hidden
          data-pulse={live ? true : undefined}
          style={{ opacity: opacity / 100 }}
          className="pointer-events-none absolute inset-0 rounded-xl border border-edge bg-plate data-[pulse=true]:animate-[focus-pulse_1.2s_ease-out_2] data-[pulse=true]:border-accent/70"
        />
        <FocusIcon name="Target" className="pointer-events-none relative size-4 shrink-0 text-accent" />
        <Goal text={hint ?? (live ? `${live.label} blocked` : view.goal)} moving={out} />
        {live?.snoozable ? (
          <button
            type="button"
            onClick={() => {
              void api.focusSnooze(live.key);
              setNotice(null);
            }}
            className="relative h-6 shrink-0 rounded-md border border-edge bg-control px-2 text-[12px] text-fg hover:bg-row-selected"
          >
            Snooze
          </button>
        ) : (
          <span
            data-tauri-drag-region
            className={`relative shrink-0 font-mono text-[12.5px] tabular-nums ${view.paused ? "text-fg/50" : "text-fg/80"}`}
          >
            {view.paused ? "Paused" : clock(remaining)}
          </span>
        )}
      </div>
    </div>
  );
}
