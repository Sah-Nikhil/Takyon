import { describe, expect, test } from "bun:test";
import type { Entry, EntryKind } from "@takyon/shared";
import { GROUP_LABEL, groupEntries } from "./groups";

function entry(id: string, kind: EntryKind): Entry {
  return { id, title: id, kind, score: 0, actions: [] };
}

describe("grouping the Entry list", () => {
  /**
   * The property the whole feature rests on. If a group could sort ahead of the
   * Entry that beat it, the best answer to a query would stop being the first
   * thing on screen — which is the one thing a launcher may not do.
   */
  test("v0.10 a group sits where its best Entry sat", () => {
    const groups = groupEntries([
      entry("readme", "file"),
      entry("code", "app"),
      entry("notes", "file"),
    ]);
    expect(groups.map((g) => g.kind)).toEqual(["file", "app"]);
    expect(groups[0]!.entries.map((e) => e.id)).toEqual(["readme", "notes"]);
  });

  /** Two runs of one Kind are one section, not two identical headings. */
  test("v0.10 a Kind that reappears joins the group it already has", () => {
    const groups = groupEntries([
      entry("a", "app"),
      entry("f", "file"),
      entry("b", "app"),
    ]);
    expect(groups).toHaveLength(2);
    expect(groups[0]!.entries.map((e) => e.id)).toEqual(["a", "b"]);
  });

  /** Ranked order inside a group is never touched. */
  test("v0.10 the ranked order survives inside a group", () => {
    const groups = groupEntries([entry("1", "app"), entry("2", "app"), entry("3", "app")]);
    expect(groups[0]!.entries.map((e) => e.id)).toEqual(["1", "2", "3"]);
  });

  test("v0.10 nothing in, nothing out", () => {
    expect(groupEntries([])).toEqual([]);
  });

  /**
   * A calculation is a card, not a labelled section, and there is never more
   * than one. It still forms a group so the list can render it in place.
   */
  test("v0.10 a calculation gets no heading", () => {
    const groups = groupEntries([entry("2+2", "calc"), entry("calc.exe", "app")]);
    expect(groups[0]!.label).toBeUndefined();
    expect(groups[1]!.label).toBe("Applications");
  });

  /**
   * Every Kind that can reach the list needs a heading, or Expanded draws an
   * unlabelled section and nobody notices until that Source ships.
   */
  test("v0.10 every Kind but calc has a heading", () => {
    const kinds: EntryKind[] = [
      "app",
      "file",
      "folder",
      "clip",
      "recent",
      "system",
      "systemTask",
      "command",
    ];
    for (const kind of kinds) {
      expect(GROUP_LABEL[kind], `${kind} has no group heading`).toBeTruthy();
    }
    expect(GROUP_LABEL.calc).toBeUndefined();
  });
});
