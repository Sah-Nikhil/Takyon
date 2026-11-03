#!/usr/bin/env bun
/**
 * Release-time half of the in-app updater (ADR-0040). Run by `release.yml`.
 *
 *   bun scripts/updater.ts config <out.json>              the `--config` overlay for a signed build
 *   bun scripts/updater.ts verify <installer.exe>         its `.sig` checks out under the pubkey
 *   bun scripts/updater.ts manifest <dir> <version> <notes.md>   latest.json + release notes
 *
 * Reads `TAKYON_UPDATER_PUBKEY` (public, a repository variable) and, for
 * `config`/`manifest`, `GITHUB_REPOSITORY`. Never reads or prints the private key:
 * that reaches `tauri build` alone, as `TAURI_SIGNING_PRIVATE_KEY`.
 */

import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { basename, join, resolve } from "node:path";

import { manifest, releaseNotes, updaterConfig, verifySignature } from "./lib/updater";

const ROOT = resolve(import.meta.dir, "..");

function fail(message: string): never {
  console.error(`updater: ${message}`);
  process.exit(1);
}

function env(name: string): string {
  const value = process.env[name]?.trim();
  if (!value) fail(`${name} is not set. See docs/verify/updater.md, section S.`);
  return value;
}

/** Throws on a malformed key or signature; `false` means a real mismatch. */
function check(installer: string): string {
  const signature = readFileSync(`${installer}.sig`, "utf8").trim();
  let ok: boolean;
  try {
    ok = verifySignature(env("TAKYON_UPDATER_PUBKEY"), signature, readFileSync(installer));
  } catch (e) {
    fail((e as Error).message);
  }
  if (!ok) {
    fail(
      `${basename(installer)}.sig does not verify under TAKYON_UPDATER_PUBKEY. ` +
        "The signing secret and the public key come from different key pairs; every installed copy would refuse this update.",
    );
  }
  return signature;
}

const [command, first, second, third] = process.argv.slice(2);
const usage = "usage: config <out.json> | verify <installer.exe> | manifest <dir> <version> <notes.md>";

if (command === "config" && first) {
  let config: ReturnType<typeof updaterConfig>;
  try {
    config = updaterConfig({ pubkey: env("TAKYON_UPDATER_PUBKEY"), repository: env("GITHUB_REPOSITORY") });
  } catch (e) {
    fail((e as Error).message);
  }
  writeFileSync(first, `${JSON.stringify(config, null, 2)}\n`);
  console.log(`updater: config written, feed ${config.plugins.updater.endpoints[0]}`);
} else if (command === "verify" && first) {
  check(first);
  console.log(`updater: ${basename(first)} signature verified`);
} else if (command === "manifest" && first && second && third) {
  publish(first, second, third);
} else {
  fail(usage);
}

/** latest.json beside the installer, and the notes the release body uses. */
function publish(dir: string, version: string, notesOut: string) {
  const installers = readdirSync(dir).filter((f) => f.endsWith("-setup.exe"));
  const name = installers[0];
  if (installers.length !== 1 || !name) fail(`expected one *-setup.exe in ${dir}, found ${installers.length}`);
  if (!name.includes(`_${version}_`)) fail(`${name} is not the v${version} installer`);

  const signature = check(join(dir, name));
  let notes: string;
  try {
    notes = releaseNotes(readFileSync(join(ROOT, "CHANGELOG.md"), "utf8"), version);
  } catch (e) {
    fail((e as Error).message);
  }
  const repository = env("GITHUB_REPOSITORY");
  const url = `https://github.com/${repository}/releases/download/v${version}/${encodeURIComponent(name)}`;
  const latest = manifest({ version, notes, pubDate: new Date(), url, signature });

  writeFileSync(join(dir, "latest.json"), `${JSON.stringify(latest, null, 2)}\n`);
  writeFileSync(notesOut, `${notes}\n`);
  console.log(`updater: latest.json for v${version} -> ${name}`);
}
