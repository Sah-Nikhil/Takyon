/**
 * Applications: the alias table, which until now could only be edited with an
 * `INSERT` by hand (tbd v0.3 §3).
 *
 * Applying is in-place and needs no re-walk, so an alias takes effect on the
 * next keystroke rather than the next launch.
 */

import { useCallback, useEffect, useState } from "react";
import type { AliasRow } from "@takyon/shared";
import * as api from "@/api";
import { Group, Row } from "../controls";

export function Applications() {
  const [rows, setRows] = useState<AliasRow[]>([]);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    void api.aliases().then(setRows);
  }, []);
  useEffect(load, [load]);

  const remove = useCallback(
    async (alias: string) => {
      setError(null);
      try {
        await api.setAlias(alias, null);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setRows(await api.aliases());
      }
    },
    [],
  );

  return (
    <Group title="Aliases">
      <Row
        id="aliases"
        label="Type a short name, get the application"
        error={error}
        description="Created from the Palette's Ctrl+K menu on the row you want. This is where they are reviewed and removed."
      >
        <span className="text-[12.5px] text-fg/40">
          {rows.length === 0 ? "None yet" : `${rows.length}`}
        </span>
      </Row>

      {rows.map((row) => (
        <div
          key={row.alias}
          className="flex items-center justify-between gap-4 px-3.5 py-2.5"
        >
          <div className="min-w-0">
            <span className="font-mono text-[13px] text-fg">{row.alias}</span>
            <span className="mx-2 text-fg/30">→</span>
            {/*
              An alias can outlive its application — an uninstall, or a rename.
              Saying so beats showing an opaque id, and the row stays deletable.
            */}
            <span className={row.title ? "text-[13px] text-fg/70" : "text-[13px] text-amber-300"}>
              {row.title ?? "no longer installed"}
            </span>
          </div>
          <button
            type="button"
            onClick={() => void remove(row.alias)}
            className="shrink-0 rounded-control px-2 py-1 text-[12.5px] text-fg/50 transition-colors hover:bg-row-hover hover:text-fg"
          >
            Remove
          </button>
        </div>
      ))}
    </Group>
  );
}
