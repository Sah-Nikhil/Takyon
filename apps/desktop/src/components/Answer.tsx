/**
 * An Agent's answer, rendered (v0.9).
 *
 * Agents write markdown whatever they are asked for, and the answer used to be
 * drawn as plain text, so `**August 2, 2027**` arrived with its asterisks. This
 * renders the inline marks an answer actually uses — bold, italic, inline code —
 * and paragraph breaks. Nothing else: no headings, no links, no lists.
 *
 * **Never HTML.** Every node here is React text; there is no `innerHTML` on this
 * path. Answer text comes from a model that has just read pages off the open
 * web (`docs/tbd/v0.9.md` §9), so it is content, never markup.
 */

import { type ReactNode } from "react";

import { parseInline } from "./answerText";

export function Answer({ text, className = "" }: { text: string; className?: string }) {
  // Blank-line separated, so an answer keeps the shape it was written in and a
  // streamed half-paragraph still renders as one.
  const paragraphs = text.split(/\n{2,}/);
  return (
    <div className={className}>
      {paragraphs.map((paragraph, i) => (
        <p key={i} className={i === 0 ? "" : "mt-3"}>
          {inline(paragraph)}
        </p>
      ))}
    </div>
  );
}

/** One paragraph's inline marks. Single newlines survive as line breaks. */
function inline(text: string): ReactNode[] {
  return parseInline(text).map((span, i) => {
    switch (span.kind) {
      case "bold":
        return (
          <strong key={i} className="font-semibold text-fg">
            {span.text}
          </strong>
        );
      case "italic":
        return (
          <em key={i} className="italic">
            {span.text}
          </em>
        );
      case "code":
        return (
          <code key={i} className="rounded-[0.25rem] bg-control px-1 py-px font-mono text-[0.92em]">
            {span.text}
          </code>
        );
      default:
        // `whitespace-pre-wrap` would swallow a wrapped line's indent, so single
        // newlines become breaks here instead.
        return span.text
          .split("\n")
          .flatMap((line, j) => (j === 0 ? [line] : [<br key={`${i}-${j}`} />, line]));
    }
  });
}
