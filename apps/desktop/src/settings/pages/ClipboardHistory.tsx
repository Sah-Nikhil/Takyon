/**
 * Clipboard History: retention, the `!v` Bang, and the capture blocklist.
 *
 * Retention is the one genuinely destructive setting in the app. Shortening it
 * deletes rows, `secure_delete` means they are gone rather than recoverable
 * (ADR-0006), and the confirmation therefore names the real count — asked from
 * Rust *before* the change, never guessed.
 */

import { useCallback, useEffect, useState } from "react";
import type { ClipRetention } from "@takyon/shared";
import * as api from "@/api";
import { preferences, refresh, setClipBang } from "@/prefs";
import { Chips, Confirm, Group, Row, Switch, useApplied } from "../controls";

const WINDOWS: ReadonlyArray<{ value: ClipRetention; label: string }> = [
  { value: "forever", label: "Forever" },
  { value: "6-months", label: "6 months" },
  { value: "1-month", label: "1 month" },
  { value: "1-week", label: "1 week" },
  { value: "1-day", label: "1 day" },
];

export function ClipboardHistory() {
  const [retention, setRetention] = useState<ClipRetention>(() => preferences().clipRetention);
  const [bang, setBang] = useState(() => preferences().clipBang);
  const [blocked, setBlocked] = useState<string[]>([]);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  /** A retention change waiting on its confirmation, with the count it destroys. */
  const [pending, setPending] = useState<{ value: ClipRetention; impact: number } | null>(null);

  const bangApplied = useApplied(setClipBang, async () => (await refresh()).clipBang);

  useEffect(() => {
    void api.clipBlocklist().then(setBlocked);
  }, []);

  /** Ask Rust what this would destroy before doing anything. */
  const choose = useCallback(
    async (value: ClipRetention) => {
      setError(null);
      const impact = await api.clipRetentionImpact(value);
      if (impact === 0) {
        await api.setClipRetention(value);
        setRetention((await refresh()).clipRetention);
        return;
      }
      setPending({ value, impact });
    },
    [],
  );

  const commit = useCallback(async () => {
    if (!pending) return;
    await api.setClipRetention(pending.value);
    setPending(null);
    setRetention((await refresh()).clipRetention);
  }, [pending]);

  const block = useCallback(
    async (exe: string, on: boolean) => {
      setError(null);
      try {
        setBlocked(await api.setClipBlocked(exe, on));
        if (on) setDraft("");
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
    },
    [],
  );

  return (
    <>
      <Group>
        <Row
          id="retention"
          label="Keep history for"
          description="Expiry deletes rather than hides, and the sweep runs at startup and hourly. Shortening this destroys rows immediately."
        >
          <Chips
            label="Keep history for"
            value={retention}
            options={WINDOWS}
            onChange={(next) => void choose(next)}
          />
        </Row>
        <Row
          id="clip-bang"
          label="Reach history with !v"
          applied={bangApplied.applied}
          error={bangApplied.error}
          description="A shortcut, not the door: the Clipboard History command stays in the Bangless list either way."
        >
          <Switch
            label="Reach history with !v"
            checked={bang}
            onChange={(on) => void bangApplied.apply(on, setBang)}
          />
        </Row>
      </Group>

      <Group title="Never record">
        <Row
          id="blocklist"
          label="Excluded applications"
          error={error}
          description="Matched on the executable that owned the clipboard. A new entry applies to the next copy, not the next launch."
        >
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void block(draft, true);
            }}
            className="flex items-center gap-2"
          >
            <input
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              placeholder="keepass.exe"
              aria-label="Executable to exclude"
              className="w-40 rounded-control bg-control px-2.5 py-1 text-[12.5px] text-fg outline-none placeholder:text-fg/30"
            />
            <button
              type="submit"
              disabled={!draft.trim()}
              className="rounded-control px-2 py-1 text-[12.5px] text-fg/60 transition-colors hover:bg-row-hover hover:text-fg disabled:opacity-30"
            >
              Add
            </button>
          </form>
        </Row>

        {blocked.map((exe) => (
          <div key={exe} className="flex items-center justify-between gap-4 px-3.5 py-2.5">
            <span className="font-mono text-[13px] text-fg/80">{exe}</span>
            <button
              type="button"
              onClick={() => void block(exe, false)}
              className="rounded-control px-2 py-1 text-[12.5px] text-fg/50 transition-colors hover:bg-row-hover hover:text-fg"
            >
              Remove
            </button>
          </div>
        ))}
      </Group>

      {pending && (
        <Confirm
          title="This deletes clipboard history"
          consequence={`Setting retention to ${
            WINDOWS.find((w) => w.value === pending.value)?.label.toLowerCase() ?? pending.value
          } will permanently delete ${pending.impact.toLocaleString()} clipboard ${
            pending.impact === 1 ? "item" : "items"
          }. They are overwritten, not moved — there is nothing to restore from.`}
          confirmLabel="Delete them"
          onConfirm={() => void commit()}
          onCancel={() => setPending(null)}
        />
      )}
    </>
  );
}
