#!/usr/bin/env bun
/**
 * Build the Windows installer and put it in `releases/v{version}/`.
 *
 * Mirrors tesseract's `scripts/release-desktop.ts`. One target for now, so one
 * script — a macOS build would become `release-desktop.ts` / `release-macos.ts`
 * beside it, which is why the helpers already live in `lib/`.
 *
 * Usage: bun run release [--skip-preflight]
 */

import { copyFileSync, mkdirSync, readFileSync, readdirSync } from "node:fs";
import { createHash } from "node:crypto";
import { join } from "node:path";
import { ROOT, preflight, runInherit } from "./lib/release-utils";

preflight(process.argv.includes("--skip-preflight"));

/*
  `tauri build`, never a bare `cargo build --release`.

  A bare cargo build produces a takyon.exe that launches, registers the hotkey and
  shows the Palette with a completely dead frontend — it fails in the one way that
  looks like a Rust bug. `tauri build` runs `beforeBuildCommand` and sets the
  TAURI_ENV_* the asset embedding depends on; cargo alone does neither.
*/
console.log("\nBuilding the installer (tauri build)...\n");
const code = runInherit(["bun", "--cwd=apps/desktop", "run", "tauri", "build"], ROOT);
if (code !== 0) {
  console.error(`tauri build failed (exit ${code})`);
  process.exit(code);
}

const conf = JSON.parse(
  readFileSync(join(ROOT, "apps/desktop/src-tauri/tauri.conf.json"), "utf-8"),
) as { version: string; productName: string };
const { version, productName } = conf;

const nsisDir = join(ROOT, "apps/desktop/src-tauri/target/release/bundle/nsis");
/*
  Match this build's filename exactly. The bundle directory accumulates every
  installer ever built locally, so "the only *-setup.exe" both fails on the second
  release and could pick up an older one if it happened to sort first.
*/
const name = `${productName}_${version}_x64-setup.exe`;
if (!readdirSync(nsisDir).includes(name)) {
  const found = readdirSync(nsisDir).filter((f) => f.endsWith("-setup.exe"));
  console.error(`Expected ${name} in ${nsisDir}; found: ${found.join(", ") || "(none)"}`);
  process.exit(1);
}

const releaseDir = join(ROOT, "releases", `v${version}`);
mkdirSync(releaseDir, { recursive: true });
const dest = join(releaseDir, name);
copyFileSync(join(nsisDir, name), dest);

const bytes = readFileSync(dest);
const hash = createHash("sha256").update(bytes).digest("hex");

console.log(`\nInstaller ready: releases/v${version}/${name}`);
console.log(`  ${(bytes.length / 1024 / 1024).toFixed(1)} MB`);
console.log(`  SHA-256: ${hash}`);
/*
  No `latest.json` or `.sig` yet, unlike tesseract's releases. Those are the
  updater's manifest and signature, and `tauri-plugin-updater` arrives at v1.0
  (ROADMAP). Writing an unsigned manifest now would look like a working update
  feed and silently be one nothing can verify.
*/
console.log("\nNo latest.json or .sig — the updater is a v1.0 item.");
console.log("The helper is also unsigned, so the Palette will not appear over");
console.log("elevated windows. See docs/plans/uiaccess-signing.md.");
