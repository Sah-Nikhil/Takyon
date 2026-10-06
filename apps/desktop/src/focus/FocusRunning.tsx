/**
 * The running Focus session: goal, countdown, Pause, Resume, +10 min,
 * Complete. Strict puts friction in front of Pause and Complete: retype the
 * goal, wait out 30 s (ADR-0036). The countdown is local, from `endsAt`.
 */

import { useEffect, useRef, useState } from "react";
import type { FocusView } from "@takyon/shared";
import * as api from "@/api";
import { clock, untilText } from "./format";
import { focusErrorText } from "./FocusForm";

export type Intent = "start" | "complete" | "pause" | "resume";
type Exit = "pause" | "complete";

const BREAKS = [5, 10, 15, 30];

function useNow(ms = 250) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), ms);
    return () => clearInterval(t);
  }, [ms]);
  return now;
}

const button =
  "h-9 rounded-lg border border-edge bg-control px-3.5 text-[13px] font-medium text-fg hover:bg-row-selected disabled:opacity-45";

export function FocusRunning({ view, intent }: { view: FocusView; intent: Intent }) {
  const now = useNow();
  const [pending, setPending] = useState<Exit | null>(null);
  const [breakMinutes, setBreakMinutes] = useState<number | null>(null);
  const [typed, setTyped] = useState("");
  const [error, setError] = useState<string | null>(null);
  const handled = useRef(false);

  const remaining = view.paused ? view.remainingMs : Math.max(0, view.endsAt - now);
  // Local countdown from the moment Rust confirmed the goal; Rust stays the
  // judge of readiness when Pause or Complete is pressed.
  // Adjusted during render when Rust's answer changes (React's documented
  // pattern for state derived from props), not in an effect.
  const [frictionSeen, setFrictionSeen] = useState<number | null | undefined>(undefined);
  const [frictionFrom, setFrictionFrom] = useState<number | null>(null);
  if (view.frictionMs !== frictionSeen) {
    setFrictionSeen(view.frictionMs);
    setFrictionFrom(view.frictionMs == null ? null : (frictionFrom ?? now - (30_000 - view.frictionMs)));
  }
  const frictionLeft = frictionFrom === null ? null : Math.max(0, 30_000 - (now - frictionFrom));

  // State only changes once the call settles, so this is safe from an effect.
  const act = (exit: Exit, minutes: number | null = breakMinutes) => {
    const call = exit === "pause" ? api.focusPause(minutes) : api.focusComplete();
    call.then(
      () => {
        setPending(null);
        setError(null);
      },
      (e) => {
        if ((e as { kind?: string })?.kind === "needsFriction") setPending(exit);
        else setError(focusErrorText(e));
      },
    );
  };

  // A command Entry (Pause, Complete, Resume Focus Session) acts once on open.
  useEffect(() => {
    if (handled.current) return;
    handled.current = true;
    if (intent === "resume" && view.paused) void api.focusResume();
    if (intent === "pause" && !view.paused) act("pause", null);
    if (intent === "complete") act("complete");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Leaving mid-countdown starts it over (ADR-0036).
  useEffect(() => () => void api.focusResetFriction(), []);

  const status = view.paused
    ? view.breakEndsAt
      ? `Paused · back at ${untilText(view.breakEndsAt)}`
      : "Paused"
    : `left · until ${untilText(view.endsAt)}`;

  return (
    <div className="flex min-h-0 flex-1 flex-col items-start justify-center gap-6 px-10 py-8">
      <div>
        <p className="mb-2 text-[13px] text-fg/55">{view.strict ? "Strict Focus session" : "Focus session"}</p>
        <h2 className="max-w-[34rem] text-[28px] font-semibold leading-tight tracking-[-0.02em] text-fg [text-wrap:balance]">
          {view.goal}
        </h2>
      </div>

      <div className="flex w-full max-w-[34rem] items-baseline gap-4 border-t border-seam pt-5">
        <span className={`font-mono text-[34px] font-medium tabular-nums ${view.paused ? "text-fg/50" : "text-fg"}`}>
          {clock(remaining)}
        </span>
        <span className="text-[13px] text-fg/60">{status}</span>
      </div>

      {pending ? (
        <Friction
          exit={pending}
          goal={view.goal}
          typed={typed}
          setTyped={setTyped}
          frictionLeft={frictionLeft}
          onConfirm={() => act(pending)}
          onCancel={() => {
            setPending(null);
            setTyped("");
            void api.focusResetFriction();
          }}
        />
      ) : (
        <div className="flex flex-wrap items-center gap-2">
          {view.paused ? (
            <button type="button" className={button} onClick={() => void api.focusResume()}>
              Resume
            </button>
          ) : (
            <>
              <span className="text-[12px] text-fg/55">Pause for</span>
              {BREAKS.map((m) => (
                <button
                  key={m}
                  type="button"
                  className={button}
                  onClick={() => {
                    setBreakMinutes(m);
                    act("pause", m);
                  }}
                >
                  {m} min
                </button>
              ))}
            </>
          )}
          <span aria-hidden className="mx-1 h-5 w-px bg-seam" />
          <button type="button" className={button} onClick={() => void api.focusExtend(10)}>
            +10 min
          </button>
          <button type="button" className={button} onClick={() => act("complete")}>
            Complete
          </button>
        </div>
      )}

      {error && (
        <p className="text-[13px] text-warning" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

function Friction({
  exit,
  goal,
  typed,
  setTyped,
  frictionLeft,
  onConfirm,
  onCancel,
}: {
  exit: Exit;
  goal: string;
  typed: string;
  setTyped: (t: string) => void;
  frictionLeft: number | null;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const verb = exit === "pause" ? "pause" : "end this session early";
  const matched = frictionLeft !== null;
  return (
    <div className="w-full max-w-[34rem] rounded-xl border border-edge bg-card p-4">
      <p className="text-[13px] text-fg/80">
        This is a Strict session. To {verb}, type your goal exactly, then wait 30 seconds.
      </p>
      <input
        autoFocus
        value={typed}
        disabled={matched}
        aria-label="Retype your goal"
        placeholder={goal}
        onChange={(e) => {
          setTyped(e.target.value);
          void api.focusTypeGoal(e.target.value);
        }}
        className="mt-3 h-10 w-full rounded-lg border border-edge bg-control/40 px-3 text-[14px] text-fg outline-none placeholder:text-fg/30 focus:border-accent/60 disabled:opacity-60"
      />
      <div className="mt-3 flex items-center justify-between">
        <span className="font-mono text-[13px] tabular-nums text-fg/60">
          {matched ? (frictionLeft! > 0 ? `${clock(frictionLeft!)} to wait` : "Ready") : "Waiting for your goal"}
        </span>
        <div className="flex gap-2">
          <button type="button" className={button} onClick={onCancel}>
            Keep focusing
          </button>
          <button type="button" className={button} disabled={!matched || frictionLeft! > 0} onClick={onConfirm}>
            {exit === "pause" ? "Pause" : "End session"}
          </button>
        </div>
      </div>
    </div>
  );
}
