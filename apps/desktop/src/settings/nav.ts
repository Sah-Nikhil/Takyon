/**
 * The settings registry: what pages exist and how they are ordered.
 *
 * Pure, and deliberately separate from the components it describes. The sidebar
 * renders whatever this returns, so "a new Source adds a page without touching
 * navigation" is a property of a function rather than a convention nobody checks.
 */

import type { ComponentType } from "react";

/**
 * Tier one is the fixed app-level set above the divider, tier two is one page
 * per feature below it (ROADMAP v0.6). Raycast's structure, and the reason a
 * page can be appended without editing a list of sections.
 */
export type Tier = "app" | "feature";

/**
 * One searchable control on a page.
 *
 * The search box returns *settings*, not page names — past about fifteen pages
 * "Clipboard History" is not what someone is hunting for, "retention" is.
 */
export interface SettingsControl {
  /** Anchor id, so a search result can scroll to the control it named. */
  id: string;
  label: string;
  /** Words a person might search that the label does not contain. */
  keywords?: string[];
}

export interface SettingsPage {
  id: string;
  title: string;
  tier: Tier;
  controls: SettingsControl[];
  Component?: ComponentType;
}

/**
 * One search hit: a control, or a page when the page's own title matched.
 *
 * `controlId` absent means "go to this page"; present means "go to this page and
 * scroll to this control".
 */
export interface SettingsSearchItem {
  pageId: string;
  pageTitle: string;
  controlId?: string;
  label: string;
}

/** Where a hit came from. Lower sorts first, so a label beats a keyword. */
const enum Rank {
  Label = 0,
  Page = 1,
  Keyword = 2,
}

/**
 * Find settings, not pages (task 4).
 *
 * Substring rather than fuzzy: the corpus is a few dozen labels read off the
 * screen, and a fuzzy matcher on that set returns everything for a two-letter
 * query. Revisit past the ~15 pages that made the box necessary.
 */
export function searchSettings(
  query: string,
  pages: readonly SettingsPage[],
): SettingsSearchItem[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];

  const ranked: Array<{ rank: Rank; item: SettingsSearchItem }> = [];

  for (const page of pages) {
    // A page-title match is the page, not each of its controls: matching
    // "Clipboard History" should not spray every clipboard setting into the list.
    if (page.title.toLowerCase().includes(q)) {
      ranked.push({
        rank: Rank.Page,
        item: { pageId: page.id, pageTitle: page.title, label: page.title },
      });
      continue;
    }

    for (const control of page.controls) {
      const rank = control.label.toLowerCase().includes(q)
        ? Rank.Label
        : control.keywords?.some((k) => k.toLowerCase().includes(q))
          ? Rank.Keyword
          : null;
      if (rank === null) continue;

      ranked.push({
        rank,
        item: {
          pageId: page.id,
          pageTitle: page.title,
          controlId: control.id,
          label: control.label,
        },
      });
    }
  }

  return ranked.sort((a, b) => a.rank - b.rank).map((r) => r.item);
}

/**
 * Split the registry into the two tiers the sidebar draws.
 *
 * Tier one keeps declared order because that order is the design: General is
 * first because it is where a person looks first. Tier two sorts alphabetically
 * because it grows without anyone curating it.
 */
export function navSections(pages: readonly SettingsPage[]): {
  app: SettingsPage[];
  feature: SettingsPage[];
} {
  return {
    app: pages.filter((p) => p.tier === "app"),
    feature: pages
      .filter((p) => p.tier === "feature")
      .sort((a, b) => a.title.localeCompare(b.title)),
  };
}
