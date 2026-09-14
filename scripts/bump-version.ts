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
import { ROOT, VERSION_JSON_FILES as JSON_FILES, VERSION_TOML_FILES as TOML_FILES } from "./lib/release-utils";

const next = process.argv[2];
if (!next || !/^\d+\.\d+\.\d+$/.test(next)) {
  console.error("Usage: bun run bump <major.minor.patch>   e.g. bun run bump 0.1.1");
  process.exit(1);
}

let previous: string | null = null;

for (const rel of JSON_FILES) {
  const path = join(ROOT, rel);
  const text = readFileSync(path, "utf-8");
  // Edited as text, not via JSON.parse + stringify: round-tripping would reformat
  // the whole file and bury the one-line change in a diff nobody can review.
  const pattern = /("version"\s*:\s*")([^"]+)(")/;
  // Matched, not changed: a file already at `next` is a re-run, not a missing field.
  if (!pattern.test(text)) {
    console.error(`No "version" field found in ${rel}`);
    process.exit(1);
  }
  const updated = text.replace(pattern, (_m, a, old, b) => {
    previous ??= old;
    return `${a}${next}${b}`;
  });
  writeFileSync(path, updated);
  console.log(`  ${rel}`);
}

for (const rel of TOML_FILES) {
  const path = join(ROOT, rel);
  const text = readFileSync(path, "utf-8");
  const pattern = /^(version\s*=\s*")([^"]+)(")/m;
  if (!pattern.test(text)) {
    console.error(`No package version found in ${rel}`);
    process.exit(1);
  }
  writeFileSync(path, text.replace(pattern, (_m, a, _old, b) => `${a}${next}${b}`));
  console.log(`  ${rel}`);
}

console.log(`\n${previous ?? "?"} -> ${next}`);
console.log("Cargo.lock updates on the next cargo command; commit it with these.");
console.log("\nNext: bun run release --local to build here, or --git <version> to publish.");
