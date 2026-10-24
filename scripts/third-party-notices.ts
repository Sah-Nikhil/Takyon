/**
 * Write `THIRD_PARTY_NOTICES.txt`: licence and notice text of every third-party
 * component the installer distributes. MIT wants its notice in all copies,
 * Apache-2.0 §4 the licence plus any NOTICE file, so `tauri build` runs this first.
 *
 * Shipped means compiled in: crates on normal edges from `takyon` for the target
 * triple, and JS reached through `@takyon/desktop`'s production dependencies.
 * Dev and build edges are skipped. Output is generated and gitignored; bundled
 * by `tauri.bundle.conf.json`, never by the base config (a missing resource
 * fails every cargo build, clippy and tests included).
 */

import { existsSync, readdirSync, readFileSync, realpathSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const ROOT = resolve(import.meta.dir, "..");
const MANIFEST = join(ROOT, "apps/desktop/src-tauri/Cargo.toml");
const APP = join(ROOT, "apps/desktop");
const OUT = join(ROOT, "apps/desktop/src-tauri/THIRD_PARTY_NOTICES.txt");
/** Tauri sets this for `beforeBuildCommand`; bare runs mean the Windows installer. */
const TARGET = process.env.TAURI_ENV_TARGET_TRIPLE || "x86_64-pc-windows-msvc";

/** Crates whose binaries ship. The uiaccess helper is not bundled yet; add it here when it is. */
const SHIPPED_ROOTS = ["takyon"];
/** Production dependencies that only run inside Vite. Their output ships, they do not. */
const BUILD_ONLY = new Set(["@tailwindcss/vite"]);

const LICENCE_FILE = /^(?:licen[cs]e|copying|copyright|notice)/i;

export type LicenceFile = { name: string; text: string };
export type Component = {
  kind: "crate" | "npm";
  name: string;
  version: string;
  licence: string | null;
  repository: string | null;
  files: LicenceFile[];
};

type CargoPackage = {
  id: string;
  name: string;
  version: string;
  license: string | null;
  license_file: string | null;
  repository: string | null;
  source: string | null;
  manifest_path: string;
};
export type CargoMetadata = {
  packages: CargoPackage[];
  resolve: {
    root: string | null;
    nodes: { id: string; deps: { pkg: string; dep_kinds: { kind: string | null; target: string | null }[] }[] }[];
  };
};

/** Case-insensitive first, ordinal second: deterministic on every OS and locale. */
function compare(a: string, b: string): number {
  const [la, lb] = [a.toLowerCase(), b.toLowerCase()];
  if (la !== lb) return la < lb ? -1 : 1;
  return a < b ? -1 : a > b ? 1 : 0;
}

/** LF endings, no trailing blank lines: a CRLF checkout must render the same bytes as CI. */
function normalise(text: string): string {
  return text.replace(/\r\n?/g, "\n").replace(/\s+$/, "");
}

/** Top-level licence, copying, copyright and notice files in `dir`, plus a declared one. */
export function licenceFiles(dir: string, declared?: string | null): LicenceFile[] {
  const names = existsSync(dir)
    ? readdirSync(dir).filter((n) => LICENCE_FILE.test(n) && statSync(join(dir, n)).isFile())
    : [];
  const declaredName = declared?.replaceAll("\\", "/");
  if (declaredName && !names.includes(declaredName) && existsSync(join(dir, declaredName))) names.push(declaredName);
  return names.sort(compare).map((name) => ({ name, text: normalise(readFileSync(join(dir, name), "utf8")) }));
}

/** External crates reached from `roots` over normal edges. Path crates are ours, walked but not listed. */
export function shippedCrates(meta: CargoMetadata, roots: string[]): CargoPackage[] {
  const byId = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const stack = roots.map((name) => {
    const root = meta.packages.find((p) => p.name === name && p.source === null);
    if (!root) throw new Error(`shipped root crate ${name} is not a workspace package`);
    return root.id;
  });
  const seen = new Set<string>();
  while (stack.length > 0) {
    const id = stack.pop()!;
    if (seen.has(id)) continue;
    seen.add(id);
    for (const dep of nodes.get(id)?.deps ?? []) {
      if (dep.dep_kinds.some((k) => k.kind === null)) stack.push(dep.pkg);
    }
  }
  return [...seen]
    .map((id) => byId.get(id)!)
    .filter((p) => p.source !== null)
    .sort((a, b) => compare(a.name, b.name) || compare(a.version, b.version));
}

type PackageJson = {
  name?: string;
  version?: string;
  private?: boolean;
  license?: string | { type?: string };
  licenses?: { type?: string }[];
  repository?: string | { url?: string };
  dependencies?: Record<string, string>;
  optionalDependencies?: Record<string, string>;
  peerDependencies?: Record<string, string>;
  peerDependenciesMeta?: Record<string, { optional?: boolean }>;
};

const readJson = (dir: string) => JSON.parse(readFileSync(join(dir, "package.json"), "utf8")) as PackageJson;

/** Node's lookup: `node_modules/<name>` in `from` and each parent. Real paths, so bun's store resolves. */
function resolvePackage(name: string, from: string): string | null {
  for (let dir = from; ; dir = dirname(dir)) {
    const candidate = join(dir, "node_modules", name);
    if (existsSync(join(candidate, "package.json"))) return realpathSync(candidate);
    if (dirname(dir) === dir) return null;
  }
}

function npmLicence(p: PackageJson): string | null {
  const licence =
    typeof p.license === "string" ? p.license : (p.license?.type ?? p.licenses?.map((l) => l.type).join(" OR "));
  return licence || null;
}

function npmRepository(p: PackageJson): string | null {
  const url = typeof p.repository === "string" ? p.repository : p.repository?.url;
  return url ? url.replace(/^git\+/, "") : null;
}

/** Packages reached from `appDir`'s production dependencies. Workspace and private packages walked, not listed. */
export function bundledJs(appDir: string, buildOnly: Set<string>): (Component & { dir: string })[] {
  const found = new Map<string, Component & { dir: string }>();
  const visited = new Set<string>();
  const visit = (dir: string) => {
    if (visited.has(dir)) return;
    visited.add(dir);
    const p = readJson(dir);
    const ours = p.private || p.name?.startsWith("@takyon/");
    if (!ours && p.name) {
      found.set(dir, {
        kind: "npm",
        name: p.name,
        version: p.version ?? "unknown",
        licence: npmLicence(p),
        repository: npmRepository(p),
        files: licenceFiles(dir),
        dir,
      });
    }
    const required = Object.keys(p.dependencies ?? {});
    const optional = [
      ...Object.keys(p.optionalDependencies ?? {}),
      ...Object.keys(p.peerDependencies ?? {}).filter((n) => !p.peerDependenciesMeta?.[n]?.optional),
    ];
    for (const name of [...required, ...optional]) {
      if (buildOnly.has(name)) continue;
      const at = resolvePackage(name, dir);
      if (at) visit(at);
      else if (required.includes(name)) throw new Error(`${p.name ?? dir} depends on ${name}, which is not installed`);
    }
  };
  visit(realpathSync(appDir));
  return [...found.values()].sort((a, b) => compare(a.name, b.name) || compare(a.version, b.version));
}

/** Licence ids in an SPDX expression, exceptions after `WITH` dropped. */
function licenceIds(expression: string): string[] {
  const tokens = expression.replaceAll("/", " OR ").match(/[^\s()]+/g) ?? [];
  const ids = tokens.filter((t, i) => !/^(?:AND|OR|WITH)$/i.test(t) && tokens[i - 1]?.toUpperCase() !== "WITH");
  return [...new Set(ids)];
}

const label = (c: Component) => `${c.kind} ${c.name} ${c.version}`;
const rule = (char: string) => char.repeat(78);

/** The notices file. Identical texts print once; a package shipping none points at SPDX and the appendix. */
export function render(components: Component[], canonical: Record<string, string>): string {
  const sorted = [...components].sort(
    (a, b) => compare(a.kind, b.kind) || compare(a.name, b.name) || compare(a.version, b.version),
  );
  const texts = new Map<string, { id: number; name: string; users: string[] }>();
  const appendix = new Set<string>();
  const entries = sorted.map((c) => {
    const lines = [label(c), `  Licence: ${c.licence ?? "none declared"}`];
    if (c.repository) lines.push(`  Repository: ${c.repository}`);
    if (c.files.length > 0) {
      const refs = c.files.map((f) => {
        let entry = texts.get(f.text);
        if (!entry) texts.set(f.text, (entry = { id: texts.size + 1, name: f.name, users: [] }));
        entry.users.push(label(c));
        return `[${entry.id}] ${f.name}`;
      });
      lines.push(`  Licence texts: ${refs.join(", ")}`);
    } else {
      const ids = c.licence ? licenceIds(c.licence) : [];
      const urls = ids.map((id) => `https://spdx.org/licenses/${id}.html`);
      for (const id of ids) if (canonical[id]) appendix.add(id);
      lines.push(
        urls.length > 0
          ? `  Licence texts: licence text not shipped by package; see ${urls.join(", ")}`
          : "  Licence texts: none shipped and no licence declared",
      );
    }
    return lines.join("\n");
  });

  const out = [
    "THIRD-PARTY NOTICES FOR TAKYON",
    "",
    "Takyon includes the third-party components listed below. Each one remains",
    "under its own licence, reproduced here as its authors ship it. Those licences",
    "are separate from Takyon's own licence, CC-BY-NC-SA-4.0, which covers only",
    "Takyon's code and does not change the terms of any component listed here.",
    "",
    "This file is generated at build time from the components compiled into",
    "this release.",
    "",
    rule("="),
    `Components (${sorted.length})`,
    rule("="),
    "",
    entries.join("\n\n"),
    "",
    rule("="),
    "Licence texts",
    rule("="),
  ];
  for (const [text, { id, name, users }] of texts) {
    out.push("", rule("-"), `[${id}] ${name}`, `Used by: ${users.join(", ")}`, rule("-"), "", text);
  }
  if (appendix.size > 0) {
    out.push("", rule("="), "Canonical licence texts, for components that ship none", rule("="));
    for (const id of [...appendix].sort(compare)) out.push("", rule("-"), id, rule("-"), "", canonical[id]!);
  }
  return `${out.join("\n")}\n`;
}

function cargoComponents(): Component[] {
  const proc = Bun.spawnSync([
    "cargo",
    "metadata",
    "--format-version",
    "1",
    "--locked",
    "--filter-platform",
    TARGET,
    "--manifest-path",
    MANIFEST,
  ]);
  if (proc.exitCode !== 0) throw new Error(`cargo metadata failed:\n${proc.stderr.toString()}`);
  const meta = JSON.parse(proc.stdout.toString()) as CargoMetadata;
  return shippedCrates(meta, SHIPPED_ROOTS).map((p) => ({
    kind: "crate",
    name: p.name,
    version: p.version,
    licence: p.license,
    repository: p.repository,
    files: licenceFiles(dirname(p.manifest_path), p.license_file),
  }));
}

function canonicalTexts(): Record<string, string> {
  const dir = join(import.meta.dir, "licences");
  return Object.fromEntries(
    readdirSync(dir)
      .filter((n) => n.endsWith(".txt"))
      .map((n) => [n.slice(0, -4), normalise(readFileSync(join(dir, n), "utf8"))]),
  );
}

if (import.meta.main) {
  const crates = cargoComponents();
  const js = bundledJs(APP, BUILD_ONLY).map(({ dir: _dir, ...c }) => c);
  writeFileSync(OUT, render([...crates, ...js], canonicalTexts()));
  const bare = [...crates, ...js].filter((c) => c.files.length === 0).length;
  console.log(
    `third-party-notices: ${crates.length} crates (${TARGET}), ${js.length} JS packages, ${bare} without a shipped licence text`,
  );
}
