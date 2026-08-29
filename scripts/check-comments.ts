/**
 * Enforce CLAUDE.md's comment ceiling: three lines of prose on an item or line
 * comment, ten on a module doc-string. Delimiters and blank lines are free.
 *
 * A convention nobody can check is a convention that decays, and this one is
 * easy to drift past — long comments are added one reasonable paragraph at a
 * time. Reasoning that needs more room goes to `docs/`, not into the source.
 */

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";

const ROOT = resolve(import.meta.dir, "..");
const ITEM_MAX = 4; // a summary line plus three of detail
const MODULE_MAX = 10;

/** Source we own. Generated output and dependencies are not ours to reformat. */
const ROOTS = [
  "apps/desktop/src",
  "apps/desktop/src-tauri/src",
  "apps/desktop/src-tauri/tests",
  "apps/desktop/tests",
  "packages/shared/src",
  "scripts",
];
const SKIP = new Set(["node_modules", "target", "dist", "gen", "__screenshots__"]);
const EXTS = [".rs", ".ts", ".tsx"];

function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    if (SKIP.has(name)) continue;
    const path = join(dir, name);
    if (statSync(path).isDirectory()) walk(path, out);
    else if (EXTS.some((e) => name.endsWith(e))) out.push(path);
  }
  return out;
}

type Violation = { file: string; line: number; kind: string; prose: number; max: number };

/** Lines of actual text, ignoring the syntax that wraps them. */
function prose(lines: string[]): number {
  return lines.filter((l) => {
    const t = l.trim().replace(/^\/\/[!/]?|^\/\*+|\*+\/$|^\*/g, "").trim();
    return t.length > 0;
  }).length;
}

function check(file: string): Violation[] {
  const lines = readFileSync(file, "utf8").split("\n");
  const found: Violation[] = [];
  const rel = relative(ROOT, file);
  let i = 0;

  while (i < lines.length) {
    const t = lines[i]!.trim();
    const isModuleRust = t.startsWith("//!");
    const isLine = t.startsWith("//");
    const isBlock = t.startsWith("/*");

    if (isModuleRust || isLine) {
      let j = i;
      while (j < lines.length) {
        const s = lines[j]!.trim();
        if (!s.startsWith("//")) break;
        if (isModuleRust !== s.startsWith("//!")) break;
        j++;
      }
      const n = prose(lines.slice(i, j));
      const max = isModuleRust ? MODULE_MAX : ITEM_MAX;
      if (n > max) found.push({ file: rel, line: i + 1, kind: isModuleRust ? "module" : "item", prose: n, max });
      i = j;
      continue;
    }

    if (isBlock) {
      let j = i;
      while (j < lines.length && !lines[j]!.includes("*/")) j++;
      // A block comment before any code is the file's doc-string.
      const isModule = lines.slice(0, i).every((l) => !l.trim());
      const n = prose(lines.slice(i, j + 1));
      const max = isModule ? MODULE_MAX : ITEM_MAX;
      if (n > max) found.push({ file: rel, line: i + 1, kind: isModule ? "module" : "block", prose: n, max });
      i = j + 1;
      continue;
    }
    i++;
  }
  return found;
}

const files = ROOTS.flatMap((r) => {
  const dir = join(ROOT, r);
  try {
    return statSync(dir).isDirectory() ? walk(dir) : [dir];
  } catch {
    return [];
  }
});

const violations = files.flatMap(check);

if (violations.length === 0) {
  console.log(`check-comments: ${files.length} files, every comment within the ceiling`);
  process.exit(0);
}

const byFile = new Map<string, Violation[]>();
for (const v of violations) byFile.set(v.file, [...(byFile.get(v.file) ?? []), v]);

for (const [file, rows] of [...byFile].sort()) {
  console.error(`\n${file}  (${rows.length})`);
  for (const r of rows) {
    console.error(`  L${String(r.line).padEnd(5)} ${r.kind.padEnd(6)} ${r.prose} lines of prose (max ${r.max})`);
  }
}
console.error(
  `\n${violations.length} comments over the ceiling in ${byFile.size} files.` +
    `\nCLAUDE.md: reasoning that needs more room belongs in docs/, with a pointer left at the code.`,
);
process.exit(1);
