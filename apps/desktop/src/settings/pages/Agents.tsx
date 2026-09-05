/**
 * Agents: which one `!c` reaches, where a Turn runs, and one card per Agent.
 *
 * **The model and effort are locked here and nowhere else.** Whatever a card
 * says is what every Turn uses; there is no per-query override, and `agent_ask`
 * reads the pair from `settings.db` rather than taking it from the frontend.
 *
 * **There is no Sign in button, deliberately.** Takyon reads an Agent's Sign-in
 * state and never changes it (ADR-0017); the card carries the command to run.
 * TBC-0012 is the amendment if that turns out not to be enough.
 */

import { useCallback, useEffect, useState } from "react";
import type { AgentKind, AgentSettings, AgentSnapshot } from "@takyon/shared";

import * as api from "@/api";
import { agentSummary, canAsk, HEALTH_DOT, versionLabel } from "@/agents/status";
import { Chips, Group, Row, useApplied } from "../controls";

const AGENT_ORDER: ReadonlyArray<{ value: AgentKind; label: string }> = [
  { value: "claude", label: "Claude Code" },
  { value: "codex", label: "Codex" },
  { value: "opencode", label: "opencode" },
];

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

  // Fetched here rather than through `probe`, which resets the cards to their
  // "Checking…" state synchronously — on mount they are already in it.
  useEffect(() => {
    void api.agentSettings().then((next) => {
      setSettings(next);
      setCwd(next.cwd);
    });
    void api.agentSnapshots().then(setSnapshots);
  }, []);

  const agentApplied = useApplied(api.setAskAgent, async () => (await api.agentSettings()).default);
  const cwdApplied = useApplied(api.setAskCwd, async () => (await api.agentSettings()).cwd);

  return (
    <>
      <Group>
        <Row
          id="ask-agent"
          label="Ask !c with"
          applied={agentApplied.applied}
          error={agentApplied.error}
          description="One Bang for every agent. Whichever is chosen here answers !c; the others stay available in their own CLIs."
        >
          <Chips
            label="Ask !c with"
            value={settings?.default ?? "claude"}
            options={AGENT_ORDER}
            onChange={(next) =>
              void agentApplied.apply(next, (value) =>
                setSettings((s) => (s ? { ...s, default: value } : s)),
              )
            }
          />
        </Row>
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

      <Group title="Installed agents">
        <div className="flex items-center justify-between px-3.5 py-2">
          <p className="text-[12.5px] text-fg/45">
            Takyon runs the agent you already installed and signed in to. It never holds an
            account or a key of its own. The model and effort you pick here are the only ones
            it will use.
          </p>
          <button
            type="button"
            onClick={probe}
            className="ms-4 shrink-0 rounded-md border border-white/10 px-2.5 py-1 text-[12px] text-fg/70 hover:text-fg"
          >
            {snapshots === null ? "Checking…" : "Check again"}
          </button>
        </div>
        {AGENT_ORDER.map(({ value, label }) => (
          <AgentCard
            key={value}
            agent={value}
            fallbackLabel={label}
            snapshot={snapshots?.find((s) => s.kind === value)}
            model={settings?.models[value] ?? ""}
            effort={settings?.efforts[value] ?? ""}
            onSettings={(next) => setSettings((s) => (s ? next(s) : s))}
          />
        ))}
      </Group>
    </>
  );
}

function AgentCard({
  agent,
  fallbackLabel,
  snapshot,
  model,
  effort,
  onSettings,
}: {
  agent: AgentKind;
  fallbackLabel: string;
  snapshot: AgentSnapshot | undefined;
  model: string;
  effort: string;
  onSettings: (next: (s: AgentSettings) => AgentSettings) => void;
}) {
  const [models, setModels] = useState<string[] | null>(null);
  const summary = agentSummary(snapshot);
  const version = versionLabel(snapshot?.version);
  // Only an Agent that can answer gets pickers. A model list for a CLI that is
  // not installed is a control that cannot mean anything.
  const usable = canAsk(snapshot);

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
    <Row
      id={`agent-${agent}`}
      label={snapshot?.label ?? fallbackLabel}
      description={
        <span className="flex flex-col gap-0.5">
          <span className="flex items-center gap-2">
            <span
              aria-hidden
              className={`size-2 shrink-0 rounded-full ${HEALTH_DOT[snapshot?.health ?? "warning"]}`}
            />
            <span>{summary.headline}</span>
            {version && <span className="text-fg/30">{version}</span>}
          </span>
          {summary.detail && <span>{summary.detail}</span>}
        </span>
      }
    >
      {usable ? (
        <>
          <Picker
            label={`Model for ${snapshot?.label ?? fallbackLabel}`}
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
            label={`Effort for ${snapshot?.label ?? fallbackLabel}`}
            value={effort}
            options={snapshot?.efforts ?? []}
            emptyLabel="Agent default"
            loadingLabel="Agent default"
            onChange={(next) => {
              void api.setAskEffort(agent, next);
              onSettings((s) => ({ ...s, efforts: { ...s.efforts, [agent]: next } }));
            }}
          />
        </>
      ) : (
        <span className="text-[12.5px] text-fg/35">Sign in to choose a model</span>
      )}
    </Row>
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
      className="h-8 w-52 max-w-full rounded-md border border-white/10 bg-black/20 px-2 text-[13px] text-fg outline-none disabled:text-fg/30"
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
