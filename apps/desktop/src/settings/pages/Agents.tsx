/**
 * Agents: one row each, switched on or off, ranked in the order `!c` walks.
 *
 * **The switch is what makes the Palette instant.** `!c` reads the order and the
 * switches — both stored preferences — so it names its Agent on the first
 * keystroke. Sign-in state could not do that job: reading it costs a process.
 *
 * **The model and effort are locked here and nowhere else.** Whatever a row says
 * is what every Turn uses; `agent_ask` reads the pair from `settings.db` rather
 * than taking it from the frontend.
 *
 * **There is no Sign in button, deliberately.** Takyon reads an Agent's Sign-in
 * state and never changes it (ADR-0017); the row carries the command to run.
 */

import { useCallback, useEffect, useState } from "react";
import type { AgentKind, AgentSettings, AgentSnapshot } from "@takyon/shared";

import * as api from "@/api";
import { AGENT_LABELS, agentSummary, canAsk, HEALTH_DOT, versionLabel } from "@/agents/status";
import { Group, Row, Switch, useApplied } from "../controls";

/** Shown while `agent_settings` is still out. Rust's order, same reason. */
const DEFAULT_ORDER: AgentKind[] = ["claude", "codex", "opencode"];

export function Agents() {
  const [settings, setSettings] = useState<AgentSettings | null>(null);
  const [snapshots, setSnapshots] = useState<AgentSnapshot[] | null>(null);
  const [cwd, setCwd] = useState("");

  // Three process spawns, so on mount and on demand — never per keystroke and
  // never at login (v0.9 Traps).
  const probe = useCallback(() => {
    setSnapshots(null);
    void api.agentSnapshots().then(setSnapshots);
  }, []);

  // Fetched here rather than through `probe`, which resets the rows to their
  // "Checking…" state synchronously — on mount they are already in it.
  useEffect(() => {
    void api.agentSettings().then((next) => {
      setSettings(next);
      setCwd(next.cwd);
    });
    void api.agentSnapshots().then(setSnapshots);
  }, []);

  const cwdApplied = useApplied(api.setAskCwd, async () => (await api.agentSettings()).cwd);
  const order = settings?.order ?? DEFAULT_ORDER;

  const reorder = (from: number, to: number) => {
    const moved = order[from];
    if (!moved) return;
    const next = [...order];
    next.splice(from, 1);
    next.splice(to, 0, moved);
    setSettings((s) => (s ? { ...s, order: next } : s));
    void api.setAskOrder(next);
  };

  const toggle = (agent: AgentKind, on: boolean) => {
    setSettings((s) => (s ? { ...s, enabled: { ...s.enabled, [agent]: on } } : s));
    void api.setAskEnabled(agent, on);
  };

  return (
    <>
      <Group>
        <div className="flex items-start justify-between px-3.5 py-3">
          <div className="min-w-0 flex-1 basis-64">
            <span className="text-[14px] text-fg">Ask !c with</span>
            <p className="mt-1 text-[12.5px] leading-snug text-fg/45">
              Takyon runs the agent you already installed and signed in to. It never holds an
              account or a key of its own. `!c` asks the first agent switched on here and works
              down the list.
            </p>
          </div>
          <button
            type="button"
            onClick={probe}
            className="ms-4 shrink-0 rounded-md border border-white/10 px-2.5 py-1 text-[12px] text-fg/70 hover:text-fg"
          >
            {snapshots === null ? "Checking…" : "Check again"}
          </button>
        </div>
        {order.map((agent, i) => (
          <AgentRow
            key={agent}
            agent={agent}
            rank={i + 1}
            enabled={settings?.enabled[agent] ?? true}
            snapshot={snapshots?.find((s) => s.kind === agent)}
            model={settings?.models[agent] ?? ""}
            effort={settings?.efforts[agent] ?? ""}
            canMoveUp={i > 0}
            canMoveDown={i < order.length - 1}
            onMove={(delta) => reorder(i, i + delta)}
            onToggle={(on) => toggle(agent, on)}
            onSettings={(next) => setSettings((s) => (s ? next(s) : s))}
          />
        ))}
      </Group>

      <Group>
        <Row
          id="ask-cwd"
          label="Run agents in"
          applied={cwdApplied.applied}
          error={cwdApplied.error}
          description="Leave this blank and every question is answered in an empty scratch folder. An agent pointed at a folder you did not choose is a bad surprise, so the default is one with nothing in it."
        >
          <input
            value={cwd}
            onChange={(e) => setCwd(e.target.value)}
            onBlur={() => void cwdApplied.apply(cwd.trim(), setCwd)}
            spellCheck={false}
            placeholder={settings?.scratch ?? ""}
            aria-label="Run agents in"
            className="h-8 w-80 max-w-full rounded-md border border-white/10 bg-black/20 px-2.5 text-[13px] text-fg outline-none placeholder:text-fg/30"
          />
        </Row>
      </Group>
    </>
  );
}

/**
 * One Agent: rank, name, state, switch, and its locked pair.
 *
 * A switched-off Agent keeps its status line but loses its pickers — a model
 * chosen for an Agent `!c` will never reach is a control that means nothing.
 */
function AgentRow({
  agent,
  rank,
  enabled,
  snapshot,
  model,
  effort,
  canMoveUp,
  canMoveDown,
  onMove,
  onToggle,
  onSettings,
}: {
  agent: AgentKind;
  rank: number;
  enabled: boolean;
  snapshot: AgentSnapshot | undefined;
  model: string;
  effort: string;
  canMoveUp: boolean;
  canMoveDown: boolean;
  onMove: (delta: number) => void;
  onToggle: (on: boolean) => void;
  onSettings: (next: (s: AgentSettings) => AgentSettings) => void;
}) {
  const [models, setModels] = useState<string[] | null>(null);
  const label = snapshot?.label ?? AGENT_LABELS[agent];
  const summary = agentSummary(snapshot);
  const version = versionLabel(snapshot?.version);
  // Pickers only where they can mean something: switched on, installed, and not
  // signed out. Everything else gets the sentence that says what to do instead.
  const usable = enabled && canAsk(snapshot);

  // One spawn, and only once the Agent is known to be usable — a signed-out
  // Agent's model list is a question nobody asked.
  useEffect(() => {
    if (!usable) return;
    let live = true;
    void api.agentModels(agent).then((list) => {
      if (live) setModels(list);
    });
    return () => {
      live = false;
    };
  }, [agent, usable]);

  return (
    <div className="px-3.5 py-3">
      {/*
        Two lines, not one wrapping row: the pickers are wider than the space
        left beside the status text, and letting them share it dropped the
        switch into the middle of the card.
      */}
      <div className="flex items-start gap-3">
        <span className="w-4 shrink-0 pt-0.5 text-end text-[12px] tabular-nums text-fg/30">
          {rank}
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className={`text-[14px] ${enabled ? "text-fg" : "text-fg/40"}`}>{label}</span>
            {version && <span className="text-[11.5px] text-fg/30">{version}</span>}
          </div>
          <div className="mt-1 flex items-start gap-2">
            <span
              aria-hidden
              className={`mt-1.5 size-2 shrink-0 rounded-full ${
                enabled ? HEALTH_DOT[snapshot?.health ?? "warning"] : "bg-fg/20"
              }`}
            />
            {/* Siblings, not nested: the headline has to stay its own exact
                string, or nothing can assert on it. */}
            <div className="min-w-0">
              <p className="text-[12.5px] leading-snug text-fg/45">
                {enabled ? summary.headline : "Off"}
              </p>
              {enabled && summary.detail && (
                <p className="text-[12.5px] leading-snug text-fg/35">{summary.detail}</p>
              )}
            </div>
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Move label={`Move ${label} up`} glyph="↑" on={canMoveUp} onClick={() => onMove(-1)} />
          <Move label={`Move ${label} down`} glyph="↓" on={canMoveDown} onClick={() => onMove(1)} />
          <Switch checked={enabled} label={`Use ${label} for !c`} onChange={onToggle} />
        </div>
      </div>

      {usable ? (
        <div className="mt-2.5 flex flex-wrap items-center gap-2 ps-7">
          <Picker
            label={`Model for ${label}`}
            value={model}
            options={models}
            emptyLabel="Agent default"
            loadingLabel="Reading models…"
            onChange={(next) => {
              void api.setAskModel(agent, next);
              onSettings((s) => ({ ...s, models: { ...s.models, [agent]: next } }));
            }}
          />
          <Picker
            label={`Effort for ${label}`}
            value={effort}
            options={snapshot?.efforts ?? []}
            emptyLabel="Agent default"
            loadingLabel="Agent default"
            onChange={(next) => {
              void api.setAskEffort(agent, next);
              onSettings((s) => ({ ...s, efforts: { ...s.efforts, [agent]: next } }));
            }}
          />
        </div>
      ) : (
        enabled && (
          <p className="mt-2 ps-7 text-[12.5px] text-fg/35">Sign in to choose a model</p>
        )
      )}
    </div>
  );
}

/** One step up or down the preference order. Keyboard-reachable, unlike a drag. */
function Move({
  label,
  glyph,
  on,
  onClick,
}: {
  label: string;
  glyph: string;
  on: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      disabled={!on}
      onClick={onClick}
      className="size-6 shrink-0 rounded-md border border-white/10 text-[12px] text-fg/70 hover:text-fg disabled:border-white/5 disabled:text-fg/20"
    >
      {glyph}
    </button>
  );
}

/**
 * A locked-down choice: a list, never free text.
 *
 * `null` options means the list is still being read. An empty array means the
 * Agent would not say, and the only choice left is its own default.
 */
function Picker({
  label,
  value,
  options,
  emptyLabel,
  loadingLabel,
  onChange,
}: {
  label: string;
  value: string;
  options: readonly string[] | null;
  emptyLabel: string;
  loadingLabel: string;
  onChange: (value: string) => void;
}) {
  const loading = options === null;
  return (
    <select
      aria-label={label}
      value={value}
      disabled={loading}
      onChange={(e) => onChange(e.target.value)}
      className="h-8 w-44 max-w-full rounded-md border border-white/10 bg-black/20 px-2 text-[13px] text-fg outline-none disabled:text-fg/30"
    >
      <option value="">{loading ? loadingLabel : emptyLabel}</option>
      {(options ?? []).map((option) => (
        <option key={option} value={option}>
          {option}
        </option>
      ))}
    </select>
  );
}
