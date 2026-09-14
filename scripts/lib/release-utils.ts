/**
 * Shared helpers for the local release scripts. Mirrors tesseract's
 * `scripts/lib/release-utils.ts`, so the two repos release the same way.
 */

import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

/** Files `bun run bump` rewrites. `brand` is versioned apart and left alone. */
export const VERSION_JSON_FILES = [
  "package.json",
  "apps/desktop/package.json",
  "packages/shared/package.json",
  "apps/desktop/src-tauri/tauri.conf.json",
];

/** Only the `[package]` version in each; see `bump-version.ts`. */
export const VERSION_TOML_FILES = [
  "apps/desktop/src-tauri/Cargo.toml",
  "apps/desktop/src-tauri/uiaccess/Cargo.toml",
];

/** Everything a version bump commit holds: the six files plus the lock they change. */
export const BUMP_COMMIT_FILES = [
  ...VERSION_JSON_FILES,
  ...VERSION_TOML_FILES,
  "apps/desktop/src-tauri/Cargo.lock",
];

export type ReleaseArgs =
  | { mode: "local"; skipPreflight: boolean }
  | { mode: "git"; version: string };

/**
 * `--local` builds the installer here; `--git <version>` bumps, checks, commits,
 * pushes and tags so CI publishes. No default: a command that can push never runs
 * because someone forgot a flag.
 */
export function parseReleaseArgs(argv: string[]): ReleaseArgs {
  const local = argv.includes("--local");
  const git = argv.includes("--git");
  if (local === git) throw new Error("Pass exactly one of --local or --git <version>.");
  const skipPreflight = argv.includes("--skip-preflight");
  if (local) return { mode: "local", skipPreflight };

  const version = argv[argv.indexOf("--git") + 1];
  if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
    throw new Error("--git needs a version: bun run release --git 0.11.0");
  }
  if (skipPreflight) throw new Error("--git never skips preflight: CI would refuse a red tree anyway.");
  return { mode: "git", version };
}

/**
 * The next `d<phase>.<n>` from commit subjects, newest first (CLAUDE.md, Git).
 * Merges and plain subjects are stepped over; no dev number at all is refused.
 */
export function nextDevNumber(subjects: string[]): string {
  for (const subject of subjects) {
    const match = subject.match(/\bd(\d+)\.(\d+)\.(\d+)\b/);
    if (match) return `d${match[1]}.${match[2]}.${Number(match[3]) + 1}`;
  }
  throw new Error("No dev number in recent history; write the bump commit by hand.");
}

/** Spawns `cmd` with stdio inherited, so build output is live. Returns its code. */
export function runInherit(cmd: string[], cwd: string): number {
  const proc = Bun.spawnSync(cmd, { cwd, stdout: "inherit", stderr: "inherit", stdin: "inherit" });
  return proc.exitCode ?? 1;
}

/**
 * Typecheck, lint and test before a release build starts.
 *
 * All three, matching CLAUDE.md's definition of done — tesseract's preflight runs
 * two because its lint is folded elsewhere. A release built from a red tree is the
 * one artefact you cannot take back once it is installed somewhere.
 */
export function preflight(skip: boolean) {
  if (skip) {
    console.log("Skipping preflight (--skip-preflight).\n");
    return;
  }

  for (const step of ["typecheck", "lint", "test"]) {
    console.log(`\nPreflight: bun run ${step} ...\n`);
    const code = runInherit(["bun", "run", step], ROOT);
    if (code !== 0) {
      console.error(`\n${step} failed (exit ${code}). Nothing was built, committed or pushed.`);
      process.exit(code);
    }
  }
}
