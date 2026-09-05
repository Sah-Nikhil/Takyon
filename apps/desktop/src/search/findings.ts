/**
 * The `!s` answer, read into the shape it was asked for (v0.9).
 *
 * Arc Search's output and ours: a headline, then labelled one-line findings,
 * each carrying the source numbers it came from. `synth.rs` asks for exactly
 * this, and this reads it back.
 *
 * **Tolerant on purpose.** Every prefix of a streaming answer passes through
 * here, and an Agent that drifts from the shape still has to be readable, so
 * anything unrecognised falls through to `rest` and renders as paragraphs.
 */

export interface Finding {
  /** The two or three words naming the line. Absent on a bare bullet. */
  label?: string;
  detail: string;
  /** Source numbers, 1-based, already checked against the list that exists. */
  cites: number[];
}

export interface ParsedAnswer {
  headline?: string;
  findings: Finding[];
  /** Anything that was not a headline or a finding, by paragraph. */
  rest: string[];
}

const HEADLINE = /^HEADLINE:\s*(.+)$/i;
const BULLET = /^\s*[-*•]\s+(.*)$/;
/** `**Label** — detail`, with an em dash, an en dash, a hyphen or a colon. */
const LABELLED = /^\*\*(.+?)\*\*\s*(?:[—–:-]\s*)?(.*)$/;
const CITE = /\[(\d+)\]/g;

/**
 * Read one answer. `sourceCount` drops citations pointing at nothing, which an
 * Agent does produce when it has been asked for numbers.
 */
export function parseAnswer(text: string, sourceCount = Number.MAX_SAFE_INTEGER): ParsedAnswer {
  const findings: Finding[] = [];
  const rest: string[] = [];
  let headline: string | undefined;

  for (const block of text.split(/\n{2,}/)) {
    for (const line of block.split("\n")) {
      const trimmed = line.trim();
      if (!trimmed) continue;

      const head = HEADLINE.exec(trimmed);
      if (head?.[1] && headline === undefined) {
        headline = head[1].trim();
        continue;
      }

      const bullet = BULLET.exec(trimmed);
      if (!bullet?.[1]) {
        rest.push(trimmed);
        continue;
      }

      const { text: body, cites } = takeCites(bullet[1], sourceCount);
      const labelled = LABELLED.exec(body);
      findings.push(
        labelled?.[1]
          ? { label: labelled[1].trim(), detail: (labelled[2] ?? "").trim(), cites }
          : { detail: body.trim(), cites },
      );
    }
  }

  return { headline, findings, rest };
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
