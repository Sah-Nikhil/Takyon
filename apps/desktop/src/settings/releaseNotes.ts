/**
 * Release notes, read from `CHANGELOG.md` (bundled, so offline).
 *
 * Shape follows t3code's update popover: a version heading, one bullet per
 * feature with its label set in bold, then a link to the release on GitHub.
 * Bullets are `- **Label.** Sentence.` (the convention CHANGELOG.md states);
 * an unlabelled bullet still renders, as plain text.
 */

/** Where every release is published. `CLAUDE.md` names the remote. */
export const RELEASES_URL = "https://github.com/Sah-Nikhil/Takyon/releases";

/** Versions the view lists before pointing at GitHub for the rest. */
export const SHOWN_RELEASES = 6;

export interface ReleaseItem {
  /** The bold lead, final period dropped. Absent on an unlabelled bullet. */
  label?: string;
  text: string;
}

export interface ReleaseNote {
  version: string;
  title?: string;
  items: ReleaseItem[];
  /** Paragraphs after the bullets: "Still open", downgrade warnings. */
  notes: string[];
}

const HEADING = /^## v(\d+\.\d+(?:\.\d+)?)\s*(?:[—-]\s*(.+))?$/;
const LABELLED = /^\*\*(.+?)\*\*\s*(.*)$/s;

export const releaseUrl = (version: string) => `${RELEASES_URL}/tag/v${version}`;

/** One section's body: bullets and paragraphs, wrapped lines joined. */
export function parseSection(body: string): Pick<ReleaseNote, "items" | "notes"> {
  const items: ReleaseItem[] = [];
  const notes: string[] = [];
  let open: string[] | null = null;
  let kind: "item" | "note" = "note";
  const close = () => {
    if (!open) return;
    const text = open.join(" ").replace(/\s+/g, " ").trim();
    open = null;
    if (!text) return;
    if (kind === "note") {
      notes.push(text);
      return;
    }
    const m = LABELLED.exec(text);
    items.push(m ? { label: m[1]!.replace(/\.$/, ""), text: m[2]!.trim() } : { text });
  };
  for (const raw of body.split(/\r?\n/)) {
    const line = raw.trimEnd();
    if (/^[-*] /.test(line)) {
      close();
      kind = "item";
      open = [line.slice(2)];
    } else if (!line.trim()) {
      close();
    } else if (open && (kind === "note" || /^\s/.test(line))) {
      open.push(line.trim());
    } else {
      close();
      kind = "note";
      open = [line.trim()];
    }
  }
  close();
  return { items, notes };
}

/** Released versions, newest first. `## Unreleased` and the preamble are skipped. */
export function parseChangelog(changelog: string): ReleaseNote[] {
  const out: ReleaseNote[] = [];
  let current: { version: string; title?: string; body: string[] } | null = null;
  const flush = () => {
    if (current) out.push({ version: current.version, title: current.title, ...parseSection(current.body.join("\n")) });
  };
  for (const line of changelog.split(/\r?\n/)) {
    if (line.startsWith("## ")) {
      flush();
      const m = HEADING.exec(line.trim());
      current = m ? { version: m[1]!, title: m[2]?.trim(), body: [] } : null;
    } else if (current) {
      current.body.push(line);
    }
  }
  flush();
  return out.filter((r) => r.items.length > 0 || r.notes.length > 0);
}
