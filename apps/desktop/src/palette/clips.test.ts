import { describe, expect, test } from "bun:test";
import type { ClipRow } from "@takyon/shared";
import { formatBytes, neighbourAfterDelete } from "./clips";

const row = (id: number): ClipRow => ({ id, createdAt: 0, kind: "text", len: 1, preview: "x" });

describe("clipboard history helpers", () => {
  test("v0.14 sizes read as Explorer writes them", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2 KB");
    expect(formatBytes(1_482_113)).toBe("1.4 MB");
    expect(formatBytes(3 * 1024 ** 3)).toBe("3.0 GB");
  });

  /** Mid-cleanup the user keeps their place: below first, then above. */
  test("v0.14 deleting selects the row below, else the one above", () => {
    const rows = [row(1), row(2), row(3)];
    expect(neighbourAfterDelete(rows, 2)).toBe(3);
    expect(neighbourAfterDelete(rows, 3)).toBe(2);
    expect(neighbourAfterDelete([row(1)], 1)).toBeNull();
    expect(neighbourAfterDelete(rows, 9)).toBeNull();
  });
});
