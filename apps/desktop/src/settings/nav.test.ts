/**
 * The two-tier navigation, as ordering rather than as markup.
 *
 * ROADMAP v0.6 makes one structural promise: every future Source or Mode adds a
 * tier-two page **without touching navigation**. That is a claim about a pure
 * function, so it is tested as one — the sidebar renders whatever this returns.
 */

import { expect, test } from "bun:test";
import { navSections, searchSettings, type SettingsPage } from "./nav";

/** Pages carry no components here: ordering does not depend on what they render. */
const page = (
  id: string,
  title: string,
  tier: SettingsPage["tier"],
  controls: SettingsPage["controls"] = [],
): SettingsPage => ({ id, title, tier, controls });

const PAGES: SettingsPage[] = [
  page("general", "General", "app"),
  page("advanced", "Advanced", "app"),
  page("about", "About", "app"),
  page("clipboard", "Clipboard History", "feature"),
  page("applications", "Applications", "feature"),
];

test("tier one keeps the order it was declared in", () => {
  // Declared order is the design — General is first because it is where a person
  // looks first, not because "General" sorts early.
  expect(navSections(PAGES).app.map((p) => p.title)).toEqual([
    "General",
    "Advanced",
    "About",
  ]);
});

test("tier two is alphabetical whatever order it was declared in", () => {
  expect(navSections(PAGES).feature.map((p) => p.title)).toEqual([
    "Applications",
    "Clipboard History",
  ]);
});

/** The pages search runs over. Controls carry the labels a person actually types. */
const SEARCHABLE: SettingsPage[] = [
  page("general", "General", "app", [
    { id: "autostart", label: "Start Takyon when I log in", keywords: ["login", "startup"] },
    { id: "motion", label: "Turn off animations", keywords: ["reduce motion"] },
  ]),
  page("clipboard", "Clipboard History", "feature", [
    { id: "retention", label: "Keep history for", keywords: ["retention", "delete"] },
  ]),
];

test("an empty query searches nothing, so the sidebar keeps its normal nav", () => {
  expect(searchSettings("", SEARCHABLE)).toEqual([]);
  expect(searchSettings("   ", SEARCHABLE)).toEqual([]);
});

test("a control is found by its label, and says which page it lives on", () => {
  const [hit] = searchSettings("animations", SEARCHABLE);

  expect(hit?.controlId).toBe("motion");
  expect(hit?.pageId).toBe("general");
  expect(hit?.pageTitle).toBe("General");
});

test("a control is found by a word its label does not contain", () => {
  // The whole reason controls carry keywords: "retention" is what someone types
  // and "Keep history for" is what the control says.
  const hits = searchSettings("retention", SEARCHABLE);

  expect(hits.map((h) => h.controlId)).toEqual(["retention"]);
});

test("a page title matches the page itself, not each of its controls", () => {
  const hits = searchSettings("clipboard", SEARCHABLE);

  expect(hits).toHaveLength(1);
  expect(hits[0]?.controlId).toBeUndefined();
  expect(hits[0]?.pageId).toBe("clipboard");
});

test("a label match outranks a keyword match", () => {
  // "Turn off animations" says it; autostart only carries "startup" as a keyword.
  const hits = searchSettings("turn off", SEARCHABLE);
  expect(hits[0]?.controlId).toBe("motion");
});

test("a new Source drops into tier two without touching navigation", () => {
  // The ROADMAP promise, stated as the test that would catch breaking it: v0.7
  // appends File Search and it lands between Clipboard History and nothing else.
  const withFileSearch = [...PAGES, page("file-search", "File Search", "feature")];

  expect(navSections(withFileSearch).feature.map((p) => p.title)).toEqual([
    "Applications",
    "Clipboard History",
    "File Search",
  ]);
  // And tier one is untouched by the addition.
  expect(navSections(withFileSearch).app).toEqual(navSections(PAGES).app);
});
