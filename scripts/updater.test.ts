import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { copyFileSync, existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

/**
 * The CLI exactly as `release.yml` runs it, over the throwaway-key fixture.
 * The release job's only test before a real tag: it publishes what this writes.
 */
const SCRIPT = join(import.meta.dir, "updater.ts");
const FIXTURES = join(import.meta.dir, "fixtures/updater");
const SIGNER = readFileSync(join(FIXTURES, "signer.pub"), "utf8").trim();
const STRANGER = readFileSync(join(FIXTURES, "stranger.pub"), "utf8").trim();
const { version: VERSION } = JSON.parse(
  readFileSync(join(import.meta.dir, "../apps/desktop/src-tauri/tauri.conf.json"), "utf8"),
) as { version: string };
const INSTALLER = `Takyon_${VERSION}_x64-setup.exe`;

let dir: string;
beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), "takyon-updater-"));
  copyFileSync(join(FIXTURES, "payload.txt"), join(dir, INSTALLER));
  copyFileSync(join(FIXTURES, "payload.txt.sig"), join(dir, `${INSTALLER}.sig`));
});
afterEach(() => rmSync(dir, { recursive: true, force: true }));

function run(args: string[], pubkey = SIGNER) {
  const proc = Bun.spawnSync(["bun", SCRIPT, ...args], {
    env: { ...process.env, TAKYON_UPDATER_PUBKEY: pubkey, GITHUB_REPOSITORY: "acme/takyon" },
    stdout: "pipe",
    stderr: "pipe",
  });
  return { code: proc.exitCode, out: proc.stdout.toString(), err: proc.stderr.toString() };
}

describe("updater manifest", () => {
  test("writes latest.json for this version and the CHANGELOG notes beside it", () => {
    const notesOut = join(dir, "..", `notes-${Date.now()}.md`);
    const { code, err } = run(["manifest", dir, VERSION, notesOut]);
    expect(err).toBe("");
    expect(code).toBe(0);

    const latest = JSON.parse(readFileSync(join(dir, "latest.json"), "utf8"));
    const url = `https://github.com/acme/takyon/releases/download/v${VERSION}/${INSTALLER}`;
    expect(latest.version).toBe(VERSION);
    expect(latest.platforms["windows-x86_64-nsis"].url).toBe(url);
    expect(latest.platforms["windows-x86_64"].signature).toBe(
      readFileSync(join(dir, `${INSTALLER}.sig`), "utf8").trim(),
    );
    expect(latest.notes.length).toBeGreaterThan(0);
    expect(readFileSync(notesOut, "utf8").trim()).toBe(latest.notes);
    rmSync(notesOut);
  });

  /** Mismatched secret and variable: the release stops before anything is published. */
  test("refuses an installer signed by another key, and writes nothing", () => {
    const { code, err } = run(["manifest", dir, VERSION, join(dir, "notes.md")], STRANGER);
    expect(code).toBe(1);
    expect(err).toContain("different key pairs");
    expect(existsSync(join(dir, "latest.json"))).toBe(false);
  });

  test("refuses a directory whose installer is another version", () => {
    const { code, err } = run(["manifest", dir, "0.0.1", join(dir, "notes.md")]);
    expect(code).toBe(1);
    expect(err).toContain("not the v0.0.1 installer");
  });
});

describe("updater config", () => {
  test("writes the overlay with the repository's feed", () => {
    const out = join(dir, "updater.conf.json");
    expect(run(["config", out]).code).toBe(0);
    const config = JSON.parse(readFileSync(out, "utf8"));
    expect(config.plugins.updater.endpoints).toEqual([
      "https://github.com/acme/takyon/releases/latest/download/latest.json",
    ]);
    expect(config.bundle.createUpdaterArtifacts).toBe(true);
  });

  test("names the missing variable without printing any key", () => {
    const { code, err } = run(["config", join(dir, "x.json")], "");
    expect(code).toBe(1);
    expect(err).toContain("TAKYON_UPDATER_PUBKEY is not set");
  });
});
