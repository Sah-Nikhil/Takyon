/**
 * Enforce CLAUDE.md's "no file under `apps/desktop/src` may name a colour".
 *
 * Colours live in `theme/themes.ts` (the registry) and `styles.css` (tokens).
 * Anything else names a hue a theme cannot change: white-at-10% borders shipped
 * invisible on a light plate for four phases. Exceptions are listed below, per
 * literal, so a second literal in an excepted file still fails.
 */

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";

const ROOT = resolve(import.meta.dir, "..");
const SRC = join(ROOT, "apps/desktop/src");

/** The registry is where colours belong; tests may assert on its values. */
const ALLOWED_FILES = new Set(["apps/desktop/src/theme/themes.ts"]);

/** Each one a decision, recorded in CLAUDE.md's Stack section. */
const EXCEPTIONS: { file: string; literal: string; why: string }[] = [
  { file: "apps/desktop/src/settings/TitleBar.tsx", literal: "#c42b1c", why: "Windows' own close-button red" },
  { file: "apps/desktop/src/settings/TitleBar.tsx", literal: "text-white", why: "white over that red, dark in every appearance" },
  { file: "apps/desktop/src/theme/ThemeOrb.tsx", literal: "oklch(", why: "lighting endpoints for previews of non-active themes" },
  { file: "apps/desktop/src/theme/ThemeOrb.tsx", literal: "rgb(", why: "the orb's cast shadow, same model" },
];

const PALETTE =
  "red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose|slate|gray|zinc|neutral|stone";
const UTILITY =
  "bg|text|border|ring|ring-offset|fill|stroke|from|via|to|outline|shadow|accent|caret|decoration|divide|placeholder";

const PATTERNS: RegExp[] = [
  /(?<=["'`\s(\[:,])#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{3,4})\b/g,
  /\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch)\(/g,
  /\bcolor\(\s*(?:srgb|display-p3|a98-rgb|prophoto-rgb|rec2020|xyz)/g,
  new RegExp(`\\b(?:${UTILITY})-(?:(?:${PALETTE})-\\d{2,3}|white|black)\\b`, "g"),
];

/** Blank out comments, keeping newlines, so a comment explaining a colour passes. */
function stripComments(text: string): string {
  const blank = (m: string) => m.replace(/[^\n]/g, " ");
  return text.replace(/\/\*[\s\S]*?\*\//g, blank).replace(/(^|[^:])\/\/.*$/gm, (m, lead) => lead + blank(m.slice(lead.length)));
}

function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) walk(path, out);
    else if (/\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name)) out.push(path);
  }
  return out;
}

const found: string[] = [];
const files = walk(SRC);
for (const path of files) {
  const file = relative(ROOT, path).replaceAll("\\", "/");
  if (ALLOWED_FILES.has(file)) continue;
  const lines = stripComments(readFileSync(path, "utf8")).split("\n");
  lines.forEach((line, i) => {
    for (const pattern of PATTERNS) {
      for (const match of line.matchAll(pattern)) {
        const excepted = EXCEPTIONS.some((e) => e.file === file && match[0].startsWith(e.literal));
        if (!excepted) found.push(`  ${file}:${i + 1}  ${match[0]}`);
      }
    }
  });
}

if (found.length === 0) {
  console.log(`check-colours: ${files.length} files, no colour outside the theme`);
  process.exit(0);
}
console.error(`${found.join("\n")}\n\n${found.length} colour literals outside the theme.`);
console.error("CLAUDE.md: add a token in styles.css or a role in theme/themes.ts instead.");
process.exit(1);
