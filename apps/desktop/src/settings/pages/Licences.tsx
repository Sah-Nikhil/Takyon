/**
 * "About / Third-party licences": every dependency in bundled notices, its
 * version and licence, searchable, each row opening onto its licence texts.
 *
 * Data is `THIRD_PARTY_NOTICES.json`, generated with the notices text from the
 * same dependency graph and read only when this view mounts: never at startup,
 * never on the Palette path. Offline by construction; no row links out.
 */

import { useEffect, useMemo, useState } from "react";
import { CaretRight } from "@phosphor-icons/react";
import type { LicenceComponent, ThirdPartyLicences } from "@takyon/shared";
import * as api from "@/api";

const key = (c: LicenceComponent) => `${c.kind}:${c.name}@${c.version}`;

/** Name, version, SPDX id or licence name: whatever someone types to find a row. */
function matches(c: LicenceComponent, needle: string): boolean {
  return [c.name, c.version, c.licence ?? "", c.licenceName ?? ""].some((s) => s.toLowerCase().includes(needle));
}

export function Licences() {
  const [data, setData] = useState<ThirdPartyLicences | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState<ReadonlySet<string>>(new Set());

  useEffect(() => {
    void api.thirdPartyLicences().then(setData, (e: unknown) => setError(e instanceof Error ? e.message : String(e)));
  }, []);

  const needle = query.trim().toLowerCase();
  const shown = useMemo(
    () => (data && needle ? data.components.filter((c) => matches(c, needle)) : (data?.components ?? [])),
    [data, needle],
  );

  const toggle = (id: string) =>
    setOpen((prev) => {
      const next = new Set(prev);
      if (!next.delete(id)) next.add(id);
      return next;
    });

  return (
    <div className="space-y-4">
      <p className="max-w-[68ch] text-[12.5px] leading-snug text-fg/60">
        Takyon's own licence is CC-BY-NC-SA-4.0. These components keep their own licences. OR means a choice of
        licence; AND means both apply.
      </p>

      <div className="flex items-center gap-3">
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            // Clears rather than leaving the view, as long as there is a query to clear.
            if (e.key === "Escape" && query) {
              e.stopPropagation();
              setQuery("");
            }
          }}
          placeholder="Search components and licences"
          aria-label="Search components and licences"
          disabled={!data}
          className="min-w-0 flex-1 rounded-control bg-control px-2.5 py-1.5 text-[13px] text-fg outline-none placeholder:text-fg/50 disabled:opacity-50"
        />
        {data && (
          <span className="shrink-0 text-[12px] tabular-nums text-fg/56" role="status">
            {needle ? `${shown.length} of ${data.components.length}` : `${data.components.length} components`}
          </span>
        )}
      </div>

      <div className="divide-y divide-hairline overflow-hidden rounded-card border border-hairline bg-card">
        {error ? (
          <p className="px-3.5 py-3 text-[12.5px] leading-snug text-warning" role="alert">
            {error}
          </p>
        ) : !data ? (
          <p className="px-3.5 py-3 text-[12.5px] text-fg/56">Reading the licence list…</p>
        ) : shown.length === 0 ? (
          <p className="px-3.5 py-3 text-[12.5px] text-fg/56">No component matches “{query.trim()}”.</p>
        ) : (
          shown.map((c) => (
            <LicenceRow key={key(c)} c={c} data={data} open={open.has(key(c))} onToggle={() => toggle(key(c))} />
          ))
        )}
      </div>
    </div>
  );
}

function LicenceRow({
  c,
  data,
  open,
  onToggle,
}: {
  c: LicenceComponent;
  data: ThirdPartyLicences;
  open: boolean;
  onToggle: () => void;
}) {
  const panel = `licence-${c.kind}-${c.name}-${c.version}`.replace(/[^\w-]/g, "-");
  return (
    <div>
      <button
        type="button"
        aria-expanded={open}
        aria-controls={open ? panel : undefined}
        onClick={onToggle}
        className="flex w-full items-center gap-2.5 px-3.5 py-2 text-left transition-colors hover:bg-row-hover focus-visible:bg-row-hover focus-visible:outline-none"
      >
        <CaretRight
          weight="duotone"
          aria-hidden
          className={`size-3.5 shrink-0 text-fg/45 transition-transform ${open ? "rotate-90" : ""}`}
        />
        <span className="min-w-0 truncate text-[13px] text-fg">{c.name}</span>
        <span className="shrink-0 font-mono text-[11.5px] tabular-nums text-fg/50">{c.version}</span>
        <span className="shrink-0 rounded bg-control px-1.5 py-px text-[10.5px] text-fg/60">{c.kind}</span>
        <span className="min-w-0 flex-1 truncate text-right font-mono text-[11.5px] text-fg/70">
          {c.licence ?? "No licence declared"}
        </span>
      </button>

      {open && (
        <div id={panel} className="space-y-3 px-3.5 pb-3.5 pl-9">
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-[12px]">
            <dt className="text-fg/50">Licence</dt>
            <dd className="select-text text-fg/80">{c.licenceName ?? "None declared by the package"}</dd>
            {c.repository && (
              <>
                <dt className="text-fg/50">Source</dt>
                <dd className="select-text break-all font-mono text-[11.5px] text-fg/70">{c.repository}</dd>
              </>
            )}
          </dl>
          {c.canonical && (
            <p className="text-[12px] leading-snug text-fg/56">
              This package ships no licence text, so the standard SPDX text is shown.
            </p>
          )}
          {c.texts.length === 0 && (
            <p className="text-[12px] leading-snug text-fg/56">No licence text is available for this package.</p>
          )}
          {c.texts.map((i) => {
            const t = data.texts[i];
            if (!t) return null;
            return (
              <figure key={i} className="space-y-1.5">
                <figcaption className="font-mono text-[11.5px] text-fg/56">{t.name}</figcaption>
                <pre className="max-h-64 select-text overflow-auto whitespace-pre-wrap break-words rounded-control border border-hairline bg-plate/60 p-3 font-mono text-[11.5px] leading-relaxed text-fg/76">
                  {t.text}
                </pre>
              </figure>
            );
          })}
        </div>
      )}
    </div>
  );
}
