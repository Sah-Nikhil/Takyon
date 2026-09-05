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

/** `**bold**`, `*italic*`, `_italic_`, `` `code` ``, in one pass. */
const INLINE = /(\*\*[^*]+\*\*|\*[^*\n]+\*|_[^_\n]+_|`[^`\n]+`)/g;

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
  return text.split(INLINE).map((part, i) => {
    if (part.startsWith("**") && part.endsWith("**") && part.length > 4) {
      return (
        <strong key={i} className="font-semibold text-fg">
          {part.slice(2, -2)}
        </strong>
      );
    }
    if (
      ((part.startsWith("*") && part.endsWith("*")) ||
        (part.startsWith("_") && part.endsWith("_"))) &&
      part.length > 2
    ) {
      return (
        <em key={i} className="italic">
          {part.slice(1, -1)}
        </em>
      );
    }
    if (part.startsWith("`") && part.endsWith("`") && part.length > 2) {
      return (
        <code
          key={i}
          className="rounded-[0.25rem] bg-control px-1 py-px font-mono text-[0.92em]"
        >
          {part.slice(1, -1)}
        </code>
      );
    }
    // `whitespace-pre-wrap` on the paragraph would swallow the leading indent of
    // a wrapped line, so single newlines are kept as breaks here instead.
    return part.split("\n").flatMap((line, j) =>
      j === 0 ? [line] : [<br key={`${i}-${j}`} />, line],
    );
  });
}
