// Generates every form of the Takyon mark from brand/geometry.js + brand/tokens.json.
//
// Run: bun run --cwd brand build
//
// Nothing this script writes should ever be hand-edited — change the geometry or
// the tokens and re-run. Outputs land directly in the places that consume them
// (apps/desktop/src-tauri/icons, apps/desktop/public) plus brand/svg for docs
// and the site. See brand/README.md for the surface map.

import { Resvg } from "@resvg/resvg-js";
import { mkdirSync, writeFileSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { CONE, PARTICLE, VIEWBOX, fit } from "./geometry.js";

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, "..");
const tokens = JSON.parse(readFileSync(join(here, "tokens.json"), "utf8"));

const out = {
  svg: join(here, "svg"),
  icons: join(repo, "apps/desktop/src-tauri/icons"),
  public: join(repo, "apps/desktop/public"),
};
for (const dir of Object.values(out)) mkdirSync(dir, { recursive: true });

// ---------------------------------------------------------------- svg sources

const circle = (fill) =>
  `<circle cx="${PARTICLE.cx}" cy="${PARTICLE.cy}" r="${PARTICLE.r}" fill="${fill}"/>`;
const cone = (fill) => `<path d="${CONE}" fill="${fill}"/>`;

/** The mark alone on transparency, at its authored size. Two-tone or one-tone. */
function markSvg({ mono = false } = {}) {
  const particleFill = mono ? "currentColor" : `var(--accent, ${tokens.accent})`;
  return svg(VIEWBOX, [cone("currentColor"), circle(particleFill)].join("\n  "));
}

/** The mark centred on a coloured canvas, scaled to `fill` of the width. */
function platedSvg(canvas, { fill = 0.68, radius = 0.18, plate, glyph, accent } = {}) {
  const r = canvas * radius;
  const body = [
    `<rect width="${canvas}" height="${canvas}" rx="${r}" ry="${r}" fill="${plate}"/>`,
    `<g transform="${fit(canvas, fill)}">`,
    `  ${cone(glyph)}`,
    `  ${circle(accent ?? glyph)}`,
    `</g>`,
  ].join("\n  ");
  return svg(canvas, body);
}

/** The mark on transparency in one flat colour, sized for a tray slot. */
function traySvg(canvas, colour) {
  const body = [
    `<g transform="${fit(canvas, 0.94)}">`,
    `  ${cone(colour)}`,
    `  ${circle(colour)}`,
    `</g>`,
  ].join("\n  ");
  return svg(canvas, body);
}

function svg(canvas, body) {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${canvas} ${canvas}" width="${canvas}" height="${canvas}">\n  ${body}\n</svg>\n`;
}

const appIcon = platedSvg(1024, {
  plate: tokens.plate,
  glyph: tokens.fg,
  accent: tokens.accent,
});

const sources = {
  "mark.svg": markSvg(),
  "mark-mono.svg": markSvg({ mono: true }),
  "app-icon.svg": appIcon,
  "tray-dark.svg": traySvg(32, tokens.fg), // light glyph, for a dark taskbar
  "tray-light.svg": traySvg(32, tokens.plate), // dark glyph, for a light taskbar
};

for (const [name, contents] of Object.entries(sources)) {
  writeFileSync(join(out.svg, name), contents);
}

// ------------------------------------------------------------------ rendering

function png(source, width) {
  return new Resvg(source, { fitTo: { mode: "width", value: width } })
    .render()
    .asPng();
}

/**
 * PNG-payload ICO. Vista and later read PNG frames directly, so no BMP encoding.
 * A 256px frame is written as width byte 0, which is what the format uses for
 * "256" in a single byte.
 */
function ico(frames) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0); // reserved
  header.writeUInt16LE(1, 2); // type: icon
  header.writeUInt16LE(frames.length, 4);

  let offset = 6 + frames.length * 16;
  const dir = [];
  for (const { size, data } of frames) {
    const entry = Buffer.alloc(16);
    entry.writeUInt8(size >= 256 ? 0 : size, 0);
    entry.writeUInt8(size >= 256 ? 0 : size, 1);
    entry.writeUInt8(0, 2); // palette size
    entry.writeUInt8(0, 3); // reserved
    entry.writeUInt16LE(1, 4); // colour planes
    entry.writeUInt16LE(32, 6); // bits per pixel
    entry.writeUInt32LE(data.length, 8);
    entry.writeUInt32LE(offset, 12);
    offset += data.length;
    dir.push(entry);
  }
  return Buffer.concat([header, ...dir, ...frames.map((f) => f.data)]);
}

/** ICNS container. Every type below accepts a PNG payload on macOS 10.7+. */
function icns(frames) {
  const chunks = [];
  for (const { type, data } of frames) {
    const head = Buffer.alloc(8);
    head.write(type, 0, 4, "ascii");
    head.writeUInt32BE(data.length + 8, 4);
    chunks.push(head, data);
  }
  const body = Buffer.concat(chunks);
  const head = Buffer.alloc(8);
  head.write("icns", 0, 4, "ascii");
  head.writeUInt32BE(body.length + 8, 4);
  return Buffer.concat([head, body]);
}

const cache = new Map();
const appPng = (size) => {
  if (!cache.has(size)) cache.set(size, png(appIcon, size));
  return cache.get(size);
};

// --------------------------------------------------------- apps/desktop icons

const write = (dir, name, data) => writeFileSync(join(dir, name), data);

// Tauri's standard bundle set. Names are fixed — tauri.conf.json's bundle.icon
// array refers to them by exactly these paths.
write(out.icons, "32x32.png", appPng(32));
write(out.icons, "128x128.png", appPng(128));
write(out.icons, "128x128@2x.png", appPng(256));
write(out.icons, "icon.png", appPng(1024));

for (const size of [30, 44, 71, 89, 107, 142, 150, 284, 310]) {
  write(out.icons, `Square${size}x${size}Logo.png`, appPng(size));
}
write(out.icons, "StoreLogo.png", appPng(50));

write(
  out.icons,
  "icon.ico",
  ico([16, 24, 32, 48, 64, 128, 256].map((size) => ({ size, data: appPng(size) }))),
);

// macOS is a future target, but the seam is cheap to keep filled.
write(
  out.icons,
  "icon.icns",
  icns([
    ["ic07", 128], ["ic08", 256], ["ic09", 512], ["ic10", 1024],
    ["ic11", 32], ["ic12", 64], ["ic13", 256], ["ic14", 512],
  ].map(([type, size]) => ({ type, data: appPng(size) }))),
);

// Tray. Windows renders the notification area glyph over a taskbar that follows
// the system theme, so ship both polarities and pick at runtime — a single
// light glyph vanishes the moment someone switches to the light theme.
for (const polarity of ["dark", "light"]) {
  const source = sources[`tray-${polarity}.svg`];
  write(out.icons, `tray-${polarity}.png`, png(source, 32));
  write(
    out.icons,
    `tray-${polarity}.ico`,
    ico([16, 20, 24, 32].map((size) => ({ size, data: png(source, size) }))),
  );
}

// -------------------------------------------------------- apps/desktop public

write(out.public, "favicon.svg", platedSvg(64, {
  plate: tokens.plate,
  glyph: tokens.fg,
  accent: tokens.accent,
}));
write(out.public, "favicon.ico", ico([16, 32, 48].map((size) => ({ size, data: appPng(size) }))));
write(out.public, "mark.svg", sources["mark.svg"]);

console.log("brand assets written:");
console.log(`  ${out.svg}`);
console.log(`  ${out.icons}`);
console.log(`  ${out.public}`);
