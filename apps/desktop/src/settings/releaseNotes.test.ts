import { describe, expect, it } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { parseChangelog, parseSection, releaseUrl } from "./releaseNotes";

const SAMPLE = `# Changelog

Preamble that is not a release.

## Unreleased

- **Not shipped.** Never listed.

## v0.12.9 — Focus celebration

- **Focus confetti.** Fires from both lower corners.
- **Wrapped label.** A bullet that runs
  onto a second line.
- An unlabelled bullet.

**Still open.** Hand verification.

## v0.12.8 — Selection

- **Follows the selection.** Arrow keys scroll.

## v0.7.0 and earlier

Older history.
`;

describe("parseChangelog", () => {
  // Only shipped versions: the preamble, Unreleased and the catch-all are not releases.
  it("v0.11.2 lists released versions newest first, skipping Unreleased", () => {
    const notes = parseChangelog(SAMPLE);
    expect(notes.map((n) => n.version)).toEqual(["0.12.9", "0.12.8"]);
    expect(notes[0]!.title).toBe("Focus celebration");
  });

  // The label is what the view sets in bold; a wrapped bullet is one item.
  it("v0.11.2 splits a labelled bullet and joins wrapped lines", () => {
    const [latest] = parseChangelog(SAMPLE);
    expect(latest!.items).toEqual([
      { label: "Focus confetti", text: "Fires from both lower corners." },
      { label: "Wrapped label", text: "A bullet that runs onto a second line." },
      { text: "An unlabelled bullet." },
    ]);
    expect(latest!.notes).toEqual(["**Still open.** Hand verification."]);
  });

  // The real file must parse into labelled entries for the versions shown.
  it("v0.11.2 reads the repository's own CHANGELOG.md", () => {
    const file = readFileSync(join(import.meta.dir, "../../../../CHANGELOG.md"), "utf8");
    const notes = parseChangelog(file);
    expect(notes.length).toBeGreaterThan(6);
    expect(notes.some((n) => n.version === "0.12.9")).toBe(true);
    const recent = notes.find((n) => n.version === "0.12.9")!;
    expect(recent.items.every((i) => i.label)).toBe(true);
  });
});

describe("parseSection", () => {
  it("v0.11.2 reads a feed's notes, which are one section's body", () => {
    const { items } = parseSection("- **Agent fallback.** The next Agent answers.");
    expect(items).toEqual([{ label: "Agent fallback", text: "The next Agent answers." }]);
  });

  it("v0.11.2 links each version to its GitHub release", () => {
    expect(releaseUrl("0.12.9")).toBe("https://github.com/Sah-Nikhil/Takyon/releases/tag/v0.12.9");
  });
});
