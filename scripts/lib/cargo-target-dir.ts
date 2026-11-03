/**
 * Where cargo writes build output for the desktop crate.
 *
 * Not always `src-tauri/target`: `CARGO_TARGET_DIR`, `--target-dir` or a cargo
 * config can move it (`D:\cargo-target` on the main dev machine). `cargo metadata`
 * resolves all three the way the build itself does. PowerShell twin:
 * `cargo-target-dir.ps1`.
 */

import { join } from "node:path";

export function cargoTargetDir(root: string): string {
  const manifest = join(root, "apps", "desktop", "src-tauri", "Cargo.toml");
  const cmd = ["cargo", "metadata", "--format-version", "1", "--no-deps", "--manifest-path", manifest];
  const proc = Bun.spawnSync(cmd, { cwd: root, stdout: "pipe", stderr: "pipe" });
  if (proc.exitCode !== 0) {
    throw new Error(`${cmd.join(" ")} failed:\n${proc.stderr.toString().trim()}`);
  }
  return (JSON.parse(proc.stdout.toString()) as { target_directory: string }).target_directory;
}
