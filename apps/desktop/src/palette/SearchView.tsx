/**
 * The `!s` answer, inside the Palette (v0.9 tasks 5 and 7).
 *
 * One window, exactly as `!c`: Escape goes back a step rather than dismissing.
 * The header is warm and says the query left the machine, which is the brand's
 * "cool means contained, warm means it left" made literal (`docs/brand.md`) —
 * ADR-0002's guarantee as something a person can see rather than read.
 *
 * Citations are rendered from the source list, so `[2]` becomes the second
 * source's link without any parsing of the Agent's prose.
 */

import { useEffect, useRef } from "react";
import type { SearchHit } from "@takyon/shared";

import { openUrl } from "@/api";
import { Answer } from "@/components/Answer";
import { useSearch } from "@/search/useSearch";

const PHASE_COPY: Record<string, string> = {
  searching: "Searching the web…",
  reading: "Reading sources…",
  answering: "Writing the answer…",
  done: "Answered",
  failed: "Stopped",
};

export function SearchView({
  query,
  provider,
  onClose,
}: {
  query: string;
  provider: string;
  onClose: () => void;
}) {
  const { state, search, cancel } = useSearch();
  const bodyRef = useRef<HTMLDivElement>(null);

  /*
    Once, for the query that opened this view. Guarded by a ref rather than an
    empty dependency list alone: StrictMode runs mount effects twice in `bun run
    dev`, and a search is a paid request and an Agent Turn.
   */
  const asked = useRef(false);
  useEffect(() => {
    if (asked.current) return;
    asked.current = true;
    void search(query);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const el = bodyRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [state.answer]);

  const busy =
    state.phase === "searching" || state.phase === "reading" || state.phase === "answering";

  return (
    <div
      className="flex h-full w-full flex-col overflow-hidden rounded-xl border border-white/10 bg-plate/95 shadow-2xl backdrop-blur-xl"
      onKeyDown={(e) => {
        if (e.key !== "Escape") return;
        e.stopPropagation();
        e.preventDefault();
        onClose();
      }}
    >
      <header className="flex items-center gap-2.5 border-b border-white/5 px-4 py-3">
        {/* Warm, and only here: this is the one surface where data left. */}
        <span aria-hidden className="size-2 shrink-0 rounded-full bg-amber-400" />
        <span className="shrink-0 text-[13px] text-amber-200/90" data-testid="outbound">
          Left this machine · {provider}
        </span>
        <span className="ms-auto shrink-0 text-[12px] text-fg/40" role="status">
          {state.agent && state.phase === "answering"
            ? `${state.agent} is writing…`
            : (PHASE_COPY[state.phase] ?? "")}
        </span>
      </header>

      <div ref={bodyRef} className="flex-1 space-y-4 overflow-y-auto px-4 py-3">
        <p className="text-[13px] text-fg/60">{query}</p>

        {state.answer && (
          <Answer text={state.answer} className="text-[13.5px] leading-relaxed text-fg/90" />
        )}

        {state.phase === "failed" && (
          <p className="text-[13px] text-amber-300" role="alert">
            {state.error ?? "The search stopped without an answer."}
          </p>
        )}

        {state.sources.length > 0 && (
          <Sources sources={state.sources} />
        )}
      </div>

      <div className="flex items-center gap-3 border-t border-white/5 px-4 py-2.5">
        <span className="text-[12px] text-fg/35">
          {busy ? "Esc to go back" : "Esc to go back · Enter on a source to open it"}
        </span>
        {busy && (
          <button
            type="button"
            onClick={cancel}
            className="ms-auto shrink-0 rounded-md border border-white/10 px-2.5 py-1 text-[12px] text-fg/70 hover:text-fg"
          >
            Stop
          </button>
        )}
      </div>
    </div>
  );
}

/** The numbered list the answer cites. Numbers match `[n]` in the prose. */
function Sources({ sources }: { sources: SearchHit[] }) {
  return (
    <div className="space-y-1.5" data-testid="sources">
      <p className="text-[11px] uppercase tracking-wide text-fg/35">Sources</p>
      {sources.map((source, i) => (
        <button
          key={source.url}
          type="button"
          onClick={() => void openUrl(source.url)}
          className="flex w-full items-baseline gap-2 rounded-md px-1 py-0.5 text-left hover:bg-white/5"
        >
          <span className="shrink-0 text-[12px] tabular-nums text-fg/40">[{i + 1}]</span>
          <span className="truncate text-[13px] text-fg/85">{source.title}</span>
          <span className="ms-auto shrink-0 truncate text-[11px] text-fg/35">
            {hostOf(source.url)}
          </span>
        </button>
      ))}
    </div>
  );
}

/** The host, for naming a source without spending a row on its whole URL. */
function hostOf(url: string): string {
  const host = /^https?:\/\/([^/]+)/i.exec(url)?.[1];
  return host ? host.replace(/^www\./i, "") : url;
}
