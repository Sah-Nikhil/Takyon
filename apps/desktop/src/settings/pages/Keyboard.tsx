/**
 * Keyboard: the global hotkey, and nothing else yet.
 *
 * Pinned chords with a reset, never a raw capture field (ROADMAP v0.6). A
 * capture field invites chords Windows reserves and only reports the failure
 * afterwards; every chip here is known-registrable.
 *
 * The control reads the **response**, not the click: a refused chord leaves the
 * previous binding live, and the chips have to say so.
 */

import { useCallback, useEffect, useState } from "react";
import type { HotkeyStatus } from "@takyon/shared";
import * as api from "@/api";
import { Chips, Group, Row } from "../controls";

/** Matches Rust's `hotkey::DEFAULT_ACCELERATOR`; the reset button offers it. */
const DEFAULT = "Alt+Space";

export function Keyboard() {
  const [choices, setChoices] = useState<string[]>([]);
  const [status, setStatus] = useState<HotkeyStatus | null>(null);
  const [applied, setApplied] = useState(false);

  useEffect(() => {
    void api.hotkeyChoices().then(setChoices);
    void api.hotkeyStatus().then(setStatus);
  }, []);

  const bind = useCallback(async (accelerator: string) => {
    setApplied(false);
    // The response is the truth: `registered` with an `error` set means the new
    // chord was refused and the old one was kept.
    const next = await api.setHotkey(accelerator);
    setStatus(next);
    if (!next.error) {
      setApplied(true);
      setTimeout(() => setApplied(false), 1400);
    }
  }, []);

  const live = status?.accelerator ?? DEFAULT;

  return (
    <Group>
      <Row
        id="hotkey"
        label="Open Takyon with"
        applied={applied}
        error={status?.error ?? null}
        description={
          status && !status.registered
            ? "Nothing is bound. Pick another chord — the launcher is running, but only the tray can reach it."
            : "Pressed anywhere in Windows. Alt+Space is contested: PowerToys Run and the classic window menu both want it."
        }
      >
        <Chips
          label="Open Takyon with"
          value={live}
          options={choices.map((c) => ({ value: c, label: c.replace(/\+/g, " + ") }))}
          onChange={(next) => void bind(next)}
        />
        <button
          type="button"
          onClick={() => void bind(DEFAULT)}
          disabled={live === DEFAULT}
          className="rounded-control px-2 py-1 text-[12.5px] text-fg/50 transition-colors hover:bg-row-hover hover:text-fg/80 disabled:opacity-30 disabled:hover:bg-transparent"
        >
          Reset
        </button>
      </Row>
    </Group>
  );
}
