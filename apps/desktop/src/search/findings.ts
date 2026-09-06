/**
 * The `!s` answer, read into the shape it was asked for (v0.9, extended v0.10).
 *
 * Arc Search's output and ours: a headline, optional `##` sections, then
 * labelled one-line findings, each with an icon naming its kind and the source
 * numbers it came from. `synth.rs` asks for exactly this, and this reads it back.
 *
 * **Tolerant on purpose.** Every prefix of a streaming answer passes through
 * here, and an Agent that drifts from the shape still has to be readable, so
 * anything unrecognised falls through to a section's `rest` and renders as
 * paragraphs.
 */

export interface Finding {
  /** The two or three words naming the line. Absent on a bare bullet. */
  label?: string;
  detail: string;
  /** Source numbers, 1-based, already checked against the list that exists. */
  cites: number[];
  /**
   * Which icon names this line's kind, from the vocabulary `synth.rs` offers
   * (ADR-0022). Absent where the Agent wrote none or invented one, so the row
   * draws its neutral icon rather than a literal token.
   */
  icon?: string;
}

/** One `## heading` and everything under it. The first section has no heading. */
export interface Section {
  heading?: string;
  findings: Finding[];
  /** Anything in this section that was not a finding, by paragraph. */
  rest: string[];
}

export interface ParsedAnswer {
  headline?: string;
  sections: Section[];
}

const HEADLINE = /^HEADLINE:\s*(.+)$/i;
const BULLET = /^\s*[-*•]\s+(.*)$/;
/** `**Label** — detail`, with an em dash, an en dash, a hyphen or a colon. */
const LABELLED = /^\*\*(.+?)\*\*\s*(?:[—–:-]\s*)?(.*)$/;
const CITE = /\[(\d+)\]/g;
/** `## Section name`. One or three hashes too, because Agents drift. */
const SECTION = /^#{1,3}\s+(.+?)\s*#*$/;
/** `{token}` at the front of a finding, naming its icon. */
const ICON_TOKEN = /^\{([a-z-]{1,20})\}\s*/;

/**
 * The icon vocabulary, mirroring the list `synth.rs` puts in the prompt.
 *
 * A closed set on purpose: an Agent asked for a free-form name invents one every
 * few answers, and a name with nothing behind it is a blank gutter.
 */
export const ICONS: ReadonlySet<string> = new Set([
  "score",
  "money",
  "time",
  "date",
  "place",
  "person",
  "group",
  "car",
  "chart",
  "up",
  "down",
  "warning",
  "check",
  "cross",
  "question",
  "quote",
  "list",
  "star",
  "globe",
  "book",
  "tool",
  "fire",
  "shield",
  "disagree",
  "unknown",
  "egg",
  "food",
  "health",
  "code",
  "music",
]);

/**
 * Read one answer. `sourceCount` drops citations pointing at nothing, which an
 * Agent does produce when it has been asked for numbers.
 */
export function parseAnswer(text: string, sourceCount = Number.MAX_SAFE_INTEGER): ParsedAnswer {
  const sections: Section[] = [{ findings: [], rest: [] }];
  let headline: string | undefined;
  const current = () => sections[sections.length - 1]!;

  for (const block of text.split(/\n{2,}/)) {
    for (const line of block.split("\n")) {
      const trimmed = line.trim();
      if (!trimmed) continue;

      const head = HEADLINE.exec(trimmed);
      if (head?.[1] && headline === undefined) {
        headline = head[1].trim();
        continue;
      }

      const section = SECTION.exec(trimmed);
      if (section?.[1]) {
        sections.push({ heading: section[1].trim(), findings: [], rest: [] });
        continue;
      }

      const bullet = BULLET.exec(trimmed);
      if (!bullet?.[1]) {
        current().rest.push(trimmed);
        continue;
      }

      const { text: body, cites } = takeCites(bullet[1], sourceCount);
      // Taken before the label, because the token sits in front of the `**`.
      const marked = ICON_TOKEN.exec(body);
      const icon = marked?.[1] && ICONS.has(marked[1]) ? marked[1] : undefined;
      const rest = marked ? body.slice(marked[0].length) : body;

      const labelled = LABELLED.exec(rest);
      current().findings.push(
        labelled?.[1]
          ? { label: labelled[1].trim(), detail: (labelled[2] ?? "").trim(), cites, icon }
          : { detail: rest.trim(), cites, icon },
      );
    }
  }

  // A leading section with nothing in it is what "headline then heading"
  // produces, and it would draw as a gap above the first real heading.
  return {
    headline,
    sections: sections.filter((s, i) => i === 0 || s.heading || s.findings.length || s.rest.length),
  };
}

/**
 * Split the `[1][3]` that trails a line, keeping only numbers that resolve.
 *
 * **Trailing only.** A citation written mid-sentence is part of the sentence,
 * and lifting it out leaves "calls it a muffed catch; scores it a fumble",
 * which is prose with its subjects deleted.
 */
function takeCites(line: string, sourceCount: number): { text: string; cites: number[] } {
  const trailing = /(?:\s*\[\d+\])+\s*$/.exec(line);
  if (!trailing) return { text: line.trim(), cites: [] };

  const cites: number[] = [];
  for (const match of trailing[0].matchAll(CITE)) {
    const n = Number(match[1]);
    if (n >= 1 && n <= sourceCount && !cites.includes(n)) cites.push(n);
  }
  return { text: line.slice(0, trailing.index).trim(), cites };
}
