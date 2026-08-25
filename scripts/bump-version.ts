#!/usr/bin/env bun
/**
 * Set the product version everywhere it is written down.
 *
 * Six files carry it, and a partial bump is worse than none: the installer is
 * named from `tauri.conf.json` while the binary reports the `Cargo.toml` version,
 * so they would silently disagree. Mirrors tesseract's `scripts/bump-version.ts`.
 *
 * Usage: bun run bump 0.1.1
 */

import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { ROOT } from "./lib/release-utils";

const next = process.argv[2];
if (!next || !/^\d+\.\d+\.\d+$/.test(next)) {
  console.error("Usage: bun run bump <major.minor.patch>   e.g. bun run bump 0.1.1");
  process.exit(1);
}

/** `brand` is versioned independently and deliberately left alone. */
const JSON_FILES = [
  "package.json",
  "apps/desktop/package.json",
  "packages/shared/package.json",
  "apps/desktop/src-tauri/tauri.conf.json",
];

/** Only the first `version = "..."`, which is the `[package]` one. A blind
 *  replace would rewrite every dependency pin in the file. */
const TOML_FILES = [
  "apps/desktop/src-tauri/Cargo.toml",
  "apps/desktop/src-tauri/uiaccess/Cargo.toml",
];

let previous: string | null = null;

for (const rel of JSON_FILES) {
  const path = join(ROOT, rel);
  const text = readFileSync(path, "utf-8");
  // Edited as text, not via JSON.parse + stringify: round-tripping would reformat
  // the whole file and bury the one-line change in a diff nobody can review.
  const updated = text.replace(/("version"\s*:\s*")([^"]+)(")/, (_m, a, old, b) => {
    previous ??= old;
    return `${a}${next}${b}`;
  });
  if (updated === text) {
    console.error(`No "version" field found in ${rel}`);
    process.exit(1);
  }
  writeFileSync(path, updated);
  console.log(`  ${rel}`);
}

for (const rel of TOML_FILES) {
  const path = join(ROOT, rel);
  const text = readFileSync(path, "utf-8");
  const updated = text.replace(/^(version\s*=\s*")([^"]+)(")/m, (_m, a, _old, b) => `${a}${next}${b}`);
  if (updated === text) {
    console.error(`No package version found in ${rel}`);
    process.exit(1);
  }
  writeFileSync(path, updated);
  console.log(`  ${rel}`);
}

console.log(`\n${previous ?? "?"} -> ${next}`);
console.log("Cargo.lock updates itself on the next build.");
console.log("\nNext: bun run release");
