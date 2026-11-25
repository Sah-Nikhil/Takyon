import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { manifest, releaseBody, releaseNotes, updaterConfig, verifySignature } from "./updater";

/**
 * Signed by `tauri signer sign` with a throwaway key generated for this fixture
 * and deleted straight after. Only the public halves are kept.
 */
const FIXTURES = join(import.meta.dir, "../fixtures/updater");
const fixture = (name: string) => readFileSync(join(FIXTURES, name));
const SIGNER = fixture("signer.pub").toString().trim();
const STRANGER = fixture("stranger.pub").toString().trim();
const PAYLOAD = fixture("payload.txt");
const SIGNATURE = fixture("payload.txt.sig").toString().trim();

describe("verifySignature", () => {
  test("accepts what tauri signer produced, under the key that signed it", () => {
    expect(verifySignature(SIGNER, SIGNATURE, PAYLOAD)).toBe(true);
  });

  /** The case the release job exists to catch: secret and pubkey from two pairs. */
  test("refuses a signature from a different key pair", () => {
    expect(verifySignature(STRANGER, SIGNATURE, PAYLOAD)).toBe(false);
  });

  test("refuses a file changed by one byte", () => {
    const tampered = Buffer.from(PAYLOAD);
    tampered.writeUInt8(tampered.readUInt8(0) ^ 1, 0);
    expect(verifySignature(SIGNER, SIGNATURE, tampered)).toBe(false);
  });

  /** minisign signs the trusted comment too; the app's verifier checks both. */
  test("refuses an edited trusted comment", () => {
    const text = Buffer.from(SIGNATURE, "base64").toString("utf8");
    const edited = text.replace("file:payload.txt", "file:payload.exe");
    expect(edited).not.toBe(text);
    expect(verifySignature(SIGNER, Buffer.from(edited).toString("base64"), PAYLOAD)).toBe(false);
  });

  test("names a malformed key rather than reporting a mismatch", () => {
    expect(() => verifySignature("not a key", SIGNATURE, PAYLOAD)).toThrow("public key");
  });
});

describe("updaterConfig", () => {
  test("turns on signed artefacts and points at the repository's latest release", () => {
    expect(updaterConfig({ pubkey: SIGNER, repository: "acme/takyon" })).toEqual({
      bundle: { createUpdaterArtifacts: true },
      plugins: {
        updater: {
          pubkey: SIGNER,
          endpoints: ["https://github.com/acme/takyon/releases/latest/download/latest.json"],
          windows: { installMode: "passive" },
        },
      },
    });
  });

  /** A pasted private key or a typo would otherwise ship a build nothing can update. */
  test("refuses a pubkey that is not a minisign public key", () => {
    expect(() => updaterConfig({ pubkey: "", repository: "acme/takyon" })).toThrow("TAKYON_UPDATER_PUBKEY");
    expect(() => updaterConfig({ pubkey: SIGNATURE, repository: "acme/takyon" })).toThrow(
      "TAKYON_UPDATER_PUBKEY",
    );
  });

  test("refuses a repository that is not owner/name", () => {
    expect(() => updaterConfig({ pubkey: SIGNER, repository: "takyon" })).toThrow("repository");
    expect(() => updaterConfig({ pubkey: SIGNER, repository: "a/b/c" })).toThrow("repository");
  });
});

describe("manifest", () => {
  test("is the latest.json shape tauri-plugin-updater reads, for both Windows keys", () => {
    const url = "https://github.com/acme/takyon/releases/download/v1.2.3/Takyon_1.2.3_x64-setup.exe";
    expect(
      manifest({
        version: "1.2.3",
        notes: "Fixed a thing.",
        pubDate: new Date("2026-10-28T09:30:00.000Z"),
        url,
        signature: SIGNATURE,
      }),
    ).toEqual({
      version: "1.2.3",
      notes: "Fixed a thing.",
      pub_date: "2026-10-28T09:30:00.000Z",
      platforms: {
        "windows-x86_64": { url, signature: SIGNATURE },
        "windows-x86_64-nsis": { url, signature: SIGNATURE },
      },
    });
  });

  /** The plugin parses `version` as SemVer; a leading `v` fails every check. */
  test("refuses a version that is not bare SemVer", () => {
    const base = { notes: "", pubDate: new Date(0), url: "https://x/y", signature: SIGNATURE };
    expect(() => manifest({ ...base, version: "v1.2.3" })).toThrow("version");
  });
});

describe("releaseNotes", () => {
  const CHANGELOG = [
    "# Changelog",
    "",
    "Intro.",
    "",
    "## v1.2.3 — Updates (pending release)",
    "",
    "- **In-app updates.** Settings checks when asked.",
    "",
    "## v1.2.2 — Older",
    "",
    "- Something else.",
    "",
  ].join("\n");

  test("is the body of exactly that version's section", () => {
    expect(releaseNotes(CHANGELOG, "1.2.3")).toBe("- **In-app updates.** Settings checks when asked.");
  });

  test("reads the last section to the end of the file", () => {
    expect(releaseNotes(CHANGELOG, "1.2.2")).toBe("- Something else.");
  });

  /** v1.2.30 must not answer for v1.2.3. */
  test("matches the version whole, not as a prefix", () => {
    expect(() => releaseNotes(CHANGELOG.replace("v1.2.3 ", "v1.2.30 "), "1.2.3")).toThrow("CHANGELOG");
  });

  test("refuses a version with no section rather than publishing nothing", () => {
    expect(() => releaseNotes(CHANGELOG, "9.9.9")).toThrow("CHANGELOG");
  });
});

describe("releaseBody", () => {
  const notes = "- **Agent fallback.** The next Agent answers.\n- **Faster `!s`.** Pages stop at 3.5 s.";
  const commits = [
    { subject: "NEW d0.11.74 agent fallback, !s fast model", hash: "a1b2c3d" },
    { subject: "FIX d0.11.75 what's new row retires", hash: "b2c3d4e" },
    { subject: "FIX d0.11.76 opencode v2 flags", hash: "c3d4e5f" },
    { subject: "Merge pull request #33 from Sah-Nikhil/t3/x", hash: "d4e5f60" },
    { subject: "Update demo_vid.mp4", hash: "e5f6071" },
    { subject: "UPDATE d0.11.77 version 0.12.10", hash: "f607182" },
  ];

  test("leads with a counted header, then the labelled bullets, then the folded list", () => {
    const body = releaseBody("0.12.10", notes, commits);
    expect(body.startsWith("**v0.12.10** · 1 new, 2 fixes\n\n- **Agent fallback.**")).toBe(true);
    expect(body).toContain("<details>\n<summary>Full changelog</summary>");
    expect(body).toContain("- NEW agent fallback, !s fast model (a1b2c3d)");
    expect(body).toContain("- FIX opencode v2 flags (c3d4e5f)");
  });

  test("leaves merges and other commits out of the count and the list", () => {
    const body = releaseBody("0.12.10", notes, commits);
    expect(body).not.toContain("Merge pull request");
    expect(body).not.toContain("demo_vid");
    expect(body).not.toContain("version 0.12.10");
  });

  test("a range with no conventional commits has no tally and no fold", () => {
    const body = releaseBody("0.12.10", notes, [{ subject: "Update demo_vid.mp4", hash: "e5f6071" }]);
    expect(body).toBe(`**v0.12.10**\n\n${notes}\n`);
  });
});

