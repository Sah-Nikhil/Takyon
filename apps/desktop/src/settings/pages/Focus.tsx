/**
 * Settings → Focus (v0.16): session preferences, Strict hurdles, Focus
 * categories. Raycast's shape: one line per setting, a dropdown on the right,
 * row actions on hover. Coming soon is static text, never a disabled control
 * (`pages.ts`). Clearing history lives on Focus Stats, beside the history.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import type { FocusCategory, FocusSettings } from "@takyon/shared";
import * as api from "@/api";
import { BlockPicker } from "@/focus/BlockPicker";
import { FocusIcon, PICKABLE_ICONS } from "@/focus/icons";
import { keyLabel } from "@/focus/format";
import { Select } from "@/components/Select";
import { Group, Row, Switch } from "../controls";

const SNOOZE = [
  { value: "1", label: "1 minute" },
  { value: "3", label: "3 minutes" },
  { value: "5", label: "5 minutes" },
  { value: "10", label: "10 minutes" },
  { value: "0", label: "No snooze" },
] as const;

const HURDLE = [
  { value: "strict", label: "Strict sessions" },
  { value: "always", label: "Every session" },
  { value: "never", label: "Never" },
] as const;

const SURFACE = [
  { value: "bar", label: "Floating bar" },
  { value: "tray", label: "Tray only" },
] as const;

const small =
  "rounded-control px-2.5 py-1 text-[12.5px] text-fg/80 transition-colors hover:bg-row-hover hover:text-fg";

function Editor({
  draft,
  onChange,
  onSave,
  onCancel,
  error,
}: {
  draft: FocusCategory;
  onChange: (c: FocusCategory) => void;
  onSave: () => void;
  onCancel: () => void;
  error: string | null;
}) {
  return (
    <div className="space-y-3 px-3.5 py-3">
      <div className="flex items-center gap-2">
        <input
          autoFocus
          value={draft.name}
          aria-label="Focus category name"
          placeholder="Name"
          onChange={(e) => onChange({ ...draft, name: e.target.value })}
          className="w-56 rounded-control bg-control px-2.5 py-1 text-[13px] text-fg outline-none placeholder:text-fg/46"
        />
        <div role="radiogroup" aria-label="Icon" className="flex flex-wrap gap-0.5">
          {PICKABLE_ICONS.map((icon) => (
            <button
              key={icon}
              type="button"
              role="radio"
              aria-checked={draft.icon === icon}
              aria-label={icon}
              onClick={() => onChange({ ...draft, icon })}
              className={`grid size-7 place-items-center rounded-control ${
                draft.icon === icon ? "bg-plate ring-1 ring-accent/45" : "hover:bg-row-hover"
              }`}
            >
              <FocusIcon name={icon} className="size-4 text-accent" />
            </button>
          ))}
        </div>
      </div>
      <BlockPicker
        value={{ categories: [], items: draft.items.map((i) => i.key) }}
        categories={[]}
        onChange={(list) =>
          onChange({
            ...draft,
            items: list.items.map((key) => draft.items.find((i) => i.key === key) ?? { key, label: keyLabel(key, []) }),
          })
        }
      />
      {error && (
        <p className="text-[12.5px] text-warning" role="alert">
          {error}
        </p>
      )}
      <div className="flex justify-end gap-2">
        <button type="button" className={small} onClick={onCancel}>
          Cancel
        </button>
        <button type="button" className={`${small} bg-control`} onClick={onSave}>
          Save Focus category
        </button>
      </div>
    </div>
  );
}

export function Focus() {
  const [settings, setSettings] = useState<FocusSettings | null>(null);
  const [builtins, setBuiltins] = useState<FocusCategory[]>([]);
  const [custom, setCustom] = useState<FocusCategory[]>([]);
  const [draft, setDraft] = useState<FocusCategory | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const file = useRef<HTMLInputElement>(null);

  const load = useCallback(() => {
    void api.focusSettings().then(setSettings);
    void api.focusCategories().then((c) => {
      setBuiltins(c.builtins);
      setCustom(c.custom);
    });
  }, []);
  useEffect(load, [load]);

  const set = (key: string, value: string) => void api.focusSetSetting(key, value).then(load);

  const save = () => {
    if (!draft) return;
    api.focusSaveCategory(draft).then(
      () => {
        setDraft(null);
        setError(null);
        load();
      },
      (e) => setError(String(e)),
    );
  };

  const exportAll = () => {
    void api.focusExportCategories([]).then((json) => {
      const url = URL.createObjectURL(new Blob([json], { type: "application/json" }));
      const a = document.createElement("a");
      a.href = url;
      a.download = "takyon-focus-categories.json";
      a.click();
      URL.revokeObjectURL(url);
    });
  };

  const importFile = async (f: File) => {
    try {
      const saved = await api.focusImportCategories(await f.text());
      setNote(`Imported ${saved.length} Focus ${saved.length === 1 ? "category" : "categories"}.`);
      load();
    } catch (e) {
      setNote(String(e));
    }
  };

  if (!settings) return null;

  const row = "group flex items-center gap-3 px-3.5 py-2.5";
  const onHover = "opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100";

  return (
    <div className="space-y-6">
      <Group>
        <Row id="focus-snooze" label="Snooze duration" description="How long a blocked app or site comes back for.">
          <Select
            label="Snooze duration"
            value={String(settings.snoozeMinutes) as (typeof SNOOZE)[number]["value"]}
            options={SNOOZE}
            onChange={(v) => set("focus.snooze_minutes", v)}
            className="w-40"
          />
        </Row>
        <Row id="focus-surface" label="Running session" description="Where the goal and time left stay in view.">
          <Select label="Running session" value={settings.surface} options={SURFACE} onChange={(v) => set("focus.surface", v)} className="w-40" />
        </Row>
        <Row id="focus-restore" label="Restore terminated apps" description="Reopen apps Focus quit when the session ends.">
          <Switch label="Restore terminated apps" checked={settings.restoreApps} onChange={(on) => set("focus.restore_apps", on ? "1" : "0")} />
        </Row>
        <Row id="focus-bang" label="Start with !f" description={<>Type <code className="font-mono">!f write report 45m</code> in the Palette.</>}>
          <Switch label="Start with !f" checked={settings.bang} onChange={(on) => set("focus.bang", on ? "1" : "0")} />
        </Row>
        <Row id="focus-scheduled" label="Scheduled sessions" description="Sessions that start themselves at set times.">
          <span className="text-[12px] text-fg/50">Coming soon</span>
        </Row>
      </Group>

      <Group title="Strict">
        <Row id="focus-hurdle-shutdown" label="Hold shutdown" description="Shows your goal on Windows' shutdown screen.">
          <Select label="Hold shutdown" value={settings.hurdleShutdown} options={HURDLE} onChange={(v) => set("focus.hurdle_shutdown", v)} className="w-40" />
        </Row>
        <Row id="focus-hurdle-idle" label="Keep the PC awake" description="Holds idle sleep off. Sleeping by hand pauses the session.">
          <Select label="Keep the PC awake" value={settings.hurdleIdle} options={HURDLE} onChange={(v) => set("focus.hurdle_idle", v)} className="w-40" />
        </Row>
        <Row id="focus-locked" label="Locked sessions" description="No way out until the clock runs out.">
          <span className="text-[12px] text-fg/50">Coming soon</span>
        </Row>
      </Group>

      <section>
        <div className="mb-2 flex items-center gap-1 px-1">
          <h2 className="flex-1 text-[13px] font-medium text-fg/60">Focus categories</h2>
          {note && <span className="mr-2 text-[12px] text-fg/60">{note}</span>}
          <button type="button" className={small} onClick={() => setDraft({ id: "", name: "", icon: "Target", items: [] })}>
            New
          </button>
          <button type="button" className={small} onClick={() => file.current?.click()}>
            Import
          </button>
          <button type="button" className={small} disabled={custom.length === 0} onClick={exportAll}>
            Export
          </button>
          <input
            ref={file}
            type="file"
            accept="application/json,.json"
            className="hidden"
            aria-label="Import Focus categories"
            onChange={(e) => {
              const f = e.target.files?.[0];
              if (f) void importFile(f);
              e.target.value = "";
            }}
          />
        </div>
        <div className="divide-y divide-hairline overflow-hidden rounded-card border border-hairline bg-card">
          {draft && draft.id === "" && (
            <Editor draft={draft} onChange={setDraft} onSave={save} onCancel={() => setDraft(null)} error={error} />
          )}
          {custom.map((c) =>
            draft?.id === c.id ? (
              <Editor key={c.id} draft={draft} onChange={setDraft} onSave={save} onCancel={() => setDraft(null)} error={error} />
            ) : (
              <div key={c.id} className={row}>
                <FocusIcon name={c.icon} className="size-4 text-accent" />
                <span className="text-[13px] text-fg">{c.name}</span>
                <span className="flex-1 truncate text-[12px] text-fg/50">{c.items.length} apps and sites</span>
                <button type="button" className={`${small} ${onHover}`} onClick={() => setDraft(c)}>
                  Edit
                </button>
                <button type="button" className={`${small} ${onHover}`} onClick={() => void api.focusDeleteCategory(c.id).then(load)}>
                  Delete
                </button>
              </div>
            ),
          )}
          {builtins.map((c) => (
            <div key={c.id} className={row}>
              <FocusIcon name={c.icon} className="size-4 text-accent" />
              <span className="text-[13px] text-fg">{c.name}</span>
              <span className="flex-1 truncate text-[12px] text-fg/50">
                {c.dynamic === "games" ? "Installed games and launchers" : `${c.items.length} apps and sites`}
              </span>
              <button
                type="button"
                className={`${small} ${onHover}`}
                onClick={() => setDraft({ ...c, id: "", name: `${c.name} (copy)`, dynamic: null, copiedFrom: c.id })}
              >
                Duplicate
              </button>
              <span className="w-14 text-right text-[11px] text-fg/40">Built in</span>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}
