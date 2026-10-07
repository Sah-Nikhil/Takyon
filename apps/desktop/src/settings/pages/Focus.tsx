/**
 * Settings → Focus (v0.16): session preferences, Strict hurdles, Focus
 * categories. Raycast's shape: one line per setting, a dropdown on the right,
 * row actions on hover. Coming soon is static text, never a disabled control
 * (`pages.ts`). Stats and each Focus category are sub-views: "Focus / Stats",
 * "Focus / Social".
 */

import { useCallback, useContext, useEffect, useRef, useState } from "react";
import { CaretRight } from "@phosphor-icons/react";
import type { FocusCategory, FocusSettings } from "@takyon/shared";
import * as api from "@/api";
import { FocusIcon } from "@/focus/icons";
import { Select } from "@/components/Select";
import { Group, Row, Switch } from "../controls";
import { CrumbContext } from "../crumb";
import { FocusCategoryPanel } from "./FocusCategoryPanel";
import { FocusStatsPanel, hours } from "./FocusStats";

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

export function Focus() {
  const [settings, setSettings] = useState<FocusSettings | null>(null);
  const [builtins, setBuiltins] = useState<FocusCategory[]>([]);
  const [custom, setCustom] = useState<FocusCategory[]>([]);
  const [note, setNote] = useState<string | null>(null);
  const file = useRef<HTMLInputElement>(null);
  /** `main`, `stats`, or a Focus category's id. */
  const [view, setView] = useState("main");
  const [week, setWeek] = useState<number | null>(null);
  const setCrumb = useContext(CrumbContext);

  const load = useCallback(() => {
    void api.focusSettings().then(setSettings);
    void api.focusCategories().then((c) => {
      setBuiltins(c.builtins);
      setCustom(c.custom);
    });
  }, []);
  useEffect(load, [load]);
  useEffect(() => {
    void api.focusStats(7).then((s) => setWeek(s.focusedMs));
  }, []);

  const back = () => {
    setView("main");
    setCrumb(null);
    load();
  };
  const openStats = () => {
    setView("stats");
    setCrumb({ label: "Stats", back });
  };
  const openCategory = (c: FocusCategory) => {
    setView(c.id);
    setCrumb({ label: c.name, back });
  };
  const newCategory = () => {
    const names = new Set(custom.map((c) => c.name));
    let name = "New Focus category";
    for (let n = 2; names.has(name); n++) name = `New Focus category (${n})`;
    void api.focusSaveCategory({ id: "", name, icon: "Target", items: [] }).then(openCategory);
  };

  const set = (key: string, value: string) => void api.focusSetSetting(key, value).then(load);

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
  if (view === "stats") return <FocusStatsPanel />;
  if (view !== "main")
    return (
      <FocusCategoryPanel
        key={view}
        id={view}
        onOpen={openCategory}
        onGone={back}
        onRename={(label) => setCrumb({ label, back })}
      />
    );

  const row =
    "flex w-full items-center gap-3 px-3.5 py-2.5 text-left transition-colors hover:bg-row-hover focus-visible:bg-row-hover";

  return (
    <div className="space-y-6">
      <button
        id="setting-focus-stats"
        type="button"
        onClick={openStats}
        className="flex w-full items-center gap-3 rounded-card border border-hairline bg-card px-3.5 py-3 text-left transition-colors hover:bg-row-hover"
      >
        <span className="flex-1">
          <span className="block text-[13px] text-fg">Focus stats</span>
          <span className="block text-[12px] text-fg/55">
            {week === null ? "Focused time, per day and per Focus category." : `${hours(week)} focused in the last 7 days`}
          </span>
        </span>
        <CaretRight weight="duotone" className="size-4 text-fg/45" />
      </button>

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
          <button type="button" className={small} onClick={newCategory}>
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
          {[...custom, ...builtins].map((c) => (
            <button key={c.id} type="button" className={row} onClick={() => openCategory(c)}>
              <FocusIcon name={c.icon} className="size-4 shrink-0 text-accent" />
              <span className="shrink-0 text-[13px] text-fg">{c.name}</span>
              <span className="min-w-0 flex-1 truncate text-[12px] text-fg/50">
                {c.dynamic === "games" ? "Installed games and launchers" : `${c.items.length} apps and sites`}
              </span>
              {!c.id.startsWith("custom:") && <span className="shrink-0 text-[11px] text-fg/40">Built in</span>}
              <CaretRight weight="duotone" className="size-3.5 shrink-0 text-fg/40" />
            </button>
          ))}
        </div>
      </section>
    </div>
  );
}
