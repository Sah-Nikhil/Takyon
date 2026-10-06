/** Focus text helpers. Pure, so `bun test` covers them without a DOM. */

import type { FocusCategory, FocusRefusal } from "@takyon/shared";

/** Duration chips on the start form. */
export const presetMinutes = [25, 45, 60, 120];

const pad = (n: number) => String(n).padStart(2, "0");

/** Countdown text. Rounds up: the last second reads 0:01, never 0:00 early. */
export function clock(ms: number): string {
  const s = Math.max(0, Math.ceil(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return h ? `${h}:${pad(m)}:${pad(s % 60)}` : `${m}:${pad(s % 60)}`;
}

/** "45 min", "1 h 30 min": how a preset or a length reads on a chip. */
export function lengthText(ms: number): string {
  const m = Math.round(ms / 60_000);
  const h = Math.floor(m / 60);
  if (!h) return `${m} min`;
  return m % 60 ? `${h} h ${m % 60} min` : `${h} h`;
}

/** Wall-clock time a session ends, in the user's locale. */
export function untilText(endsAt: number): string {
  return new Date(endsAt).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

/**
 * Human name for a Block list key: its label in a Focus category if any,
 * else what the key itself names.
 */
export function keyLabel(key: string, categories: FocusCategory[]): string {
  for (const c of categories) {
    const item = c.items.find((i) => i.key === key);
    if (item) return item.label;
  }
  const [kind, ...rest] = key.split(":");
  const value = rest.join(":");
  switch (kind) {
    case "site":
      return value;
    case "exe":
      return value.replace(/\.exe$/i, "");
    case "package":
      // `5319275a.whatsappdesktop_cv1g1gvanyjgm` -> the name between dot and id.
      // Built-in packages carry a label; this is only the fallback.
      return value.split("_")[0]?.split(".").pop() ?? value;
    case "entry": {
      const path = value.split("|")[0] ?? value;
      if (/^[a-z]:[\\/]/i.test(path) || path.startsWith("\\\\")) {
        return (path.split(/[\\/]/).pop() ?? path).replace(/\.exe$/i, "");
      }
      return path;
    }
    default:
      return key;
  }
}

/** The start form's refusals, one sentence each; protected keys share one. */
export function refusalLines(refusals: FocusRefusal[], categories: FocusCategory[]): string[] {
  const protectedKeys = refusals.flatMap((r) => (r.kind === "protected" ? [keyLabel(r.key, categories)] : []));
  const lines = protectedKeys.length
    ? [`Can't block ${protectedKeys.join(", ")}: part of Windows or Takyon itself.`]
    : [];
  for (const r of refusals) {
    if (r.kind === "elevated") {
      lines.push(`Can't block ${r.name} while it runs as administrator. Reopen it normally first.`);
    } else if (r.kind === "browser") {
      lines.push(
        r.elevated
          ? `Can't block sites in ${r.name} while it runs as administrator.`
          : `Can't block sites in ${r.name}: Focus can't read its address bar.`,
      );
    }
  }
  return lines;
}

/** "X, Twitter, Threads and 2 more": what a Focus category holds, in a line. */
export function contentsText(category: FocusCategory, shown = 3): string {
  const labels = [...new Set(category.items.map((i) => i.label))];
  const games = category.dynamic === "games";
  if (labels.length === 0) return games ? "Every installed game" : "Nothing yet";
  if (games) return `${labels.join(", ")} and every installed game`;
  const head = labels.slice(0, shown);
  const more = labels.length - head.length;
  if (more > 0) return `${head.join(", ")} and ${more} more`;
  return head.length > 1 ? `${head.slice(0, -1).join(", ")} and ${head.at(-1)}` : head[0]!;
}
