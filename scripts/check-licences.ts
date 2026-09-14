/**
 * Enforce CLAUDE.md's "avoid GPL dependencies" while distribution is undecided.
 *
 * Every crate in `Cargo.lock` (all targets) and every installed JS package must be
 * usable under at least one licence outside the copyleft families below. An SPDX
 * `OR` passes if any branch does, so `MIT OR LGPL-2.1` is fine. A missing licence
 * fails too: unknown is not permissive. Our own workspace packages are skipped.
 */

import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";

const ROOT = resolve(import.meta.dir, "..");
const MANIFEST = join(ROOT, "apps/desktop/src-tauri/Cargo.toml");

/** Strong or network copyleft, plus LGPL: Rust links statically, so LGPL binds too. */
const COPYLEFT = /^(?:A?GPL|LGPL|SSPL|EUPL|OSL|CPAL|RPL|Sleepycat|CC-BY-(?:NC|SA))/i;

/** Whether an SPDX expression can be satisfied without a copyleft licence. */
export function permits(expression: string): boolean {
  const tokens = expression.replaceAll("/", " OR ").match(/\(|\)|[^\s()]+/g) ?? [];
  let at = 0;
  const peek = () => tokens[at];
  const or = (): boolean => {
    let ok = and();
    while (peek()?.toUpperCase() === "OR") {
      at++;
      ok = and() || ok;
    }
    return ok;
  };
  const and = (): boolean => {
    let ok = atom();
    while (peek()?.toUpperCase() === "AND") {
      at++;
      ok = atom() && ok;
    }
    return ok;
  };
  const atom = (): boolean => {
    const token = tokens[at++];
    if (token === "(") {
      const ok = or();
      at++; // ")"
      return ok;
    }
    // `WITH` adds an exception to a licence; it never removes copyleft on its own.
    if (peek()?.toUpperCase() === "WITH") at += 2;
    return token !== undefined && !COPYLEFT.test(token);
  };
  return tokens.length > 0 && or();
}

type Finding = { name: string; licence: string };

function cargo(): { count: number; found: Finding[] } {
  const proc = Bun.spawnSync(["cargo", "metadata", "--format-version", "1", "--locked", "--manifest-path", MANIFEST]);
  if (proc.exitCode !== 0) throw new Error(`cargo metadata failed:\n${proc.stderr.toString()}`);
  const meta = JSON.parse(proc.stdout.toString()) as {
    packages: { name: string; version: string; license: string | null; source: string | null }[];
  };
  const external = meta.packages.filter((p) => p.source !== null); // null source: a path crate, ours
  const found = external
    .filter((p) => !p.license || !permits(p.license))
    .map((p) => ({ name: `crate ${p.name}@${p.version}`, licence: p.license ?? "none declared" }));
  return { count: external.length, found };
}

/** Installed packages, from bun's store and any hoisted copy. */
function js(): { count: number; found: Finding[] } {
  const seen = new Map<string, string | null>();
  const readPackage = (dir: string) => {
    const file = join(dir, "package.json");
    if (!existsSync(file)) return;
    const p = JSON.parse(readFileSync(file, "utf8")) as {
      name?: string;
      version?: string;
      private?: boolean;
      license?: string | { type?: string };
      licenses?: { type?: string }[];
    };
    if (!p.name || p.private || p.name.startsWith("@takyon/")) return;
    const licence =
      typeof p.license === "string" ? p.license : (p.license?.type ?? p.licenses?.map((l) => l.type).join(" OR ") ?? null);
    seen.set(`${p.name}@${p.version}`, licence || null);
  };
  const scanModules = (modules: string) => {
    if (!existsSync(modules)) return;
    for (const name of readdirSync(modules)) {
      if (name.startsWith(".")) continue;
      const dir = join(modules, name);
      if (name.startsWith("@")) for (const scoped of readdirSync(dir)) readPackage(join(dir, scoped));
      else readPackage(dir);
    }
  };
  for (const workspace of ["", "apps/desktop", "packages/shared", "brand"]) {
    const modules = join(ROOT, workspace, "node_modules");
    scanModules(modules);
    const store = join(modules, ".bun");
    if (existsSync(store)) for (const entry of readdirSync(store)) scanModules(join(store, entry, "node_modules"));
  }
  const found = [...seen]
    .filter(([, licence]) => !licence || !permits(licence))
    .map(([name, licence]) => ({ name: `npm ${name}`, licence: licence ?? "none declared" }));
  return { count: seen.size, found };
}

if (import.meta.main) {
  const crates = cargo();
  const packages = js();
  const found = [...crates.found, ...packages.found];
  if (found.length === 0) {
    console.log(`check-licences: ${crates.count} crates, ${packages.count} JS packages, none needs a copyleft licence`);
    process.exit(0);
  }
  for (const f of found) console.error(`  ${f.name}  ${f.licence}`);
  console.error(`\n${found.length} dependencies need a copyleft licence or declare none (CLAUDE.md: avoid GPL).`);
  process.exit(1);
}
