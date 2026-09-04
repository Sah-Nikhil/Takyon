/**
 * Applications: every installed application, with an editable alias per row.
 *
 * The alias is created **on the application**, which is why this lists all of
 * them rather than only the ones that have one — v0.6 shipped the review half of
 * `docs/tbd/v0.3.md` §3 and left creation to a hand-written `INSERT`, while
 * saying otherwise in three places.
 *
 * Applying is in-place and needs no re-walk, so an alias takes effect on the next
 * keystroke rather than the next launch.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import type { AliasRow, AppAliasRow } from "@takyon/shared";
import * as api from "@/api";
import { Group, Row } from "../controls";

/** Rows drawn at once. ~1900 applications is more DOM than a settings page needs. */
const PAGE = 60;

export function Applications() {
  const [rows, setRows] = useState<AppAliasRow[]>([]);
  /**
   * Aliases whose application is gone — an uninstall, or a rename.
   *
   * Listing by application would hide these entirely, and a rule nobody can see
   * is a rule nobody can delete (v0.3 tbd §3). They get their own section.
   */
  const [orphans, setOrphans] = useState<AliasRow[]>([]);
  const [filter, setFilter] = useState("");
  const [shown, setShown] = useState(PAGE);
  /** Which row is being edited, and the text in its field. */
  const [editing, setEditing] = useState<{ id: string; text: string } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    void api.applicationRows().then(setRows);
    // `title` is absent exactly when the target no longer resolves, which is the
    // one thing the by-application list cannot show.
    void api.aliases().then((all) => setOrphans(all.filter((a) => !a.title)));
  }, []);
  useEffect(load, [load]);

  const matches = useMemo(() => {
    const needle = filter.trim().toLowerCase();
    if (!needle) return rows;
    return rows.filter(
      (row) =>
        row.title.toLowerCase().includes(needle) ||
        row.aliases.some((a) => a.includes(needle)),
    );
  }, [rows, filter]);

  const commit = useCallback(async () => {
    if (!editing) return;
    setError(null);
    // Comma-separated, because the store allows several aliases per application
    // and showing only the first would hide the rest with no way to reach them.
    const next = editing.text.split(",").map((a) => a.trim()).filter(Boolean);
    try {
      await api.setAliasesFor(editing.id, next);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setEditing(null);
      setRows(await api.applicationRows());
    }
  }, [editing]);

  const forget = useCallback(
    async (alias: string) => {
      setError(null);
      try {
        await api.setAlias(alias, null);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        load();
      }
    },
    [load],
  );

  return (
    <>
    <Group title="Aliases">
      <Row
        id="aliases"
        label="Type a short name, get the application"
        error={error}
        description="Click an alias to edit it. Separate several with commas. A new alias works on the next keystroke, with no restart."
      >
        <input
          value={filter}
          onChange={(e) => {
            setFilter(e.target.value);
            setShown(PAGE);
          }}
          placeholder="Filter applications"
          aria-label="Filter applications"
          className="w-44 rounded-control bg-control px-2.5 py-1 text-[12.5px] text-fg outline-none placeholder:text-fg/30"
        />
      </Row>

      {rows.length === 0 && (
        <div className="px-3.5 py-2.5 text-[12.5px] text-fg/40">
          Still finding applications…
        </div>
      )}

      {matches.slice(0, shown).map((row) => (
        <div
          key={row.id}
          className="flex items-center justify-between gap-4 px-3.5 py-2.5"
        >
          <div className="min-w-0">
            <div className="truncate text-[13px] text-fg">{row.title}</div>
            {row.subtitle && (
              <div className="truncate text-[11.5px] text-fg/35">{row.subtitle}</div>
            )}
          </div>

          {editing?.id === row.id ? (
            <input
              autoFocus
              value={editing.text}
              onChange={(e) => setEditing({ id: row.id, text: e.target.value })}
              onBlur={() => void commit()}
              onKeyDown={(e) => {
                if (e.key === "Enter") void commit();
                // Escape abandons the edit rather than saving it, which is what
                // Escape means everywhere else in this app.
                if (e.key === "Escape") setEditing(null);
              }}
              aria-label={`Alias for ${row.title}`}
              placeholder="ps, photo"
              className="w-36 shrink-0 rounded-control bg-control px-2.5 py-1 text-right font-mono text-[12.5px] text-fg outline-none placeholder:text-fg/30"
            />
          ) : (
            <button
              type="button"
              onClick={() => setEditing({ id: row.id, text: row.aliases.join(", ") })}
              aria-label={`Alias for ${row.title}`}
              className={`w-36 shrink-0 rounded-control px-2.5 py-1 text-right transition-colors hover:bg-row-hover ${
                row.aliases.length > 0
                  ? "font-mono text-[12.5px] text-fg/80"
                  : "text-[12.5px] text-fg/35"
              }`}
            >
              {row.aliases.length > 0 ? row.aliases.join(", ") : "Add alias"}
            </button>
          )}
        </div>
      ))}

      {matches.length > shown && (
        <button
          type="button"
          onClick={() => setShown((n) => n + PAGE)}
          className="w-full px-3.5 py-2.5 text-left text-[12.5px] text-fg/50 transition-colors hover:bg-row-hover hover:text-fg"
        >
          Show more ({matches.length - shown} left)
        </button>
      )}

      {rows.length > 0 && matches.length === 0 && (
        <div className="px-3.5 py-2.5 text-[12.5px] text-fg/40">
          No application matches “{filter}”.
        </div>
      )}
    </Group>

    {orphans.length > 0 && (
      <Group title="Aliases with no application">
        <Row
          id="orphan-aliases"
          label="These point at something that is gone"
          description="An uninstall or a rename leaves the rule behind. Listed rather than hidden, because a rule nobody can see is a rule nobody can delete."
        >
          <span className="text-[12.5px] text-fg/40">{orphans.length}</span>
        </Row>

        {orphans.map((row) => (
          <div
            key={row.alias}
            className="flex items-center justify-between gap-4 px-3.5 py-2.5"
          >
            <div className="min-w-0">
              <span className="font-mono text-[13px] text-fg">{row.alias}</span>
              <span className="mx-2 text-fg/30">→</span>
              <span className="text-[13px] text-amber-300">no longer installed</span>
            </div>
            <button
              type="button"
              onClick={() => void forget(row.alias)}
              className="shrink-0 rounded-control px-2 py-1 text-[12.5px] text-fg/50 transition-colors hover:bg-row-hover hover:text-fg"
            >
              Remove
            </button>
          </div>
        ))}
      </Group>
    )}
    </>
  );
}
