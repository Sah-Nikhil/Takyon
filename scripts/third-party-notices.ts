/**
 * Write `THIRD_PARTY_NOTICES.txt`: licence and notice text of every third-party
 * component the installer distributes. MIT wants its notice in all copies,
 * Apache-2.0 §4 the licence plus any NOTICE file, so `tauri build` runs this first.
 * `THIRD_PARTY_NOTICES.json` carries the same components for About's list.
 *
 * Shipped means compiled in: crates on normal edges from `takyon` for the target
 * triple, and JS reached through `@takyon/desktop`'s production dependencies.
 * Dev and build edges are skipped. Output is generated and gitignored; bundled
 * by `tauri.bundle.conf.json`, never by the base config (a missing resource
 * fails every cargo build, clippy and tests included).
 */

import { existsSync, readdirSync, readFileSync, realpathSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import type { LicenceComponent, LicenceText, ThirdPartyLicences } from "../packages/shared/src/ipc";

const ROOT = resolve(import.meta.dir, "..");
const MANIFEST = join(ROOT, "apps/desktop/src-tauri/Cargo.toml");
const APP = join(ROOT, "apps/desktop");
const OUT = join(ROOT, "apps/desktop/src-tauri/THIRD_PARTY_NOTICES.txt");
/** Read by `notices.rs` when About's licence list opens. Never parsed from the text file. */
const OUT_JSON = join(ROOT, "apps/desktop/src-tauri/THIRD_PARTY_NOTICES.json");
/** Tauri sets this for `beforeBuildCommand`; bare runs mean the Windows installer. */
const TARGET = process.env.TAURI_ENV_TARGET_TRIPLE || "x86_64-pc-windows-msvc";

/** Crates whose binaries ship. The uiaccess helper is not bundled yet; add it here when it is. */
const SHIPPED_ROOTS = ["takyon"];
/** Production dependencies that only run inside Vite. Their output ships, they do not. */
const BUILD_ONLY = new Set(["@tailwindcss/vite"]);

const LICENCE_FILE = /^(?:licen[cs]e|copying|copyright|notice)/i;

/** SPDX full names for licences in current shipped dependency metadata. */
const LICENCE_NAMES: Record<string, string> = {
  "0BSD": "BSD Zero Clause License",
  "Apache-2.0": "Apache License 2.0",
  "BSD-2-Clause": 'BSD 2-Clause "Simplified" License',
  "BSD-3-Clause": 'BSD 3-Clause "New" or "Revised" License',
  "CC0-1.0": "Creative Commons Zero v1.0 Universal",
  MIT: "MIT License",
  "MIT-0": "MIT No Attribution",
  "MPL-2.0": "Mozilla Public License 2.0",
  "Unicode-3.0": "Unicode License v3",
  Unlicense: "The Unlicense",
  Zlib: "zlib License",
};

/** Crate archives omit workspace-root licence files; pinned copies live beside this script. */
const UPSTREAM_LICENCES: Record<string, string[]> = {
  "alloc-stdlib@0.2.4": ["alloc-stdlib-0.2.4-LICENSE.txt"],
  "defmt-parser@1.0.0": ["defmt-parser-1.0.0-LICENSE-APACHE.txt", "defmt-parser-1.0.0-LICENSE-MIT.txt"],
  "unic-char-property@0.9.0": ["unic-0.9.0-LICENSE-APACHE.txt", "unic-0.9.0-LICENSE-MIT.txt"],
  "unic-char-range@0.9.0": ["unic-0.9.0-LICENSE-APACHE.txt", "unic-0.9.0-LICENSE-MIT.txt"],
  "unic-common@0.9.0": ["unic-0.9.0-LICENSE-APACHE.txt", "unic-0.9.0-LICENSE-MIT.txt"],
  "unic-ucd-ident@0.9.0": ["unic-0.9.0-LICENSE-APACHE.txt", "unic-0.9.0-LICENSE-MIT.txt"],
  "unic-ucd-version@0.9.0": ["unic-0.9.0-LICENSE-APACHE.txt", "unic-0.9.0-LICENSE-MIT.txt"],
  "webview2-com@0.38.2": ["webview2-com-LICENSE.txt"],
  "webview2-com-macros@0.8.1": ["webview2-com-macros-LICENSE.txt"],
  "webview2-com-sys@0.38.2": ["webview2-com-LICENSE.txt"],
};

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

function upstreamLicenceFiles(name: string, version: string): LicenceFile[] {
  return (UPSTREAM_LICENCES[`${name}@${version}`] ?? []).map((file) => ({
    name: `${file.slice(file.lastIndexOf("-LICENSE") + 1, -4)} (upstream source)`,
    text: normalise(readFileSync(join(import.meta.dir, "licences/upstream", file), "utf8")),
  }));
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

/** Expand SPDX ids; preserve OR choices and AND obligations from package metadata. */
function licenceNames(expression: string): string {
  return expression.replaceAll("/", " OR ").replace(/[^\s()]+/g, (token) => {
    if (token === "OR" || token === "AND" || token === "WITH") return token;
    const name = LICENCE_NAMES[token];
    if (!name) throw new Error(`unknown SPDX licence id ${token}`);
    return `${name} (${token})`;
  });
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
    const lines = [label(c), `  Licence: ${c.licence ? licenceNames(c.licence) : "none declared"}`];
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
    "under its declared licence. OR means a choice; AND means both apply. The",
    "licence names and identifiers come from SPDX. Texts come from component",
    "archives or pinned upstream source; canonical texts cover remaining gaps.",
    "Those licences are separate from Takyon's own CC-BY-NC-SA-4.0 licence.",
    "It covers only Takyon's code and does not change their terms.",
    "",
    "This file is generated at build time from the production dependency graphs",
    "for this release.",
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

/** About's licence list: same components and texts as `render`, as data. */
export function catalogue(components: Component[], canonical: Record<string, string>): ThirdPartyLicences {
  const sorted = [...components].sort(
    (a, b) => compare(a.kind, b.kind) || compare(a.name, b.name) || compare(a.version, b.version),
  );
  const texts: LicenceText[] = [];
  const index = new Map<string, number>();
  const intern = (name: string, text: string) => {
    const identity = `${name}\u0000${text}`;
    let at = index.get(identity);
    if (at === undefined) index.set(identity, (at = texts.push({ name, text }) - 1));
    return at;
  };
  const list = sorted.map((c): LicenceComponent => {
    const ids = c.files.length > 0 || !c.licence ? [] : licenceIds(c.licence).filter((id) => canonical[id]);
    return {
      kind: c.kind,
      name: c.name,
      version: c.version,
      licence: c.licence,
      licenceName: c.licence ? licenceNames(c.licence) : null,
      repository: c.repository,
      texts:
        c.files.length > 0
          ? c.files.map((f) => intern(f.name, f.text))
          : ids.map((id) => intern(id, canonical[id]!)),
      canonical: c.files.length === 0 && ids.length > 0,
    };
  });
  return { components: list, texts };
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
  return shippedCrates(meta, SHIPPED_ROOTS).map((p) => {
    const files = licenceFiles(dirname(p.manifest_path), p.license_file);
    return {
      kind: "crate",
      name: p.name,
      version: p.version,
      licence: p.license,
      repository: p.repository,
      files: files.length > 0 ? files : upstreamLicenceFiles(p.name, p.version),
    };
  });
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
  const canonical = canonicalTexts();
  writeFileSync(OUT, render([...crates, ...js], canonical));
  writeFileSync(OUT_JSON, `${JSON.stringify(catalogue([...crates, ...js], canonical))}\n`);
  const bare = [...crates, ...js].filter((c) => c.files.length === 0).length;
  console.log(
    `third-party-notices: ${crates.length} crates (${TARGET}), ${js.length} JS packages, ${bare} without a shipped licence text`,
  );
}
