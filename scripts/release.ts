#!/usr/bin/env bun
/**
 * Cut a release, one of two ways.
 *
 *   bun run release --local [--skip-preflight]   installer into `releases/v{version}/`
 *   bun run release --git 0.11.0                  bump, preflight, commit, push, tag
 *
 * `--git` builds nothing here: the pushed `v*` tag starts `release.yml`, which waits
 * for CI on that commit and publishes the installer. It commits and pushes, so an
 * agent never runs it (CLAUDE.md, Git).
 */

import { copyFileSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { join } from "node:path";
import {
  BUMP_COMMIT_FILES,
  ROOT,
  nextDevNumber,
  parseReleaseArgs,
  preflight,
  runInherit,
  type ReleaseArgs,
} from "./lib/release-utils";

let args: ReleaseArgs;
try {
  args = parseReleaseArgs(process.argv.slice(2));
} catch (e) {
  console.error((e as Error).message);
  console.error("\n  bun run release --local [--skip-preflight]\n  bun run release --git <major.minor.patch>");
  process.exit(1);
}

if (args.mode === "local") local(args.skipPreflight);
else await git(args.version);

function local(skipPreflight: boolean) {
  preflight(skipPreflight);

  /*
    `tauri build`, never a bare `cargo build --release`: that exe launches with a dead
    frontend. `tauri build` runs `beforeBuildCommand` and sets the TAURI_ENV_* asset
    embedding needs (CLAUDE.md, Gotchas).
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
  // This build's exact name: the directory keeps every installer ever built here.
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

  /*
    Written beside the installer, not only printed. Two spaces is `sha256sum`
    format, so `sha256sum -c` verifies it. Not a reproducibility check: Rust
    release builds are not bit-identical, so a rebuild hashes differently.
  */
  writeFileSync(join(releaseDir, "SHA256SUMS.txt"), `${hash}  ${name}\n`);

  console.log(`\nInstaller ready: releases/v${version}/${name}`);
  console.log(`  ${(bytes.length / 1024 / 1024).toFixed(1)} MB`);
  console.log(`  SHA-256: ${hash}`);
  console.log(`  recorded in releases/v${version}/SHA256SUMS.txt`);
  // No `latest.json` or `.sig`: an unsigned manifest would pass for an update feed.
  console.log("\nNo latest.json or .sig — the updater is a v1.0 item.");
  console.log("The helper is also unsigned, so the Palette will not appear over");
  console.log("elevated windows. See docs/plans/uiaccess-signing.md.");
}

/** Run a command for its output. A failure stops the release with what it said. */
function capture(cmd: string[]): string {
  const proc = Bun.spawnSync(cmd, { cwd: ROOT, stdout: "pipe", stderr: "pipe" });
  if (proc.exitCode !== 0) fail(`${cmd.join(" ")} failed:\n${proc.stderr.toString().trim()}`);
  return proc.stdout.toString().trim();
}

function step(cmd: string[], recovery?: string) {
  console.log(`\n> ${cmd.join(" ")}`);
  if (runInherit(cmd, ROOT) !== 0) fail(`${cmd.join(" ")} failed.`, recovery);
}

function fail(message: string, recovery?: string): never {
  console.error(`\n${message}`);
  if (recovery) console.error(`\n${recovery}`);
  process.exit(1);
}

async function git(version: string) {
  const tag = `v${version}`;

  // Everything that can refuse, refused before anything is written.
  capture(["gh", "auth", "status"]);
  const repo = capture(["gh", "repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"]);
  if (capture(["git", "branch", "--show-current"]) !== "main") fail("Release from main.");
  step(["git", "fetch", "origin", "main", "--tags"]);
  if (capture(["git", "rev-parse", "HEAD"]) !== capture(["git", "rev-parse", "origin/main"])) {
    fail("main is not level with origin/main. Pull or push first, so the tag lands on what CI tested.");
  }
  const dirty = capture(["git", "status", "--porcelain"])
    .split("\n")
    .filter(Boolean)
    .map((line) => line.slice(3).trim())
    .filter((file) => !BUMP_COMMIT_FILES.includes(file));
  if (dirty.length > 0) fail(`Uncommitted changes outside the version files:\n  ${dirty.join("\n  ")}`);
  if (capture(["git", "tag", "--list", tag]) || capture(["git", "ls-remote", "--tags", "origin", tag])) {
    fail(`${tag} already exists.`);
  }

  step(["bun", "run", "bump", version]);
  // After the bump: tests read the new version, and lint's cargo run rewrites Cargo.lock.
  preflight(false);

  const subjects = capture(["git", "log", "--format=%s", "-n", "200"]).split("\n");
  const message = `UPDATE ${nextDevNumber(subjects)} version ${version}`;
  step(["git", "add", "--", ...BUMP_COMMIT_FILES]);
  step(["git", "commit", "-m", message]);
  step(["git", "push", "origin", "main"], "The bump is committed locally. Fix the push, then rerun from `git push origin main`.");

  // release.yml refuses a tag whose commit has no CI run, so wait for it to exist.
  const sha = capture(["git", "rev-parse", "HEAD"]);
  const deadline = Date.now() + 3 * 60_000;
  for (;;) {
    const runs = capture(["gh", "api", `repos/${repo}/commits/${sha}/check-runs`, "--jq", ".total_count"]);
    if (Number(runs) > 0) break;
    if (Date.now() > deadline) fail(`No CI run appeared on ${sha} within 3 minutes.`, `Once one has, run: git tag -a ${tag} -m "Takyon ${version}" && git push origin ${tag}`);
    await Bun.sleep(5_000);
  }

  step(["git", "tag", "-a", tag, "-m", `Takyon ${version}`]);
  step(["git", "push", "origin", tag], `The tag exists locally. Retry: git push origin ${tag}`);

  console.log(`\n${message} pushed, ${tag} pushed.`);
  console.log(`release.yml waits for CI on ${sha.slice(0, 7)}, then publishes:`);
  console.log(`  https://github.com/${repo}/actions/workflows/release.yml`);
}
