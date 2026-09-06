/**
 * The `!s` answer, inside the Palette (v0.9 tasks 5 and 7, reshaped v0.10).
 *
 * Arc Search's shape in Takyon's clothes: while it works it names the pages it
 * is reading, then answers with an accent headline, optional sections, and
 * findings that each carry an icon, a linked label and the sources behind them.
 * A card strip sits under the first group, as Arc puts it, and the numbered list
 * is at the bottom. The chrome is `AskView`'s, so `!c` and `!s` read as one
 * product.
 *
 * One window, exactly as `!c`: Escape goes back a step rather than dismissing.
 * The header is warm and says the query left the machine, which is the brand's
 * "cool means contained, warm means it left" (`docs/brand.md`) made literal.
 */

import { OpenNewWindow } from "iconoir-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { SearchHit } from "@takyon/shared";

import { openUrl } from "@/api";
import { Answer } from "@/components/Answer";
import { InlineText } from "@/components/Answer";
import { Favicon } from "@/search/Favicon";
import { FindingIcon } from "@/search/FindingIcon";
import { parseAnswer } from "@/search/findings";
import { useSearch } from "@/search/useSearch";

/** Cards in the strip. Six is two screens of horizontal scroll, not a carousel. */
const CARDS = 6;

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

  /*
    Deliberately not following the stream. Since v0.10 the headline is the
    answer's title, and following the tail scrolls it off the top before it has
    been read — the reader ends up at the bottom of something they never saw the
    start of. Arc does the same: text arrives below the fold and you stay put.

    It also removes a real source of flake: where the view settled depended on
    how fast tokens arrived, so the screenshot of a finished answer was never
    the same twice.
   */

  const parsed = useMemo(
    () => parseAnswer(state.answer, state.sources.length),
    [state.answer, state.sources.length],
  );
  const busy =
    state.phase === "searching" || state.phase === "reading" || state.phase === "answering";
  // The reading list stands until the first token lands, which is where Arc
  // swaps its middle screen for the answer.
  const reading = state.phase === "reading" || (state.phase === "answering" && !state.answer);
  const answered = Boolean(
    parsed.headline || parsed.sections.some((s) => s.findings.length || s.rest.length),
  );

  const open = (n: number) => {
    const source = state.sources[n - 1];
    if (source) void openUrl(source.url);
  };

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
        {/* The question, set as the thing that was asked rather than as a
            caption. Arc keeps it in the search field; there is no field here. */}
        <p className="text-[13px] text-fg/45">{query}</p>

        {reading && state.sources.length > 0 && <Reading sources={state.sources} />}

        {answered && (
          <div data-testid="findings">
            {parsed.headline && (
              <h2 className="mt-2 text-[21px] font-bold leading-[1.15] tracking-[-0.02em] text-accent">
                {parsed.headline}
              </h2>
            )}

            {parsed.sections.map((section, s) => (
              <div key={s}>
                {section.heading && (
                  <h3 className="mt-6 text-[16px] font-semibold leading-snug text-fg">
                    {section.heading}
                  </h3>
                )}
                <div className={section.heading ? "mt-2.5 space-y-3" : "mt-4 space-y-3"}>
                  {section.findings.map((finding, i) => (
                    <div key={i} className="grid grid-cols-[1.15rem_1fr] gap-x-2.5">
                      <FindingIcon
                        name={finding.icon}
                        className="mt-[3px] size-[15px] shrink-0 text-accent/75"
                      />
                      <p className="min-w-0 text-[13.5px] leading-relaxed text-fg/80">
                        {finding.label &&
                          (finding.cites[0] ? (
                            // The label opens the source it came from, which is
                            // what makes Arc's labels links rather than headings.
                            <button
                              type="button"
                              onClick={() => open(finding.cites[0]!)}
                              onMouseEnter={() => setLitSource(finding.cites[0] ?? null)}
                              onMouseLeave={() => setLitSource(null)}
                              className="font-semibold text-accent underline decoration-accent/35 underline-offset-2 hover:decoration-accent"
                            >
                              {finding.label}
                            </button>
                          ) : (
                            <span className="font-semibold text-fg">{finding.label}</span>
                          ))}
                        {finding.label && <span className="text-fg/40"> — </span>}
                        <InlineText text={finding.detail} onOpenSource={open} />
                        {finding.cites.map((n) => (
                          <Cite key={n} n={n} source={state.sources[n - 1]} onHover={setLitSource} />
                        ))}
                      </p>
                    </div>
                  ))}
                  {section.rest.length > 0 && (
                    <Answer
                      text={section.rest.join("\n\n")}
                      onOpenSource={open}
                      className="text-[13.5px] leading-relaxed text-fg/80"
                    />
                  )}
                </div>

                {/* Arc drops a card strip under the first group, before the
                    answer carries on. One strip only: a second reads as an ad
                    break rather than as evidence. */}
                {s === 0 && state.sources.length > 0 && <Cards sources={state.sources} />}
              </div>
            ))}
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
    <div className="mt-4" data-testid="reading">
      <p className="text-[15px] font-semibold text-accent">Reading {sources.length} web pages</p>
      <ul className="mt-2 space-y-1">
        {sources.map((source) => (
          <li key={source.url} className="flex items-center gap-2.5">
            <Favicon host={hostOf(source.url)} size={15} />
            <span className="truncate text-[13px] text-fg/45">{hostOf(source.url)}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/**
 * The card strip Arc puts inside the answer.
 *
 * Horizontal rather than wrapped: a strip that wraps is a grid, and a grid of
 * six sources competes with the answer instead of sitting under it.
 */
function Cards({ sources }: { sources: SearchHit[] }) {
  return (
    <div className="mt-4 -mx-4 overflow-x-auto px-4 pb-1" data-testid="source-cards">
      <div className="flex w-max gap-2">
        {sources.slice(0, CARDS).map((source) => {
          const host = hostOf(source.url);
          return (
            <button
              key={source.url}
              type="button"
              onClick={() => void openUrl(source.url)}
              title={source.title}
              className="group flex h-[92px] w-[178px] shrink-0 flex-col justify-between rounded-card border border-hairline bg-card p-2.5 text-left transition-colors hover:border-accent/30"
            >
              <span className="line-clamp-3 text-[12px] leading-snug text-fg/80">
                {source.title}
              </span>
              <span className="flex items-center gap-1.5">
                <Favicon host={host} size={13} />
                <span className="truncate text-[11px] text-fg/45">{host}</span>
                <OpenNewWindow
                  aria-hidden
                  className="ms-auto size-3 shrink-0 text-fg/0 transition-colors group-hover:text-fg/40"
                />
              </span>
            </button>
          );
        })}
      </div>
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
              className={`flex w-full items-center gap-2.5 rounded-md px-1.5 py-1 text-left transition-colors ${
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
              <Favicon host={hostOf(source.url)} size={14} />
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
