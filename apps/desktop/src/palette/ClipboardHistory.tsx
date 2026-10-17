/**
 * The Clipboard History surface (v0.5).
 *
 * A full-window View the Palette navigates into, not a second window: a third
 * WebView2 would cost the login budget and a large share of the 150 MB ceiling,
 * for something opened many times a day.
 *
 * Two panes, Raycast's shape. Left is the history grouped by day; right is the
 * selected clip with its metadata. **Only previews are in the list** — the full
 * content is fetched per clip, so a filter never ships every matching secret
 * into the webview. Images (v0.14) arrive by URL through `takyon-clip`.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Trash } from "@phosphor-icons/react";
import { FOOTER_HEIGHT, type Action, type ClipRow } from "@takyon/shared";
import * as api from "@/api";
import { Select } from "@/components/Select";
import { ActionMenu } from "./ActionMenu";
import { formatBytes, neighbourAfterDelete } from "./clips";

/** Type filter, one entry per `ClipKind` that Rust stores. */
const TYPES = [
  { id: "all", label: "All Types" },
  { id: "text", label: "Text" },
  { id: "image", label: "Images" },
] as const;

type TypeId = (typeof TYPES)[number]["id"];

/**
 * Which day bucket a clip belongs to.
 *
 * Local midnight, not a 24-hour window: something copied at 23:50 yesterday is
 * "Yesterday" at 00:10, which is what a person means by the word.
 */
function dayLabel(createdAt: number, now: Date): string {
  const date = new Date(createdAt * 1000);
  const midnight = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const days = Math.floor((midnight.getTime() - date.getTime()) / 86_400_000) + 1;
  if (date >= midnight) return "Today";
  if (days <= 1) return "Yesterday";
  if (days < 7) return date.toLocaleDateString(undefined, { weekday: "long" });
  return date.toLocaleDateString(undefined, { month: "long", day: "numeric" });
}

/** Runs of clips under one day heading, in the order the rows arrived. */
function groupByDay(clips: ClipRow[], now: Date): { day: string; clips: ClipRow[] }[] {
  const groups: { day: string; clips: ClipRow[] }[] = [];
  for (const clip of clips) {
    const day = dayLabel(clip.createdAt, now);
    const last = groups[groups.length - 1];
    if (last && last.day === day) last.clips.push(clip);
    else groups.push({ day, clips: [clip] });
  }
  return groups;
}

/** "Bitwarden" from a full path. The row shows an app, not a location. */
function appName(exe: string | undefined): string {
  if (!exe) return "Unknown";
  const base = exe.split(/[\\/]/).pop() ?? exe;
  return base.replace(/\.exe$/i, "");
}

function Information({ clip }: { clip: ClipRow }) {
  const when = new Date(clip.createdAt * 1000);
  const rows: [string, string][] =
    clip.kind === "image"
      ? [
          ["Source", appName(clip.sourceExe)],
          ["Type", "Image"],
          ["Dimensions", `${clip.width ?? "?"} × ${clip.height ?? "?"}`],
          ["Size", formatBytes(clip.len)],
          ["Copied", when.toLocaleString()],
        ]
      : [
          ["Source", appName(clip.sourceExe)],
          ["Type", "Text"],
          ["Characters", clip.len.toLocaleString()],
          ["Copied", when.toLocaleString()],
        ];
  return (
    <div className="mt-4">
      <div className="mb-2 text-[11px] font-medium uppercase tracking-wide text-fg/50">
        Information
      </div>
      <dl className="space-y-1.5">
        {rows.map(([label, value]) => (
          <div key={label} className="flex items-baseline justify-between gap-4">
            <dt className="shrink-0 text-[12px] text-fg/60">{label}</dt>
            <dd className="truncate text-right text-[12px] text-fg">{value}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

function Hint({ label, keys }: { label: string; keys: string }) {
  return (
    <>
      <span>{label}</span>
      <kbd className="rounded border border-edge bg-key px-1.5 py-0.5 text-[10px] leading-none text-fg/64">
        {keys}
      </kbd>
    </>
  );
}

export function ClipboardHistory({ onClose }: { onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [type, setType] = useState<TypeId>("all");
  const [clips, setClips] = useState<ClipRow[]>([]);
  const [selected, setSelected] = useState<number | null>(null);
  const [menu, setMenu] = useState<Action[] | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  /** `prefer` survives a reload when still listed; else the newest match. */
  const load = useCallback(
    (q: string, prefer?: number | null) => {
      void api.clipPage(q).then((rows) => {
        const shown = type === "all" ? rows : rows.filter((c) => c.kind === type);
        setClips(shown);
        // A selection that filtered itself out would leave the detail pane
        // describing an invisible row.
        setSelected((current) => {
          const want = prefer === undefined ? current : prefer;
          return want !== null && shown.some((c) => c.id === want)
            ? want
            : (shown[0]?.id ?? null);
        });
      });
    },
    [type],
  );

  useEffect(() => load(query), [query, load]);
  useEffect(() => inputRef.current?.focus(), []);

  const groups = useMemo(() => groupByDay(clips, new Date()), [clips]);
  const current = clips.find((c) => c.id === selected);

  /** Move the selection by one row, across day boundaries. */
  const move = useCallback(
    (delta: number) => {
      if (clips.length === 0) return;
      const at = clips.findIndex((c) => c.id === selected);
      const next = Math.min(Math.max(at + delta, 0), clips.length - 1);
      setSelected(clips[next]?.id ?? null);
    },
    [clips, selected],
  );

  const run = useCallback(
    (actionId: string) => {
      if (selected === null) return;
      const next = neighbourAfterDelete(clips, selected);
      void api.activate(`clip:${selected}`, actionId).then(() => {
        // Deleting edits the list being read, so it is the one action that
        // reloads rather than dismissing.
        if (actionId === "delete_clip") load(query, next);
      });
    },
    [selected, clips, load, query],
  );

  const closeMenu = useCallback(() => {
    setMenu(null);
    inputRef.current?.focus();
  }, []);

  /*
    No `setActionMenu`: that resizes the Palette for its list, and this View is
    already `VIEW_HEIGHT`. The menu overlays the surface instead.
   */
  const openMenu = useCallback(() => {
    if (selected === null) return;
    void api.actionsFor(`clip:${selected}`).then((actions) => {
      if (actions.length > 0) setMenu(actions);
    });
  }, [selected]);

  return (
    <div
      className="relative flex h-full w-full flex-col overflow-hidden rounded-xl border border-edge bg-plate shadow-panel"
      onKeyDown={(e) => {
        // The menu handles its own keys; letting them through would move the
        // selection under a menu still describing the old one.
        if (menu) return;
        if (e.key === "Escape") {
          // Back one step, not out: Escape here returns to the search that
          // opened this, the same way it closes the action menu first.
          e.preventDefault();
          e.stopPropagation();
          onClose();
          return;
        }
        if (e.key === "ArrowDown") {
          e.preventDefault();
          move(1);
        } else if (e.key === "ArrowUp") {
          e.preventDefault();
          move(-1);
        } else if (e.key === "Enter") {
          e.preventDefault();
          run(e.ctrlKey ? "copy_clip" : "paste");
        } else if (e.key.toLowerCase() === "d" && e.ctrlKey) {
          e.preventDefault();
          run("delete_clip");
        } else if (e.key.toLowerCase() === "k" && e.ctrlKey) {
          e.preventDefault();
          openMenu();
        }
      }}
    >
      <div className="flex items-center gap-3 border-b border-seam px-3 py-2">
        <button
          type="button"
          onClick={onClose}
          aria-label="Back"
          className="grid size-7 shrink-0 place-items-center rounded-md text-fg/64 hover:bg-row-selected hover:text-fg"
        >
          ←
        </button>
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Type to filter entries..."
          className="h-9 w-full bg-transparent text-[15px] text-fg outline-none placeholder:text-fg/50"
        />
        <Select
          label="Type"
          value={type}
          options={TYPES.map((t) => ({ value: t.id, label: t.label }))}
          onChange={(next: string) => setType(next as TypeId)}
          className="w-32 shrink-0"
        />
      </div>

      <div className="flex min-h-0 flex-1">
        {/*
          `listbox`, because the rows below are `option`s and an option outside
          one does not resolve as a role at all - the selected-state assertion
          reads empty rather than true.
         */}
        <div
          role="listbox"
          aria-label="Clipboard history"
          className="w-[46%] shrink-0 overflow-y-auto border-r border-seam px-2 py-2"
        >
          {clips.length === 0 && (
            <div className="px-2 py-3 text-[13px] text-fg/56">
              {query ? "No matching clips." : "Nothing copied yet."}
            </div>
          )}
          {groups.map((group) => (
            <div key={group.day}>
              <div className="px-2 py-1 text-[11px] text-fg/50">{group.day}</div>
              {group.clips.map((clip) => (
                <button
                  key={clip.id}
                  type="button"
                  role="option"
                  aria-selected={clip.id === selected}
                  data-selected={clip.id === selected || undefined}
                  onClick={() => setSelected(clip.id)}
                  onDoubleClick={() => run("paste")}
                  className="flex w-full items-center gap-2 rounded-md px-2 py-2 text-left data-[selected=true]:bg-row-selected"
                >
                  {clip.kind === "image" && (
                    // Fixed box so a late thumbnail never shifts the text.
                    <img
                      src={api.clipImageUrl(clip.id, "thumb")}
                      alt=""
                      className="h-5 w-8 shrink-0 rounded-sm object-cover"
                      decoding="async"
                    />
                  )}
                  <span className="truncate text-[13px] text-fg">{clip.preview}</span>
                </button>
              ))}
            </div>
          ))}
        </div>

        <div className="min-w-0 flex-1 overflow-y-auto p-4">
          {current ? (
            <>
              {current.kind === "image" ? (
                <div className="grid h-[220px] place-items-center overflow-hidden rounded-lg border border-seam bg-control p-2">
                  <img
                    src={api.clipImageUrl(current.id, "full")}
                    alt={current.preview}
                    className="max-h-full max-w-full object-contain"
                  />
                </div>
              ) : (
                /*
                  `pre`, not a div: a clip is text the user copied and whitespace
                  is part of it. Wrapped rather than scrolled sideways, because a
                  long line read left to right is the common case.
                 */
                <pre className="max-h-[220px] overflow-y-auto whitespace-pre-wrap break-words rounded-lg border border-seam bg-control p-3 font-mono text-[12px] leading-relaxed text-fg/92">
                  {current.preview}
                </pre>
              )}
              <Information clip={current} />
              <button
                type="button"
                onClick={() => run("delete_clip")}
                className="mt-4 flex items-center gap-1.5 rounded-md border border-edge px-2.5 py-1.5 text-[12px] text-fg/72 hover:bg-row-selected hover:text-fg"
              >
                <Trash weight="duotone" className="size-3.5" />
                Delete from history
              </button>
            </>
          ) : (
            <div className="text-[13px] text-fg/56">Select a clip to see it here.</div>
          )}
        </div>
      </div>

      <div
        className="flex shrink-0 items-center justify-between border-t border-seam px-3"
        style={{ height: FOOTER_HEIGHT }}
      >
        <span className="text-[11px] text-fg/60">Clipboard History</span>
        <div className="flex items-center gap-2 text-[11px] text-fg/60">
          <Hint label="Paste" keys="↵" />
          <span aria-hidden className="text-fg/15">
            |
          </span>
          <Hint label="Delete" keys="Ctrl D" />
          <span aria-hidden className="text-fg/15">
            |
          </span>
          <Hint label="Actions" keys="Ctrl K" />
        </div>
      </div>

      {menu && (
        <ActionMenu
          actions={menu}
          onRun={(actionId) => {
            closeMenu();
            run(actionId);
          }}
          onClose={closeMenu}
        />
      )}
    </div>
  );
}
