/**
 * Grouping the Entry list by Kind, for Expanded mode (v0.10).
 *
 * Pure, so the ordering rule is testable without a window.
 *
 * **The ranking still decides.** Grouping only labels: a group's position is the
 * position of its best Entry, and within a group the ranked order is untouched.
 * Sorting groups by anything else — Kind precedence, alphabetically — would mean
 * the best answer to a query is not the first thing on screen, which is the one
 * thing a launcher may never do.
 */

import type { Entry, EntryKind } from "@takyon/shared";

/**
 * What each Kind's section is called.
 *
 * Plural, because it names a set rather than a row — `EntryRow`'s own
 * `KIND_LABEL` is singular for the opposite reason. `calc` has no heading: a
 * calculation is drawn as a card and there is never more than one.
 */
export const GROUP_LABEL: Partial<Record<EntryKind, string>> = {
  app: "Applications",
  file: "Files",
  folder: "Folders",
  recent: "Recent",
  system: "Settings",
  systemTask: "Tasks",
  clip: "Clipboard",
  command: "Commands",
};

export interface EntryGroup {
  kind: EntryKind;
  /** Absent for `calc`, which is a card rather than a labelled section. */
  label?: string;
  entries: Entry[];
}

/**
 * Split a ranked list into sections, first-appearance order.
 *
 * A Kind that reappears joins the group it already has: two applications either
 * side of a file are one Applications section, not two. First appearance keeps
 * the winner on top while still collecting the rest.
 */
export function groupEntries(entries: Entry[]): EntryGroup[] {
  const groups: EntryGroup[] = [];
  const byKind = new Map<EntryKind, EntryGroup>();
  for (const entry of entries) {
    let group = byKind.get(entry.kind);
    if (!group) {
      group = { kind: entry.kind, label: GROUP_LABEL[entry.kind], entries: [] };
      byKind.set(entry.kind, group);
      groups.push(group);
    }
    group.entries.push(entry);
  }
  return groups;
}
