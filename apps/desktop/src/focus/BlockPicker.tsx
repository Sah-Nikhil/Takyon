/**
 * The Block list field: chips for what is chosen, a search for what is not.
 *
 * Suggestions are Focus categories, applications (from the ordinary Bangless
 * query, apps only) and a last "Block website" row once the text looks like a
 * domain. Raycast's shape (its Focus form, 2026). With `onPick`, it is only an
 * adder: picks go to the caller and no chips are drawn (a category's page).
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { X } from "@phosphor-icons/react";
import type { BlockList, Entry, FocusCategory } from "@takyon/shared";
import * as api from "@/api";
import { Favicon } from "@/search/Favicon";
import { FocusIcon } from "./icons";
import { contentsText, keyLabel } from "./format";

type Suggestion =
  | { kind: "category"; category: FocusCategory }
  | { kind: "app"; entry: Entry }
  | { kind: "site"; domain: string };

/** A domain-looking word: has a dot, no spaces. Rust has the final say. */
function domainOf(text: string): string | null {
  const t = text.trim().toLowerCase().replace(/^https?:\/\//, "").replace(/^www\./, "").split("/")[0] ?? "";
  return /^[a-z0-9-]+(\.[a-z0-9-]+)+$/.test(t) ? t : null;
}

export function BlockPicker({
  value,
  categories,
  onChange,
  onPick,
  placeholder = "Apps, websites, Focus categories…",
}: {
  value: BlockList;
  categories: FocusCategory[];
  onChange?: (next: BlockList) => void;
  onPick?: (key: string, label: string) => void;
  placeholder?: string;
}) {
  const [text, setText] = useState("");
  // Keyed by the query that produced them, so a stale or short query shows none.
  const [found, setFound] = useState<{ q: string; apps: Entry[] }>({ q: "", apps: [] });
  const [active, setActive] = useState(0);
  // Bumped when a just-added site's icon lands (ADR-0037).
  const [epoch, setEpoch] = useState(0);
  const [open, setOpen] = useState(false);
  const seq = useRef(0);

  useEffect(() => {
    const q = text.trim();
    if (q.length < 2) return;
    const mine = ++seq.current;
    void api.query(q, mine).then((r) => {
      if (mine === seq.current) setFound({ q, apps: r.entries.filter((e) => e.kind === "app").slice(0, 5) });
    });
  }, [text]);
  const apps = useMemo(() => (found.q === text.trim() ? found.apps : []), [found, text]);

  const suggestions = useMemo<Suggestion[]>(() => {
    const q = text.trim().toLowerCase();
    const out: Suggestion[] = [];
    for (const c of categories) {
      if (value.categories.includes(c.id)) continue;
      if (!q || c.name.toLowerCase().includes(q)) out.push({ kind: "category", category: c });
    }
    for (const entry of apps) {
      if (!value.items.includes(`entry:${entry.id}`)) out.push({ kind: "app", entry });
    }
    const domain = domainOf(text);
    if (domain && !value.items.includes(`site:${domain}`)) out.push({ kind: "site", domain });
    return out;
  }, [apps, categories, text, value]);


  const add = (s: Suggestion) => {
    setText("");
    if (s.kind === "site") void api.focusFetchIcons([`site:${s.domain}`]).then((n) => n > 0 && setEpoch((e) => e + 1));
    if (onPick) {
      if (s.kind === "app") onPick(`entry:${s.entry.id}`, s.entry.title);
      if (s.kind === "site") onPick(`site:${s.domain}`, s.domain);
      return;
    }
    if (s.kind === "category") onChange?.({ ...value, categories: [...value.categories, s.category.id] });
    if (s.kind === "app") onChange?.({ ...value, items: [...value.items, `entry:${s.entry.id}`] });
    if (s.kind === "site") onChange?.({ ...value, items: [...value.items, `site:${s.domain}`] });
  };

  const removeCategory = (id: string) =>
    onChange?.({ ...value, categories: value.categories.filter((c) => c !== id) });
  const removeItem = (key: string) => onChange?.({ ...value, items: value.items.filter((k) => k !== key) });

  // Labels for app chips picked this session; later reads fall back to the key.
  const [appTitles, setAppTitles] = useState<Record<string, string>>({});
  const remember = (s: Suggestion) => {
    if (s.kind === "app") setAppTitles((t) => ({ ...t, [`entry:${s.entry.id}`]: s.entry.title }));
  };

  const chip = "flex h-7 items-center gap-1.5 rounded-md bg-control pl-2 pr-1 text-[13px] text-fg";
  const showList = open && suggestions.length > 0;

  return (
    <div className="relative">
      <div
        role="group"
        aria-label="Block list"
        className={`flex min-h-10 flex-wrap items-center gap-1.5 rounded-lg border bg-control/40 px-2 py-1.5 ${
          open ? "border-accent/60" : "border-edge"
        }`}
      >
        {value.categories.map((id) => {
          const c = categories.find((x) => x.id === id);
          return (
            <span key={id} className={chip} title={c ? contentsText(c, 12) : undefined}>
              <FocusIcon name={c?.icon} className="size-4 text-accent" />
              {c?.name ?? id}
              <button type="button" aria-label={`Remove ${c?.name ?? id}`} onClick={() => removeCategory(id)} className="grid size-5 place-items-center rounded text-fg/50 hover:bg-row-selected hover:text-fg">
                <X className="size-3" />
              </button>
            </span>
          );
        })}
        {value.items.map((key) => (
          <span key={key} className={chip}>
            {key.startsWith("site:") && <Favicon host={key.slice(5)} size={16} epoch={epoch} />}
            <span>{appTitles[key] ?? keyLabel(key, categories)}</span>
            <button type="button" aria-label={`Remove ${keyLabel(key, categories)}`} onClick={() => removeItem(key)} className="grid size-5 place-items-center rounded text-fg/50 hover:bg-row-selected hover:text-fg">
              <X className="size-3" />
            </button>
          </span>
        ))}
        <input
          value={text}
          aria-label={onPick ? "Add apps or websites" : "Add apps, websites or Focus categories"}
          placeholder={value.categories.length + value.items.length ? "" : placeholder}
          onChange={(e) => {
            setText(e.target.value);
            setActive(0);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onBlur={() => setTimeout(() => setOpen(false), 120)}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown" && showList) {
              e.preventDefault();
              e.stopPropagation();
              setActive((a) => Math.min(a + 1, suggestions.length - 1));
            } else if (e.key === "ArrowUp" && showList) {
              e.preventDefault();
              e.stopPropagation();
              setActive((a) => Math.max(a - 1, 0));
            } else if (e.key === "Enter" && !e.ctrlKey && showList) {
              e.preventDefault();
              e.stopPropagation();
              const s = suggestions[active];
              if (s) {
                remember(s);
                add(s);
              }
            } else if (e.key === "Backspace" && !text) {
              const lastItem = value.items[value.items.length - 1];
              if (lastItem) removeItem(lastItem);
              else if (value.categories.length) removeCategory(value.categories[value.categories.length - 1]!);
            }
          }}
          className="h-7 min-w-32 flex-1 bg-transparent text-[13px] text-fg outline-none placeholder:text-fg/45"
        />
      </div>

      {showList && (
        <div role="listbox" aria-label="Suggestions" className="absolute inset-x-0 top-full z-10 mt-1.5 max-h-56 overflow-y-auto rounded-lg border border-edge bg-card p-1 shadow-panel">
          {suggestions.map((s, i) => (
            <button
              key={s.kind === "category" ? s.category.id : s.kind === "app" ? s.entry.id : s.domain}
              type="button"
              role="option"
              aria-selected={i === active}
              data-selected={i === active || undefined}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => {
                remember(s);
                add(s);
              }}
              className="flex h-9 w-full items-center gap-2.5 rounded-md px-2.5 text-left text-[13px] text-fg data-[selected=true]:bg-row-selected"
            >
              {s.kind === "category" && (
                <>
                  <FocusIcon name={s.category.icon} className="size-4 text-accent" />
                  <span className="shrink-0">{s.category.name}</span>
                  <span className="min-w-0 flex-1 truncate text-right text-[11px] text-fg/50">{contentsText(s.category)}</span>
                </>
              )}
              {s.kind === "app" && (
                <>
                  <img src={api.iconUrl(s.entry.icon)} alt="" className="size-4" />
                  <span className="flex-1 truncate">{s.entry.title}</span>
                  <span className="text-[11px] text-fg/50">Application</span>
                </>
              )}
              {s.kind === "site" && (
                <>
                  <Favicon host={s.domain} size={16} />
                  <span className="flex-1 truncate">Block website: {s.domain}</span>
                </>
              )}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
