/**
 * The inline marks an Agent's answer actually uses, as spans (v0.9).
 *
 * Split from `Answer.tsx` so it can be tested against the case that matters:
 * **every prefix of a streamed answer is rendered at least once**, so a mark
 * that has not closed yet must render as the characters that arrived rather
 * than swallowing the rest while it waits for a closer.
 *
 * Not a markdown parser. Bold, italic and inline code, in that order of
 * precedence, and nothing else.
 */

export type SpanKind = "text" | "bold" | "italic" | "code" | "link";

export interface Span {
  kind: SpanKind;
  /** The content, with its marks already removed. */
  text: string;
  /**
   * For `link`, the source number it opens (ADR-0022). Always an index into the
   * list Rust already checked — a target that is not a number is left as literal
   * text, so a model that invents a URL here cannot put one on screen.
   */
  target?: number;
}

/** `[text](3)`, where the target is a source number and never a URL. */
const LINK = /^\[([^\]\n]{1,120})\]\((\d{1,3})\)/;

/** Openers, longest first: `**` has to beat `*` or bold renders as italic. */
const MARKS: ReadonlyArray<{ open: string; kind: SpanKind }> = [
  { open: "**", kind: "bold" },
  { open: "`", kind: "code" },
  { open: "*", kind: "italic" },
  { open: "_", kind: "italic" },
];

export function parseInline(text: string): Span[] {
  const spans: Span[] = [];
  let plain = "";
  let at = 0;

  const flush = () => {
    if (plain) spans.push({ kind: "text", text: plain });
    plain = "";
  };

  while (at < text.length) {
    if (text[at] === "[") {
      const link = LINK.exec(text.slice(at));
      if (link?.[1] && link[2]) {
        flush();
        spans.push({ kind: "link", text: link[1], target: Number(link[2]) });
        at += link[0].length;
        continue;
      }
      // Not a link: a bare `[2]` citation, a half-arrived one mid-stream, or a
      // target that is not a source number. All of them are literal text.
      plain += "[";
      at += 1;
      continue;
    }

    const mark = MARKS.find((m) => text.startsWith(m.open, at));
    if (!mark) {
      plain += text[at];
      at += 1;
      continue;
    }
    const from = at + mark.open.length;
    const close = text.indexOf(mark.open, from);
    // Unclosed, or empty: literal text. An answer mid-stream is full of these,
    // and the alternative is a paragraph that looks truncated until it closes.
    if (close === -1 || close === from) {
      plain += mark.open;
      at += mark.open.length;
      continue;
    }
    const body = text.slice(from, close);
    /*
      Emphasis has to hug its text, which is CommonMark's flanking rule and the
      reason `2 * 3 * 4` is arithmetic rather than an italic 3. Code spans are
      exempt: `` ` ls ` `` is a command with a space in it. A newline inside a
      mark rules it out the same way.
     */
    const flanked = !/^\s/.test(body) && !/\s$/.test(body) && !body.includes("\n");
    if (mark.kind !== "code" && !flanked) {
      plain += mark.open;
      at += mark.open.length;
      continue;
    }
    flush();
    spans.push({ kind: mark.kind, text: body });
    at = close + mark.open.length;
  }

  flush();
  return spans;
}
