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
    expect(answer.findings).toHaveLength(3);
    expect(answer.findings[0]?.label).toBe("Final score");
    expect(answer.findings[0]?.detail).toBe("Chiefs 17, Ravens 10.");
  });

  // The numbers are the whole point of asking for them: each one is a link.
  it("v0.9 pulls the cited source numbers off each finding", () => {
    const answer = parseAnswer(ARC);
    expect(answer.findings[0]?.cites).toEqual([1, 3]);
    expect(answer.findings[1]?.cites).toEqual([2]);
    // And takes them out of the detail text, since they render as chips.
    expect(answer.findings[0]?.detail).not.toContain("[1]");
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
        ...answer.findings.map((f) => `${f.label}${f.detail}`),
        ...answer.rest,
      ].join("");
      expect(rendered.length).toBeGreaterThan(0);
    }
  });

  // An Agent that ignores the shape still has to be readable, or a format the
  // model drifted from becomes a blank surface.
  it("v0.9 falls back to plain paragraphs when the shape is not followed", () => {
    const answer = parseAnswer("Just a paragraph.\n\nAnd another one.");
    expect(answer.headline).toBeUndefined();
    expect(answer.findings).toHaveLength(0);
    expect(answer.rest).toEqual(["Just a paragraph.", "And another one."]);
  });

  // A bullet without the bold label is still a finding, just an unlabelled one.
  it("v0.9 keeps an unlabelled bullet rather than dropping it", () => {
    const answer = parseAnswer("- The sources do not say. [2]");
    expect(answer.findings).toHaveLength(1);
    expect(answer.findings[0]?.label).toBeUndefined();
    expect(answer.findings[0]?.detail).toBe("The sources do not say.");
    expect(answer.findings[0]?.cites).toEqual([2]);
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
    expect(answer.findings[0]?.detail).toBe(
      "[2] calls it a muffed catch; [3] scores it a fumble.",
    );
    expect(answer.findings[0]?.cites).toEqual([2, 3]);
  });

  it("v0.9 ignores a citation number with no source behind it", () => {
    const answer = parseAnswer("- **X** — y. [9]", 3);
    expect(answer.findings[0]?.cites).toEqual([]);
  });
});
