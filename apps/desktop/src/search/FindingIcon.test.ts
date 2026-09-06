import { describe, expect, it } from "bun:test";

import { FINDING_ICONS } from "./FindingIcon";
import { ICONS } from "./findings";

/**
 * The icon vocabulary is written down three times (ADR-0022): here, in the
 * parser, and in the prompt `synth.rs` sends. Two of those are code and can be
 * held together; the third is prose an Agent reads, and its only failure is
 * asking for a token that falls back to the neutral glyph.
 */
describe("the finding icon vocabulary", () => {
  it("has a glyph for every token the parser accepts", () => {
    const missing = [...ICONS].filter((name) => !FINDING_ICONS[name]);
    expect(missing).toEqual([]);
  });

  it("has no glyph the parser would throw away", () => {
    const orphans = Object.keys(FINDING_ICONS).filter((name) => !ICONS.has(name));
    expect(orphans).toEqual([]);
  });

  /** A vocabulary this small is only useful if it covers the common shapes. */
  it("covers the kinds the prompt names explicitly", () => {
    for (const required of ["score", "disagree", "unknown", "warning", "money", "time"]) {
      expect(ICONS.has(required)).toBe(true);
    }
  });
});
