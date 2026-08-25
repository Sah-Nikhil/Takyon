/**
 * Shared helpers for the local release scripts. Mirrors tesseract's
 * `scripts/lib/release-utils.ts`, so the two repos release the same way.
 */

import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

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
      console.error(`\n${step} failed (exit ${code}). Nothing was built.`);
      process.exit(code);
    }
  }
}
