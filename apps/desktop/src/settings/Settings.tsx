/**
 * The Settings window: sidebar, search, content pane.
 *
 * Structure is Raycast's — a search box over a fixed tier-one list, a divider,
 * then one alphabetical page per feature. Surfaces are t3code's: cards barely
 * lifted off the plate, every separation an alpha of the foreground rather than
 * a grey fill. Hues are Takyon's own, because `docs/brand.md` is still open.
 *
 * The window itself is created by `settings.rs` and destroyed when closed.
 * **Closing it must not end the process** — the Palette is hidden rather than
 * destroyed (ADR-0003), which is what keeps the app alive with no visible window.
 */

import { useEffect, useMemo, useState } from "react";
import { DISPLAY_NAME } from "@takyon/shared";
import { navSections, searchSettings } from "./nav";
import { PAGES } from "./pages";
import { TitleBar } from "./TitleBar";

export function Settings() {
  const [active, setActive] = useState(PAGES[0]!.id);
  const [query, setQuery] = useState("");
  /**
   * The control a search result asked for, so the page can scroll to it.
   *
   * Carries a nonce because picking the same result twice must scroll twice, and
   * an identical value would not re-run the effect.
   */
  const [target, setTarget] = useState<{ id: string; nonce: number } | null>(null);

  const { app, feature } = useMemo(() => navSections(PAGES), []);
  const results = useMemo(() => searchSettings(query, PAGES), [query]);
  const page = PAGES.find((p) => p.id === active) ?? PAGES[0]!;
  const Body = page.Component;

  // After the page has rendered, not during: the anchor does not exist until the
  // new page's rows are in the document. `Row` writes the id, `pages.ts` declares
  // it, and this is the third side of that contract.
  useEffect(() => {
    if (!target) return;
    document.getElementById(`setting-${target.id}`)?.scrollIntoView({ block: "nearest" });
  }, [target]);

  return (
    <div className="flex h-full w-full flex-col bg-plate text-fg">
      <TitleBar title={`${DISPLAY_NAME} Settings`} />
      <div className="flex min-h-0 flex-1">
      <nav className="flex w-54 shrink-0 flex-col gap-1 overflow-y-auto border-r border-hairline bg-sidebar p-2">
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search settings"
          aria-label="Search settings"
          className="mb-2 w-full rounded-control bg-control px-2.5 py-1.5 text-[13px] text-fg outline-none placeholder:text-fg/50"
        />

        {query.trim() ? (
          <SearchResults
            results={results}
            onPick={(pageId, controlId) => {
              setActive(pageId);
              setTarget(controlId ? { id: controlId, nonce: Date.now() } : null);
              setQuery("");
            }}
          />
        ) : (
          <>
            {app.map((p) => (
              <NavItem
                key={p.id}
                title={p.title}
                active={p.id === active}
                onClick={() => setActive(p.id)}
              />
            ))}
            {/*
              The divider is the whole two-tier idea made visible: above it is a
              fixed set that never grows, below it one page per feature.
            */}
            <hr className="my-2 border-hairline" />
            {feature.map((p) => (
              <NavItem
                key={p.id}
                title={p.title}
                active={p.id === active}
                onClick={() => setActive(p.id)}
              />
            ))}
          </>
        )}
      </nav>

      <main className="flex-1 overflow-y-auto px-6 py-5" data-page={page.id}>
        <h1 className="mb-4 text-[15px] font-medium">{page.title}</h1>
        {Body && <Body />}
      </main>
      </div>
    </div>
  );
}

function NavItem({
  title,
  active,
  onClick,
}: {
  title: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={active ? "page" : undefined}
      className={`rounded-control px-2.5 py-1.5 text-left text-[13px] transition-colors ${
        active
          ? "bg-row-selected text-fg"
          : "text-fg/72 hover:bg-row-hover hover:text-fg/90"
      }`}
    >
      {title}
    </button>
  );
}

/**
 * Search results replace the nav rather than sitting under it.
 *
 * They name the page a setting lives on, because "Answer arithmetic" is not
 * where you were going — Calculator is, and the row is how you learn that.
 */
function SearchResults({
  results,
  onPick,
}: {
  results: ReturnType<typeof searchSettings>;
  onPick: (pageId: string, controlId?: string) => void;
}) {
  if (results.length === 0) {
    return <p className="px-2.5 py-2 text-[12.5px] text-fg/56">Nothing matches.</p>;
  }

  return (
    <>
      {results.map((hit) => (
        <button
          key={`${hit.pageId}-${hit.controlId ?? "page"}`}
          type="button"
          onClick={() => onPick(hit.pageId, hit.controlId)}
          className="rounded-control px-2.5 py-1.5 text-left text-[13px] text-fg/84 transition-colors hover:bg-row-hover hover:text-fg"
        >
          <span className="block truncate">{hit.label}</span>
          {hit.controlId && (
            <span className="block truncate text-[11.5px] text-fg/56">{hit.pageTitle}</span>
          )}
        </button>
      ))}
    </>
  );
}
