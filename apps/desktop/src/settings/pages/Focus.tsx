/**
 * Settings → Focus (v0.16): session preferences, the Strict hurdles, Focus
 * categories (built-ins duplicated, custom ones edited, imported, exported),
 * clearing history, and two Coming soon rows.
 *
 * Coming soon is static text inside a working page, never a disabled control:
 * the rule `pages.ts` states, amended for v0.16 by the user's choice.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import type { FocusCategory, FocusSettings } from "@takyon/shared";
import * as api from "@/api";
import { BlockPicker } from "@/focus/BlockPicker";
import { FocusIcon, PICKABLE_ICONS } from "@/focus/icons";
import { keyLabel } from "@/focus/format";
import { Chips, Confirm, Group, Row, Switch } from "../controls";

const SNOOZE = [
  { value: "0", label: "No snooze" },
  { value: "1", label: "1 min" },
  { value: "3", label: "3 min" },
  { value: "5", label: "5 min" },
  { value: "10", label: "10 min" },
] as const;

const HURDLE = [
  { value: "strict", label: "Strict sessions" },
  { value: "always", label: "Every session" },
  { value: "never", label: "Never" },
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
  const [clearing, setClearing] = useState(false);
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

  return (
    <div className="space-y-6">
      <Group title="Sessions">
        <Row id="focus-snooze" label="Snooze" description="How long a blocked app or site comes back for in a Soft session.">
          <Chips label="Snooze" value={String(settings.snoozeMinutes) as (typeof SNOOZE)[number]["value"]} options={SNOOZE} onChange={(v) => set("focus.snooze_minutes", v)} />
        </Row>
        <Row id="focus-restore" label="Reopen apps when a session ends" description="Apps Focus quit start again, once each.">
          <Switch label="Reopen apps when a session ends" checked={settings.restoreApps} onChange={(on) => set("focus.restore_apps", on ? "1" : "0")} />
        </Row>
        <Row id="focus-surface" label="Show a running session as" description="The bar floats on top and never takes focus; the tray keeps it out of sight.">
          <Chips
            label="Show a running session as"
            value={settings.surface}
            options={[
              { value: "bar", label: "Floating bar" },
              { value: "tray", label: "Tray only" },
            ]}
            onChange={(v) => set("focus.surface", v)}
          />
        </Row>
        <Row id="focus-bang" label="Start sessions with !f" description={<>Type <code className="font-mono">!f write report 45m</code> in the Palette to start with your last Block list.</>}>
          <Switch label="Start sessions with !f" checked={settings.bang} onChange={(on) => set("focus.bang", on ? "1" : "0")} />
        </Row>
      </Group>

      <Group title="Hurdles">
        <Row id="focus-hurdle-shutdown" label="Hold shutdown" description="Windows shows your goal and time left, and still offers Shut down anyway.">
          <Chips label="Hold shutdown" value={settings.hurdleShutdown} options={HURDLE} onChange={(v) => set("focus.hurdle_shutdown", v)} />
        </Row>
        <Row id="focus-hurdle-idle" label="Keep the PC awake" description="Idle sleep waits for the session. Sleep from the Start menu or the lid cannot be held; the session pauses instead.">
          <Chips label="Keep the PC awake" value={settings.hurdleIdle} options={HURDLE} onChange={(v) => set("focus.hurdle_idle", v)} />
        </Row>
      </Group>

      <Group title="Focus categories">
        {builtins.map((c) => (
          <div key={c.id} className="flex items-center gap-3 px-3.5 py-2.5">
            <FocusIcon name={c.icon} className="size-4 text-accent" />
            <span className="text-[13px] text-fg">{c.name}</span>
            <span className="flex-1 truncate text-[12px] text-fg/50">
              {c.dynamic === "games" ? "Installed games and launchers" : `${c.items.length} apps and sites`} · built in
            </span>
            <button type="button" className={small} onClick={() => setDraft({ ...c, id: "", name: `${c.name} (copy)`, dynamic: null, copiedFrom: c.id })}>
              Duplicate
            </button>
          </div>
        ))}
        {custom.map((c) =>
          draft?.id === c.id ? (
            <Editor key={c.id} draft={draft} onChange={setDraft} onSave={save} onCancel={() => setDraft(null)} error={error} />
          ) : (
            <div key={c.id} className="flex items-center gap-3 px-3.5 py-2.5">
              <FocusIcon name={c.icon} className="size-4 text-accent" />
              <span className="text-[13px] text-fg">{c.name}</span>
              <span className="flex-1 truncate text-[12px] text-fg/50">{c.items.length} apps and sites</span>
              <button type="button" className={small} onClick={() => setDraft(c)}>
                Edit
              </button>
              <button type="button" className={small} onClick={() => void api.focusDeleteCategory(c.id).then(load)}>
                Delete
              </button>
            </div>
          ),
        )}
        {draft && draft.id === "" && (
          <Editor draft={draft} onChange={setDraft} onSave={save} onCancel={() => setDraft(null)} error={error} />
        )}
        <div className="flex items-center gap-2 px-3.5 py-2.5">
          <button type="button" className={`${small} bg-control`} onClick={() => setDraft({ id: "", name: "", icon: "Target", items: [] })}>
            New Focus category
          </button>
          <span className="flex-1" />
          {note && <span className="text-[12px] text-fg/60">{note}</span>}
          <button type="button" className={small} onClick={() => file.current?.click()}>
            Import…
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
      </Group>

      <Group title="History">
        <Row id="focus-clear" label="Clear Focus history" description="Deletes every finished session and what it blocked. A running session stays.">
          <button type="button" className={small} onClick={() => setClearing(true)}>
            Clear…
          </button>
        </Row>
      </Group>

      <Group title="Coming soon">
        <Row id="focus-scheduled" label="Scheduled sessions" description="Sessions that start themselves, such as weekdays 9 to 12 with Social blocked.">
          <span className="text-[12px] text-fg/50">Coming soon</span>
        </Row>
        <Row id="focus-locked" label="Locked sessions" description="A session that cannot be ended early or escaped through Task Manager. Needs a Windows service (ADR-0036).">
          <span className="text-[12px] text-fg/50">Coming soon</span>
        </Row>
      </Group>

      {clearing && (
        <Confirm
          title="Clear Focus history?"
          consequence="Every finished Focus session and its blocks are deleted. Stats start from zero. This cannot be undone."
          confirmLabel="Clear history"
          onConfirm={() => {
            setClearing(false);
            void api.focusClearHistory();
          }}
          onCancel={() => setClearing(false)}
        />
      )}
    </div>
  );
}
