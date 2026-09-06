/**
 * An Agent's answer, rendered (v0.9, links added v0.10).
 *
 * Agents write markdown whatever they are asked for, and the answer used to be
 * drawn as plain text, so `**August 2, 2027**` arrived with its asterisks. This
 * renders the inline marks an answer actually uses — bold, italic, inline code,
 * and `[Name](3)` links onto a source — and paragraph breaks. Nothing else: no
 * headings, no lists, no arbitrary URLs.
 *
 * **Never HTML.** Every node here is React text; there is no `innerHTML` on this
 * path. Answer text comes from a model that has just read pages off the open
 * web (`docs/tbd/v0.9.md` §9), so it is content, never markup.
 *
 * **A link target is a number, never a URL.** `answerText.ts` refuses anything
 * else, so a model that invents an address cannot put one on screen and the
 * click can only ever open a source Rust already fetched.
 */

import { type ReactNode } from "react";

import { parseInline } from "./answerText";

export function Answer({
  text,
  className = "",
  onOpenSource,
}: {
  text: string;
  className?: string;
  onOpenSource?: (n: number) => void;
}) {
  // Blank-line separated, so an answer keeps the shape it was written in and a
  // streamed half-paragraph still renders as one.
  const paragraphs = text.split(/\n{2,}/);
  return (
    <div className={className}>
      {paragraphs.map((paragraph, i) => (
        <p key={i} className={i === 0 ? "" : "mt-3"}>
          <InlineText text={paragraph} onOpenSource={onOpenSource} />
        </p>
      ))}
    </div>
  );
}

/**
 * One run of inline marks, without a paragraph around it.
 *
 * Exported because a finding's detail is inline text inside a line that already
 * has a label and its citations in it, so it cannot be wrapped in a `<p>`.
 */
export function InlineText({
  text,
  onOpenSource,
}: {
  text: string;
  onOpenSource?: (n: number) => void;
}): ReactNode {
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
      case "link":
        // Without a handler the mark is decoration, so it renders as its own
        // text rather than as something that looks clickable and is not.
        if (!onOpenSource || span.target === undefined) {
          return <span key={i}>{span.text}</span>;
        }
        return (
          <button
            key={i}
            type="button"
            onClick={() => onOpenSource(span.target!)}
            className="text-accent underline decoration-accent/35 underline-offset-2 hover:decoration-accent"
          >
            {span.text}
          </button>
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
