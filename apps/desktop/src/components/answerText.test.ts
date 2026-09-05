import { describe, expect, it } from "bun:test";

import { parseInline, type Span } from "./answerText";

const kinds = (spans: Span[]) => spans.map((s) => s.kind);
const texts = (spans: Span[]) => spans.map((s) => s.text);

describe("parseInline", () => {
  it("v0.9 marks bold, italic and code", () => {
    const spans = parseInline("The next **eclipse** is *soon*, near `Luxor`.");
    expect(kinds(spans)).toEqual([
      "text",
      "bold",
      "text",
      "italic",
      "text",
      "code",
      "text",
    ]);
    expect(texts(spans)).toEqual([
      "The next ",
      "eclipse",
      " is ",
      "soon",
      ", near ",
      "Luxor",
      ".",
    ]);
  });

  /*
    The answer arrives one delta at a time, so every prefix of it is rendered at
    least once. A half-written mark must render as the characters that have
    arrived, never swallow the rest of the answer waiting for a closer.
  */
  it("v0.9 renders every prefix of a streamed answer without losing text", () => {
    const full = "The next **total eclipse** is `soon`.";
    for (let i = 1; i <= full.length; i++) {
      const prefix = full.slice(0, i);
      const rendered = parseInline(prefix)
        .map((s) => s.text)
        .join("");
      // Marks are dropped where they are complete, so compare on the letters.
      expect(rendered.replace(/[*`]/g, "")).toBe(prefix.replace(/[*`]/g, ""));
    }
  });

  // An unclosed mark is text. The alternative is an answer that looks truncated.
  it("v0.9 leaves an unbalanced mark as literal text", () => {
    expect(kinds(parseInline("a ** b"))).toEqual(["text"]);
    expect(kinds(parseInline("2 * 3 * 4"))).toEqual(["text"]);
    expect(texts(parseInline("a `unclosed"))).toEqual(["a `unclosed"]);
  });

  // Markdown inside code is not markdown, or a shell snippet loses its stars.
  it("v0.9 does not read marks inside inline code", () => {
    const spans = parseInline("run `ls **/*.rs` now");
    expect(kinds(spans)).toEqual(["text", "code", "text"]);
    expect(spans[1]?.text).toBe("ls **/*.rs");
  });

  it("v0.9 handles empty and mark-only input", () => {
    expect(parseInline("")).toEqual([]);
    expect(texts(parseInline("**"))).toEqual(["**"]);
    expect(texts(parseInline("****"))).toEqual(["****"]);
  });

  // Bold wins over italic on the same run, or `**x**` renders as an italic `*x*`.
  it("v0.9 prefers bold over italic when both could match", () => {
    expect(kinds(parseInline("**x**"))).toEqual(["bold"]);
    expect(texts(parseInline("**x**"))).toEqual(["x"]);
  });
});
