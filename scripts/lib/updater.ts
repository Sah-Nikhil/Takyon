/**
 * The release side of the in-app updater (ADR-0040).
 *
 * `tauri build` signs the installer but writes no `latest.json`, so `release.yml`
 * builds one here. Every check the app makes on a download is repeated on the
 * runner first: a release whose signature fails under the shipped pubkey would
 * publish a feed every installed copy refuses, and nobody would hear about it.
 */

import { createPublicKey, verify } from "node:crypto";

/** DER prefix that turns a raw 32-byte Ed25519 key into SPKI, which node imports. */
const ED25519_SPKI = Buffer.from("302a300506032b6570032100", "hex");

type PublicKey = { keyId: Buffer; key: ReturnType<typeof createPublicKey> };

/** Tauri's pubkey is base64 over minisign's two-line `.pub` text. */
function decodePublicKey(pubkey: string): PublicKey {
  const lines = Buffer.from(pubkey.trim(), "base64").toString("utf8").split(/\r?\n/);
  const raw = Buffer.from(lines[1] ?? "", "base64");
  if (!lines[0]?.startsWith("untrusted comment:") || raw.length !== 42 || raw.toString("latin1", 0, 2) !== "Ed") {
    throw new Error("Not a minisign public key (expected the contents of the .pub file tauri signer wrote).");
  }
  return {
    keyId: raw.subarray(2, 10),
    key: createPublicKey({ key: Buffer.concat([ED25519_SPKI, raw.subarray(10)]), format: "der", type: "spki" }),
  };
}

/**
 * Whether `signature` (a `.sig` file's contents) signs `data` under `pubkey`.
 *
 * Same two checks as `minisign-verify` in the app: the file, prehashed with
 * BLAKE2b-512 when the algorithm says `ED`, and the trusted comment.
 */
export function verifySignature(pubkey: string, signature: string, data: Uint8Array): boolean {
  const { keyId, key } = decodePublicKey(pubkey);
  const lines = Buffer.from(signature.trim(), "base64").toString("utf8").split(/\r?\n/);
  const sig = Buffer.from(lines[1] ?? "", "base64");
  const trusted = lines[2] ?? "";
  const global = Buffer.from(lines[3] ?? "", "base64");
  if (sig.length !== 74 || !trusted.startsWith("trusted comment: ") || global.length !== 64) {
    throw new Error("Not a minisign signature (expected the contents of a .sig file).");
  }

  const algorithm = sig.toString("latin1", 0, 2);
  if (!sig.subarray(2, 10).equals(keyId)) return false;
  const message =
    algorithm === "ED" ? new Bun.CryptoHasher("blake2b512").update(data).digest() : Buffer.from(data);
  const body = sig.subarray(10);
  const comment = Buffer.from(trusted.slice("trusted comment: ".length), "utf8");
  return verify(null, message, key, body) && verify(null, Buffer.concat([body, comment]), key, global);
}

const REPOSITORY = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/;

/**
 * The `--config` overlay a release build merges over `tauri.conf.json`.
 *
 * Kept out of the committed config so a local or bench `bun run build` needs no
 * key and produces a build that says updates are off, rather than failing.
 */
export function updaterConfig({ pubkey, repository }: { pubkey: string; repository: string }) {
  try {
    decodePublicKey(pubkey);
  } catch (e) {
    throw new Error(`TAKYON_UPDATER_PUBKEY: ${(e as Error).message}`);
  }
  if (!REPOSITORY.test(repository)) throw new Error(`Not an owner/name repository: "${repository}"`);
  return {
    bundle: { createUpdaterArtifacts: true },
    plugins: {
      updater: {
        pubkey: pubkey.trim(),
        endpoints: [`https://github.com/${repository}/releases/latest/download/latest.json`],
        // Progress bar, no prompts: the user already chose Install in Settings.
        windows: { installMode: "passive" },
      },
    },
  };
}

const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

/**
 * `latest.json`. Both Windows keys: the plugin tries `windows-x86_64-nsis`
 * (the bundle type it was installed from) first and the bare target second.
 */
export function manifest(m: { version: string; notes: string; pubDate: Date; url: string; signature: string }) {
  if (!SEMVER.test(m.version)) throw new Error(`Not a bare SemVer version: "${m.version}"`);
  const entry = { url: m.url, signature: m.signature };
  return {
    version: m.version,
    notes: m.notes,
    pub_date: m.pubDate.toISOString(),
    platforms: { "windows-x86_64": entry, "windows-x86_64-nsis": entry },
  };
}

/** The body of `## v{version}` in CHANGELOG.md, up to the next `## `. */
export function releaseNotes(changelog: string, version: string): string {
  const lines = changelog.split(/\r?\n/);
  const heading = new RegExp(`^## v${version.replaceAll(".", "\\.")}(?:\\s|$)`);
  const start = lines.findIndex((l) => heading.test(l));
  if (start < 0) throw new Error(`CHANGELOG.md has no "## v${version}" section to publish as release notes.`);
  const rest = lines.slice(start + 1);
  const end = rest.findIndex((l) => l.startsWith("## "));
  return (end < 0 ? rest : rest.slice(0, end)).join("\n").trim();
}
