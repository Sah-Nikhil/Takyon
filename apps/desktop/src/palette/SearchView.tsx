/**
 * The `!s` answer, inside the Palette (v0.9 tasks 5 and 7).
 *
 * Arc Search's shape in Takyon's clothes: while it works it names the pages it
 * is reading, then answers with a headline and labelled findings, each carrying
 * the sources behind it. The chrome is `AskView`'s, so `!c` and `!s` read as one
 * product.
 *
 * One window, exactly as `!c`: Escape goes back a step rather than dismissing.
 * The header is warm and says the query left the machine, which is the brand's
 * "cool means contained, warm means it left" (`docs/brand.md`) made literal.
 */

import { useEffect, useMemo, useRef } from "react";
import type { SearchHit } from "@takyon/shared";

import { openUrl } from "@/api";
import { Answer } from "@/components/Answer";
import { parseAnswer } from "@/search/findings";
import { useSearch } from "@/search/useSearch";

export function SearchView({
  query,
  provider,
  onClose,
}: {
  query: string;
  /** The provider before one has answered. Corrected by the first `searching`
   *  event, and again if `!s` falls back to the keyless one (ADR-0021). */
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

  // Follows only while the answer is being written. Once it is done the view
  // stays where the reader is, rather than yanking the headline off the top.
  useEffect(() => {
    if (state.phase !== "answering") return;
    const el = bodyRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [state.answer, state.phase]);

  const parsed = useMemo(
    () => parseAnswer(state.answer, state.sources.length),
    [state.answer, state.sources.length],
  );
  const busy =
    state.phase === "searching" || state.phase === "reading" || state.phase === "answering";
  // The reading list stands until the first token lands, which is where Arc
  // swaps its middle screen for the answer.
  const reading = state.phase === "reading" || (state.phase === "answering" && !state.answer);
  const answered = Boolean(parsed.headline || parsed.findings.length || parsed.rest.length);

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
          Left this machine · {state.provider ?? provider}
        </span>
        <span className="ms-auto shrink-0 text-[12px] text-fg/40" role="status">
          {state.phase === "searching"
            ? "Searching…"
            : state.phase === "reading"
              ? `Reading ${state.sources.length} web pages`
              : state.phase === "answering"
                ? `${state.agent ?? "Agent"} is writing…`
                : state.phase === "failed"
                  ? "Stopped"
                  : "Answered"}
        </span>
      </header>

      <div ref={bodyRef} className="flex-1 overflow-y-auto px-4 py-3">
        <p className="text-[13px] text-fg/60">{query}</p>

        {reading && state.sources.length > 0 && <Reading sources={state.sources} />}

        {answered && (
          <div className="mt-4 space-y-3" data-testid="findings">
            {parsed.headline && (
              <h2 className="text-[16px] font-semibold leading-snug text-fg">{parsed.headline}</h2>
            )}
            {parsed.findings.map((finding, i) => (
              <div key={i} className="flex gap-2.5">
                <span aria-hidden className="mt-[7px] size-1 shrink-0 rounded-full bg-accent/70" />
                <p className="min-w-0 text-[13.5px] leading-relaxed text-fg/85">
                  {finding.label && (
                    <span className="font-semibold text-fg">{finding.label} — </span>
                  )}
                  {finding.detail}
                  {finding.cites.map((n) => (
                    <Cite key={n} n={n} source={state.sources[n - 1]} />
                  ))}
                </p>
              </div>
            ))}
            {parsed.rest.length > 0 && (
              <Answer
                text={parsed.rest.join("\n\n")}
                className="text-[13.5px] leading-relaxed text-fg/85"
              />
            )}
          </div>
        )}

        {state.phase === "failed" && (
          <p className="mt-4 text-[13px] text-amber-300" role="alert">
            {state.error ?? "The search stopped without an answer."}
          </p>
        )}

        {!reading && state.sources.length > 0 && <Sources sources={state.sources} />}
      </div>

      <div className="flex items-center gap-3 border-t border-white/5 px-4 py-2.5">
        <span className="text-[12px] text-fg/35">Esc to go back</span>
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

/**
 * What is being read, while it is being read.
 *
 * Hosts rather than titles: the host is what a person recognises at a glance,
 * and it says whether the answer is worth trusting before a word of it exists.
 */
function Reading({ sources }: { sources: SearchHit[] }) {
  return (
    <div className="mt-4" data-testid="reading">
      <p className="text-[13.5px] text-accent">Reading {sources.length} web pages</p>
      <ul className="mt-1.5 space-y-0.5">
        {sources.map((source) => (
          <li key={source.url} className="truncate text-[13px] text-fg/45">
            {hostOf(source.url)}
          </li>
        ))}
      </ul>
    </div>
  );
}

/** One `[n]` inside a finding: the number, and the source it opens. */
function Cite({ n, source }: { n: number; source?: SearchHit }) {
  if (!source) return null;
  return (
    <button
      type="button"
      onClick={() => void openUrl(source.url)}
      title={source.title}
      aria-label={`Source ${n}: ${source.title}`}
      className="ms-1 rounded-[0.25rem] bg-control px-1 align-baseline text-[11px] tabular-nums text-fg/50 hover:bg-row-selected hover:text-fg"
    >
      {n}
    </button>
  );
}

/** The numbered list the findings cite, at the bottom, as Arc puts it. */
function Sources({ sources }: { sources: SearchHit[] }) {
  return (
    <div className="mt-5 space-y-1.5 border-t border-white/5 pt-3" data-testid="sources">
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
