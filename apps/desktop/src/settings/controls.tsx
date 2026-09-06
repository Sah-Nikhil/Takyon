/**
 * The settings window's control vocabulary.
 *
 * Every page is built from these four, which is what keeps forty controls from
 * becoming forty layouts. The shape is Raycast's: a group heading sits *outside*
 * a card, and each row is `label + description .......... control`.
 *
 * Apply-on-change is the rule (ROADMAP v0.6), so nothing here has a save button.
 * `Applied` is a confirmation, not a promise: it appears after the write lands.
 */

import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";

/** How long "Applied" stays up. Long enough to read, short enough not to nag. */
const APPLIED_MS = 1400;

/** A heading above a card. Absent for the first group on a page, as in Raycast. */
export function Group({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className="mb-6">
      {title && (
        <h2 className="mb-2 px-1 text-[13px] font-medium text-fg/60">{title}</h2>
      )}
      <div className="divide-y divide-hairline overflow-hidden rounded-card border border-hairline bg-card">
        {children}
      </div>
    </section>
  );
}

/**
 * One setting: what it is on the left, the control on the right.
 *
 * `id` is the anchor the settings search scrolls to, so it has to match the
 * control id declared in the page registry (`nav.ts`).
 */
export function Row({
  id,
  label,
  description,
  error,
  applied,
  children,
}: {
  id: string;
  label: string;
  description?: ReactNode;
  error?: string | null;
  applied?: boolean;
  children: ReactNode;
}) {
  // Wrapping, not a breakpoint. The widest control — six hotkey chips — is wider
  // than the content pane at *every* window size, so it has to be able to drop
  // onto its own line rather than squeeze the label to one word per line.
  return (
    <div
      id={`setting-${id}`}
      className="flex flex-wrap items-start justify-between gap-x-6 gap-y-3 px-3.5 py-3"
    >
      <div className="min-w-0 flex-1 basis-64">
        <div className="flex items-center gap-2">
          <span className="text-[14px] text-fg">{label}</span>
          {applied && (
            <span className="text-[11px] text-accent" role="status">
              Applied
            </span>
          )}
        </div>
        {description && (
          <p className="mt-1 text-[12.5px] leading-snug text-fg/60">{description}</p>
        )}
        {/*
          Beside the control rather than in a toast: the message explains why this
          switch did not move, and it has to still be there when you look back at
          it (tbd v0.1 §3).
        */}
        {error && (
          <p className="mt-1.5 text-[12.5px] leading-snug text-warning" role="alert">
            {error}
          </p>
        )}
      </div>
      {/*
        Not `shrink-0`: once this wraps onto its own line it has to be able to use
        the width it just gained, or the chips run off the card instead of
        wrapping within it. Fixed-size controls carry their own `shrink-0`.
      */}
      <div className="flex min-w-0 max-w-full flex-wrap items-center gap-2">{children}</div>
    </div>
  );
}

/** A two-state switch. Pill rather than a checkbox, matching the reference. */
export function Switch({
  checked,
  disabled,
  label,
  onChange,
}: {
  checked: boolean;
  disabled?: boolean;
  label: string;
  onChange: (on: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={`relative h-[22px] w-[38px] shrink-0 rounded-full transition-colors disabled:opacity-40 ${
        checked ? "bg-accent" : "bg-control"
      }`}
    >
      {/*
        The knob is the foreground in both states, never the plate: on the unlit
        track (8% of the foreground) a plate-coloured knob is invisible, which is
        what a switch must never be.
      */}
      <span
        className={`absolute top-[3px] size-4 rounded-full transition-[left,background-color] ${
          checked ? "left-[19px] bg-plate" : "left-[3px] bg-fg/70"
        }`}
      />
    </button>
  );
}

/**
 * Pinned options as chips (ROADMAP v0.6: chips rather than free text or sliders
 * where the option set is small).
 */
export function Chips<T extends string>({
  value,
  options,
  label,
  onChange,
}: {
  value: T;
  options: ReadonlyArray<{ value: T; label: string }>;
  label: string;
  onChange: (value: T) => void;
}) {
  return (
    // One track rather than loose buttons: six chords side by side read as six
    // controls, and they are one choice.
    <div
      role="radiogroup"
      aria-label={label}
      className="flex flex-wrap items-center gap-1 rounded-card border border-hairline bg-control/40 p-1"
    >
      {options.map((option) => {
        const chosen = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={chosen}
            onClick={() => onChange(option.value)}
            /*
              The chosen one is *lifted*, not merely tinted. 10% of the
              foreground on a near-black plate is a shade, and the old row could
              be read as having nothing selected at all.
            */
            className={`rounded-control px-2.5 py-1 text-[12.5px] transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent/60 ${
              chosen
                ? "bg-plate text-fg ring-1 ring-accent/45 shadow-[0_1px_3px_var(--color-scrim)]"
                : "text-fg/68 hover:bg-row-hover hover:text-fg/90"
            }`}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}

/**
 * A destructive change, held until it is confirmed by name.
 *
 * ROADMAP v0.6: the dialog names the consequence with the **real count**, not
 * "some items". A generic warning teaches people to click through it, which is
 * exactly the habit you do not want in front of an irreversible delete.
 */
export function Confirm({
  title,
  consequence,
  confirmLabel,
  onConfirm,
  onCancel,
}: {
  title: string;
  consequence: string;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <div
      role="alertdialog"
      aria-label={title}
      className="fixed inset-0 z-50 flex items-center justify-center bg-scrim p-6"
    >
      <div className="w-full max-w-md rounded-card border border-hairline bg-card p-5">
        <h3 className="text-[14px] font-medium text-fg">{title}</h3>
        <p className="mt-2 text-[13px] leading-snug text-fg/72">{consequence}</p>
        <div className="mt-5 flex justify-end gap-2">
          <button
            type="button"
            onClick={onCancel}
            className="rounded-control px-3 py-1.5 text-[13px] text-fg/72 transition-colors hover:bg-row-hover hover:text-fg"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={onConfirm}
            className="rounded-control bg-warning/90 px-3 py-1.5 text-[13px] font-medium text-plate transition-colors hover:bg-warning"
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

/**
 * Apply-on-change, with the confirmation and the error handling in one place.
 *
 * The optimistic value is shown immediately and then **replaced by whatever the
 * refetch returns**, never by what was clicked. That is ADR-0015's rule for
 * autostart, and it costs nothing to apply to every control.
 */
export function useApplied<T>(
  write: (value: T) => Promise<void>,
  refetch: () => Promise<T>,
): {
  applied: boolean;
  error: string | null;
  apply: (value: T, optimistic: (value: T) => void) => Promise<void>;
} {
  const [applied, setApplied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  const apply = useCallback(
    async (value: T, optimistic: (value: T) => void) => {
      setError(null);
      optimistic(value);
      try {
        await write(value);
        setApplied(true);
        if (timer.current) clearTimeout(timer.current);
        timer.current = setTimeout(() => setApplied(false), APPLIED_MS);
      } catch (e) {
        setApplied(false);
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        // Whether it threw or not. A refused write has to leave the control
        // showing what is actually true, not what was clicked.
        optimistic(await refetch());
      }
    },
    [write, refetch],
  );

  return { applied, error, apply };
}
