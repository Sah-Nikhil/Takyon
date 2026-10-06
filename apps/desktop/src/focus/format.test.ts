import { describe, expect, test } from "bun:test";
import type { FocusCategory } from "@takyon/shared";
import { clock, contentsText, keyLabel, presetMinutes, refusalLines } from "./format";

describe("clock", () => {
  test("minutes and seconds under an hour", () => {
    expect(clock(23 * 60_000 + 41_000)).toBe("23:41");
    expect(clock(5_000)).toBe("0:05");
  });
  test("hours once there are any", () => {
    expect(clock(65 * 60_000 + 9_000)).toBe("1:05:09");
  });
  test("rounds up so the last second reads 0:01, never 0:00 early", () => {
    expect(clock(500)).toBe("0:01");
    expect(clock(0)).toBe("0:00");
    expect(clock(-10)).toBe("0:00");
  });
});

describe("keyLabel", () => {
  const categories: FocusCategory[] = [
    { id: "social", name: "Social", icon: "UsersThree", items: [{ key: "site:x.com", label: "X" }] },
  ];
  test("a known item uses its category label", () => {
    expect(keyLabel("site:x.com", categories)).toBe("X");
  });
  test("unknown keys read as what they name", () => {
    expect(keyLabel("site:youtube.com", [])).toBe("youtube.com");
    expect(keyLabel("exe:discord.exe", [])).toBe("discord");
    expect(keyLabel("entry:c:\\apps\\slack\\slack.exe|--x", [])).toBe("slack");
    expect(keyLabel("entry:steam:730", [])).toBe("steam:730");
    expect(keyLabel("package:5319275a.whatsappdesktop_cv1g1gvanyjgm", [])).toBe("whatsappdesktop");
  });
});

test("presets cover a sprint to a long block", () => {
  expect(presetMinutes).toEqual([25, 45, 60, 120]);
});

describe("refusalLines", () => {
  test("protected keys share one line; each other refusal gets its own", () => {
    expect(
      refusalLines(
        [
          { kind: "protected", key: "exe:explorer.exe" },
          { kind: "protected", key: "exe:takyon.exe" },
          { kind: "elevated", key: "exe:discord.exe", name: "Discord" },
          { kind: "browser", name: "Opera GX", elevated: false },
          { kind: "browser", name: "Google Chrome", elevated: true },
        ],
        [],
      ),
    ).toEqual([
      "Can't block explorer, takyon: part of Windows or Takyon itself.",
      "Can't block Discord while it runs as administrator. Reopen it normally first.",
      "Can't block sites in Opera GX: Focus can't read its address bar.",
      "Can't block sites in Google Chrome while it runs as administrator.",
    ]);
  });
  test("nothing refused, no lines", () => {
    expect(refusalLines([], [])).toEqual([]);
  });
});

describe("contentsText", () => {
  const c = (labels: string[], dynamic: string | null = null): FocusCategory => ({
    id: "x",
    name: "X",
    icon: "Star",
    dynamic,
    items: labels.map((label, i) => ({ key: `site:s${i}.com`, label })),
  });
  test("names the first few, once each, and counts the rest", () => {
    expect(contentsText(c(["X", "Twitter", "Threads", "Threads", "Reddit", "TikTok"]))).toBe(
      "X, Twitter, Threads and 2 more",
    );
  });
  test("a short list is named whole", () => {
    expect(contentsText(c(["YouTube", "YouTube"]))).toBe("YouTube");
    expect(contentsText(c(["A", "B"]))).toBe("A and B");
  });
  test("Games says installed games come with it", () => {
    expect(contentsText(c(["Steam"], "games"))).toBe("Steam and every installed game");
  });
  test("empty says so", () => {
    expect(contentsText(c([]))).toBe("Nothing yet");
  });
});
