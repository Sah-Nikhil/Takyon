import { describe, expect, test } from "bun:test";

import { BUMP_COMMIT_FILES, nextDevNumber, parseReleaseArgs, releaseDirtyFiles } from "./release-utils";

describe("releaseDirtyFiles", () => {
  test("keeps release notes with the version bump and reports unrelated edits", () => {
    expect(BUMP_COMMIT_FILES).toContain("CHANGELOG.md");
    expect(releaseDirtyFiles(" M CHANGELOG.md\n M package.json\n M README.md\n")).toEqual(["README.md"]);
  });
});

describe("parseReleaseArgs", () => {
  test("--local builds here and may skip preflight", () => {
    expect(parseReleaseArgs(["--local"])).toEqual({ mode: "local", skipPreflight: false });
    expect(parseReleaseArgs(["--local", "--skip-preflight"])).toEqual({
      mode: "local",
      skipPreflight: true,
    });
  });

  test("--git needs a version and never skips preflight", () => {
    expect(parseReleaseArgs(["--git", "0.11.0"])).toEqual({ mode: "git", version: "0.11.0" });
    expect(() => parseReleaseArgs(["--git"])).toThrow("version");
    expect(() => parseReleaseArgs(["--git", "v0.11"])).toThrow("version");
    expect(() => parseReleaseArgs(["--git", "0.11.0", "--skip-preflight"])).toThrow("preflight");
  });

  /** A command that can push must never run by default. */
  test("no mode, or both, is refused", () => {
    expect(() => parseReleaseArgs([])).toThrow("--local or --git");
    expect(() => parseReleaseArgs(["--local", "--git", "0.11.0"])).toThrow("--local or --git");
  });
});

describe("nextDevNumber", () => {
  test("increments the newest dev number, stepping over merges", () => {
    const log = [
      "Merge pull request #16 from example/cc/single-provider-auto-activate-f1f21c",
      "FIX d0.11.12 DuckDuckGo 202 reads as rate limiting, live test waits it out",
      "FIX d0.11.11 bench reports crash vs hang, stops gating CI and releases",
    ];
    expect(nextDevNumber(log)).toBe("d0.11.13");
  });

  /** "v0.11.1" is a release version, not a dev number: the `d` is the difference. */
  test("ignores release versions in a subject", () => {
    expect(nextDevNumber(["NEW d0.11.6 - v0.11.1 and v0.15 agent plans"])).toBe("d0.11.7");
  });

  test("refuses a history with no dev number rather than guessing", () => {
    expect(() => nextDevNumber(["Create AGENTS.md", "Update README.md"])).toThrow("dev number");
  });
});
