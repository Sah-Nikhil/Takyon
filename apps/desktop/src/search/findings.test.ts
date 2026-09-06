import { describe, expect, it } from "bun:test";

import { parseAnswer } from "./findings";

const ARC = [
  "HEADLINE: Chiefs beat the Ravens to reach the Super Bowl",
  "- **Final score** — Chiefs 17, Ravens 10. [1][3]",
  "- **Key play** — An interception in the fourth quarter ended the drive. [2]",
  "- **What is next** — Kansas City play in the Super Bowl on 11 February. [1]",
].join("\n");

describe("parseAnswer", () => {
  it("v0.9 reads the headline and the findings under it", () => {
    const answer = parseAnswer(ARC);
    expect(answer.headline).toBe("Chiefs beat the Ravens to reach the Super Bowl");
    expect(answer.sections[0]!.findings).toHaveLength(3);
    expect(answer.sections[0]!.findings[0]?.label).toBe("Final score");
    expect(answer.sections[0]!.findings[0]?.detail).toBe("Chiefs 17, Ravens 10.");
  });

  // The numbers are the whole point of asking for them: each one is a link.
  it("v0.9 pulls the cited source numbers off each finding", () => {
    const answer = parseAnswer(ARC);
    expect(answer.sections[0]!.findings[0]?.cites).toEqual([1, 3]);
    expect(answer.sections[0]!.findings[1]?.cites).toEqual([2]);
    // And takes them out of the detail text, since they render as chips.
    expect(answer.sections[0]!.findings[0]?.detail).not.toContain("[1]");
  });

  /*
    Every prefix of this is rendered while it streams. A half-written line must
    show as itself rather than vanishing until its dash arrives.
   */
  it("v0.9 renders a half-streamed answer without losing text", () => {
    for (let i = 1; i <= ARC.length; i++) {
      const answer = parseAnswer(ARC.slice(0, i));
      const rendered = [
        answer.headline ?? "",
        ...answer.sections[0]!.findings.map((f) => `${f.label}${f.detail}`),
        ...answer.sections[0]!.rest,
      ].join("");
      expect(rendered.length).toBeGreaterThan(0);
    }
  });

  // An Agent that ignores the shape still has to be readable, or a format the
  // model drifted from becomes a blank surface.
  it("v0.9 falls back to plain paragraphs when the shape is not followed", () => {
    const answer = parseAnswer("Just a paragraph.\n\nAnd another one.");
    expect(answer.headline).toBeUndefined();
    expect(answer.sections[0]!.findings).toHaveLength(0);
    expect(answer.sections[0]!.rest).toEqual(["Just a paragraph.", "And another one."]);
  });

  // A bullet without the bold label is still a finding, just an unlabelled one.
  it("v0.9 keeps an unlabelled bullet rather than dropping it", () => {
    const answer = parseAnswer("- The sources do not say. [2]");
    expect(answer.sections[0]!.findings).toHaveLength(1);
    expect(answer.sections[0]!.findings[0]?.label).toBeUndefined();
    expect(answer.sections[0]!.findings[0]?.detail).toBe("The sources do not say.");
    expect(answer.sections[0]!.findings[0]?.cites).toEqual([2]);
  });

  /*
    A citation the model wrote mid-sentence is part of the sentence. Lifting it
    out leaves "— calls the fumble a muffed catch; scores it a fumble", which is
    prose with its subjects deleted.
   */
  it("v0.9 only lifts the citations that trail the line", () => {
    const answer = parseAnswer(
      "- **Sources disagree** — [2] calls it a muffed catch; [3] scores it a fumble. [2][3]",
    );
    expect(answer.sections[0]!.findings[0]?.detail).toBe(
      "[2] calls it a muffed catch; [3] scores it a fumble.",
    );
    expect(answer.sections[0]!.findings[0]?.cites).toEqual([2, 3]);
  });

  it("v0.9 ignores a citation number with no source behind it", () => {
    const answer = parseAnswer("- **X** — y. [9]", 3);
    expect(answer.sections[0]!.findings[0]?.cites).toEqual([]);
  });
});

/**
 * v0.10 (ADR-0022): Arc's shape adds three things to a finding — a section it
 * belongs to, an icon naming its kind, and links inside the prose that open a
 * source rather than only trailing numbers.
 */
describe("v0.10 Arc shape", () => {
  it("splits an answer into sections at ## headings", () => {
    const parsed = parseAnswer(
      [
        "HEADLINE: Sunny side up versus omelette",
        "- {egg} **Sunny side up** — Cooked one side only. [1]",
        "## Cooking techniques",
        "- {fire} **Over easy** — Flipped briefly. [2]",
      ].join("\n"),
      2,
    );
    expect(parsed.headline).toBe("Sunny side up versus omelette");
    expect(parsed.sections).toHaveLength(2);
    expect(parsed.sections[0]?.heading).toBeUndefined();
    expect(parsed.sections[0]?.findings).toHaveLength(1);
    expect(parsed.sections[1]?.heading).toBe("Cooking techniques");
    expect(parsed.sections[1]?.findings[0]?.label).toBe("Over easy");
  });

  it("takes the icon token off the front of a finding", () => {
    const parsed = parseAnswer("- {score} **Final score** — Kansas City 17. [1]", 1);
    const finding = parsed.sections[0]?.findings[0];
    expect(finding?.icon).toBe("score");
    expect(finding?.label).toBe("Final score");
    expect(finding?.detail).toBe("Kansas City 17.");
  });

  /** An unknown token must not survive into the prose as literal braces. */
  it("drops an unrecognised icon token rather than rendering it", () => {
    const parsed = parseAnswer("- {wharrgarbl} **Label** — Detail. [1]", 1);
    const finding = parsed.sections[0]?.findings[0];
    expect(finding?.icon).toBeUndefined();
    expect(finding?.detail).toBe("Detail.");
    expect(finding?.label).toBe("Label");
  });

  /** A finding with no token at all is still a finding. */
  it("still reads a finding written without an icon", () => {
    const parsed = parseAnswer("- **Label** — Detail. [1]", 1);
    expect(parsed.sections[0]?.findings[0]?.label).toBe("Label");
  });
});
