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

import { useEffect, useMemo, useRef, useState } from "react";
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
    Which citation the pointer is on, so its row in the source list lifts. The
    numbers are the only thread between a claim and its evidence, and a number
    that highlights nothing is a footnote you have to hunt for.
   */
  const [litSource, setLitSource] = useState<number | null>(null);

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
        {/*
          Warm, and only here: this is the one surface where data left. It
          breathes while the request is out and holds still once it lands — the
          mark's own rule, that motion means working and a spinner that never
          stops is a lie about state.
         */}
        <span
          aria-hidden
          data-outbound-pulse={busy ? "true" : undefined}
          className="size-2 shrink-0 rounded-full bg-amber-400"
        />
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
        {/* The question is what this surface is about, so it is the largest
            thing on it. It used to be set smaller than the answer it produced. */}
        <p className="text-[15px] font-medium leading-snug text-fg/90">{query}</p>

        {reading && state.sources.length > 0 && <Reading sources={state.sources} />}

        {answered && (
          <div className="mt-5 space-y-4" data-testid="findings">
            {parsed.headline && (
              <h2 className="text-[17px] font-semibold leading-snug tracking-[-0.01em] text-fg">
                {parsed.headline}
              </h2>
            )}
            {parsed.findings.map((finding, i) => (
              <div key={i} className="grid grid-cols-[0.75rem_1fr] gap-x-3">
                {/* A rule, not a dot: at this size a 4px dot is invisible. Two
                    pixels, not one — a hairline at a fractional offset
                    antialiases to grey on some rows and cyan on others. */}
                <span aria-hidden className="mt-[9px] h-0.5 w-3 rounded-full bg-accent/60" />
                <div className="min-w-0">
                  {finding.label && (
                    <p className="text-[13.5px] font-semibold leading-snug text-fg">
                      {finding.label}
                    </p>
                  )}
                  <p
                    className={`min-w-0 text-[13.5px] leading-relaxed text-fg/70 ${
                      finding.label ? "mt-0.5" : ""
                    }`}
                  >
                    {finding.detail}
                    {finding.cites.map((n) => (
                      <Cite key={n} n={n} source={state.sources[n - 1]} onHover={setLitSource} />
                    ))}
                  </p>
                </div>
              </div>
            ))}
            {parsed.rest.length > 0 && (
              <Answer
                text={parsed.rest.join("\n\n")}
                className="text-[13.5px] leading-relaxed text-fg/70"
              />
            )}
          </div>
        )}

        {state.phase === "failed" && (
          <p className="mt-4 text-[13px] text-amber-300" role="alert">
            {state.error ?? "The search stopped without an answer."}
          </p>
        )}

        {!reading && state.sources.length > 0 && (
          <Sources sources={state.sources} lit={litSource} onHover={setLitSource} />
        )}
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
    <div className="mt-5" data-testid="reading">
      <p className="text-[13.5px] font-medium text-accent">Reading {sources.length} web pages</p>
      {/* Numbered here as well as in the source list, so the two screens are one
          list at two moments rather than two unrelated columns of hosts. */}
      <ul className="mt-2 space-y-px">
        {sources.map((source, i) => (
          <li key={source.url} className="flex items-baseline gap-2.5 px-1.5 py-0.5">
            <span className="w-4 shrink-0 rounded-[0.25rem] bg-control text-center text-[11px] tabular-nums text-fg/40">
              {i + 1}
            </span>
            <span className="truncate text-[12.5px] text-fg/45">{hostOf(source.url)}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/**
 * One `[n]` inside a finding: the number, and the source it opens.
 *
 * Drawn identically to the number in the source list, because they are the same
 * object seen twice. Hovering either lights the other.
 */
function Cite({
  n,
  source,
  onHover,
}: {
  n: number;
  source?: SearchHit;
  onHover: (n: number | null) => void;
}) {
  if (!source) return null;
  return (
    <button
      type="button"
      onClick={() => void openUrl(source.url)}
      onMouseEnter={() => onHover(n)}
      onMouseLeave={() => onHover(null)}
      onFocus={() => onHover(n)}
      onBlur={() => onHover(null)}
      title={source.title}
      aria-label={`Source ${n}: ${source.title}`}
      className="ms-1 rounded-[0.25rem] bg-control px-1 align-baseline text-[11px] tabular-nums text-fg/50 transition-colors hover:bg-accent/20 hover:text-accent focus-visible:bg-accent/20 focus-visible:text-accent"
    >
      {n}
    </button>
  );
}

/**
 * The numbered list the findings cite, at the bottom, as Arc puts it.
 *
 * Reference weight, deliberately. Ten rows at the answer's own weight outweigh
 * the answer, which is what made this surface read as a list of links with a
 * note above it rather than as an answer with its workings shown.
 */
function Sources({
  sources,
  lit,
  onHover,
}: {
  sources: SearchHit[];
  lit: number | null;
  onHover: (n: number | null) => void;
}) {
  return (
    <div className="mt-7 border-t border-hairline pt-3.5" data-testid="sources">
      <p className="text-[11px] uppercase tracking-wide text-fg/35">Sources</p>
      <div className="mt-1.5 space-y-px">
        {sources.map((source, i) => {
          const n = i + 1;
          return (
            <button
              key={source.url}
              type="button"
              onClick={() => void openUrl(source.url)}
              onMouseEnter={() => onHover(n)}
              onMouseLeave={() => onHover(null)}
              className={`flex w-full items-baseline gap-2.5 rounded-md px-1.5 py-1 text-left transition-colors ${
                lit === n ? "bg-row-selected" : "hover:bg-row-hover"
              }`}
            >
              <span
                className={`w-4 shrink-0 rounded-[0.25rem] text-center text-[11px] tabular-nums transition-colors ${
                  lit === n ? "bg-accent/20 text-accent" : "bg-control text-fg/50"
                }`}
              >
                {n}
              </span>
              <span className="truncate text-[12.5px] text-fg/65">{source.title}</span>
              <span className="ms-auto shrink-0 truncate text-[11px] text-fg/45">
                {hostOf(source.url)}
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}

/** The host, for naming a source without spending a row on its whole URL. */
function hostOf(url: string): string {
  const host = /^https?:\/\/([^/]+)/i.exec(url)?.[1];
  return host ? host.replace(/^www\./i, "") : url;
}
