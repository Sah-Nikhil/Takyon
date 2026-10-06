/**
 * The Focus start form: goal, duration, Block list, Strict. Raycast's layout
 * (labels in a right-aligned column), drawn in our tokens.
 *
 * The block page is painted in the live theme: the form reads the three role
 * values off `:root` and sends them with the start request.
 */

import { useEffect, useState } from "react";
import type { BlockList, FocusCategory, FocusError, PageTheme } from "@takyon/shared";
import * as api from "@/api";
import { Switch } from "@/settings/controls";
import { BlockPicker } from "./BlockPicker";
import { keyLabel, lengthText, presetMinutes } from "./format";
import { FocusIcon } from "./icons";

/** The live theme's plate, fg and accent, as `styles.css` resolved them. */
function liveTheme(): PageTheme | undefined {
  if (typeof document === "undefined") return undefined;
  const css = getComputedStyle(document.documentElement);
  const role = (name: string) => css.getPropertyValue(`--color-${name}`).trim();
  const theme = { plate: role("plate"), fg: role("fg"), accent: role("accent") };
  return theme.plate && theme.fg && theme.accent ? theme : undefined;
}

/** A FocusError, in words. */
export function focusErrorText(e: unknown): string {
  const err = e as FocusError | string;
  if (typeof err === "string") return err;
  switch (err?.kind) {
    case "alreadyRunning":
      return "A Focus session is already running.";
    case "nothingToBlock":
      return "Nothing on the Block list can be blocked on this machine.";
    case "duration":
      return err.reason;
    case "storage":
      return `Could not save the session: ${err.reason}`;
    default:
      return "Something went wrong starting Focus.";
  }
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="grid grid-cols-[104px_1fr] items-start gap-x-5">
      <span className="pt-2.5 text-right text-[13px] text-fg/60">{label}</span>
      <div className="min-w-0">{children}</div>
    </div>
  );
}

export function FocusForm({
  categories,
  onStarted,
  initialGoal = "",
}: {
  categories: FocusCategory[];
  onStarted: () => void;
  initialGoal?: string;
}) {
  const [goal, setGoal] = useState(initialGoal);
  const [duration, setDuration] = useState("45m");
  const [lengthMs, setLengthMs] = useState<number | null>(45 * 60_000);
  const [durationError, setDurationError] = useState<string | null>(null);
  const [list, setList] = useState<BlockList>({ categories: [], items: [] });
  const [strict, setStrict] = useState(false);
  const [refused, setRefused] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    void api.focusLastList().then(setList);
  }, []);

  useEffect(() => {
    let live = true;
    api.focusParseDuration(duration).then(
      (ms) => live && (setLengthMs(ms), setDurationError(null)),
      (why) => live && (setLengthMs(null), setDurationError(String(why))),
    );
    return () => {
      live = false;
    };
  }, [duration]);

  useEffect(() => {
    void api.focusPreview(list).then(setRefused);
  }, [list]);

  const empty = list.categories.length + list.items.length === 0;
  const canStart = !busy && lengthMs !== null && !empty;

  const start = () => {
    if (!canStart || lengthMs === null) return;
    setBusy(true);
    setError(null);
    api
      .focusStart({ goal, lengthMs, strict, list, theme: liveTheme() })
      .then(onStarted, (e) => setError(focusErrorText(e)))
      .finally(() => setBusy(false));
  };

  return (
    <div
      className="flex min-h-0 flex-1 flex-col"
      onKeyDown={(e) => {
        if (e.key === "Enter" && e.ctrlKey) {
          e.preventDefault();
          start();
        }
      }}
    >
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-6 py-6 pr-10">
        <Field label="Goal">
          <input
            autoFocus
            value={goal}
            onChange={(e) => setGoal(e.target.value)}
            placeholder="What are you focusing on?"
            aria-label="Goal"
            className="h-10 w-full rounded-lg border border-edge bg-control/40 px-3 text-[14px] text-fg outline-none placeholder:text-fg/45 focus:border-accent/60"
          />
        </Field>

        <Field label="Duration">
          <div className="flex items-center gap-2">
            <input
              value={duration}
              onChange={(e) => setDuration(e.target.value)}
              aria-label="Duration"
              aria-invalid={durationError !== null}
              className="h-10 w-40 rounded-lg border border-edge bg-control/40 px-3 font-mono text-[13px] tabular-nums text-fg outline-none focus:border-accent/60 aria-[invalid=true]:border-warning/60"
            />
            <div className="flex flex-wrap gap-1.5">
              {presetMinutes.map((m) => (
                <button
                  key={m}
                  type="button"
                  onClick={() => setDuration(m >= 60 && m % 60 === 0 ? `${m / 60}h` : `${m}m`)}
                  className="h-8 rounded-md px-2.5 text-[12px] text-fg/70 hover:bg-row-selected hover:text-fg"
                >
                  {lengthText(m * 60_000)}
                </button>
              ))}
            </div>
          </div>
          <p className={`mt-1.5 text-[12px] ${durationError ? "text-warning" : "text-fg/50"}`}>
            {durationError ?? (lengthMs !== null ? `${lengthText(lengthMs)}. Or type "until 4:30pm".` : "")}
          </p>
        </Field>

        <Field label="Block">
          <BlockPicker value={list} categories={categories} onChange={setList} />
          {refused.length > 0 && (
            <p className="mt-1.5 text-[12px] text-warning" role="status">
              Can't block {refused.map((k) => keyLabel(k, categories)).join(", ")}: part of Windows or Takyon itself.
            </p>
          )}
        </Field>

        <Field label="Strict">
          <div className="flex items-start gap-3 pt-2">
            <Switch checked={strict} label="Strict" onChange={setStrict} />
            <p className="text-[12px] leading-relaxed text-fg/55">
              No snooze. Pausing or ending early means retyping your goal and waiting 30 seconds.
              Survives restarts.
            </p>
          </div>
        </Field>

        {error && (
          <p className="ml-[124px] text-[13px] text-warning" role="alert">
            {error}
          </p>
        )}
      </div>

      <div className="flex shrink-0 items-center justify-between gap-3 border-t border-seam px-3 py-2">
        <span className="flex items-center gap-2 text-[11px] text-fg/60">
          <FocusIcon name="Target" className="size-3.5 text-accent" />
          Focus
        </span>
        <button
          type="button"
          onClick={start}
          disabled={!canStart}
          className="flex h-8 items-center gap-2 rounded-lg border border-edge bg-control px-3 text-[13px] font-medium text-fg hover:bg-row-selected disabled:opacity-45"
        >
          Start Focus Session
          <kbd className="rounded border border-edge bg-key px-1.5 py-0.5 text-[10px] leading-none text-fg/64">Ctrl ↵</kbd>
        </button>
      </div>
    </div>
  );
}
