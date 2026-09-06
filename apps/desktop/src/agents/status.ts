/**
 * The words an Agent's state is shown in — T3 Code's `providerStatus.ts` and
 * `ProviderStatusBanner.tsx`, ported.
 *
 * Copy lives here rather than in Rust because Rust ships facts (ADR-0009,
 * "nothing UI-aware in Rust"), and it lives in one file rather than in each
 * surface because the Settings card and the Palette must say the same thing
 * about the same Agent.
 */

import type { AgentHealth, AgentKind, AgentSnapshot } from "@takyon/shared";

/**
 * The dot beside an Agent's name. T3 Code's treatment, Takyon's palette.
 *
 * Green for ready, against amber and red: the three read as a traffic light and
 * the accent could not, being the same colour as the selected row. The one place
 * in the build that uses green, and only for "signed in and answering".
 */
export const HEALTH_DOT: Record<AgentHealth, string> = {
  ready: "bg-emerald-400",
  warning: "bg-warning",
  error: "bg-red-400",
};

/**
 * The name an Agent goes by before its own probe has answered.
 *
 * Needed because the Palette names its Agent on the first keystroke now, and the
 * label a snapshot carries is not there yet.
 */
export const AGENT_LABELS: Record<AgentKind, string> = {
  claude: "Claude Code",
  codex: "Codex",
  opencode: "opencode",
};

export interface AgentSummary {
  headline: string;
  detail: string | null;
}

/**
 * The headline and detail under an Agent's name.
 *
 * Order matters and is T3 Code's: not installed beats Sign-in state, which beats
 * health. An Agent that is missing has nothing to be signed in to.
 */
export function agentSummary(snapshot: AgentSnapshot | undefined): AgentSummary {
  if (!snapshot) {
    return {
      headline: "Checking agent status",
      detail: "Waiting for the agent to report its version and sign-in state.",
    };
  }
  if (!snapshot.installed) {
    return {
      headline: "Not found",
      detail: snapshot.message ?? "CLI not detected on PATH.",
    };
  }
  if (snapshot.signIn.status === "in") {
    const label = snapshot.signIn.label;
    return {
      headline: label ? `Authenticated · ${label}` : "Authenticated",
      detail: snapshot.message ?? snapshot.signIn.account ?? null,
    };
  }
  if (snapshot.signIn.status === "out") {
    return { headline: "Not authenticated", detail: snapshot.message ?? null };
  }
  if (snapshot.health === "warning") {
    return {
      headline: "Needs attention",
      detail:
        snapshot.message ?? "The agent is installed, but Takyon could not fully verify it.",
    };
  }
  if (snapshot.health === "error") {
    return {
      headline: "Unavailable",
      detail: snapshot.message ?? "The agent failed its startup checks.",
    };
  }
  return {
    headline: "Available",
    detail: snapshot.message ?? "Installed and ready, but sign-in could not be verified.",
  };
}

/**
 * Whether `!c` can actually reach this Agent right now.
 *
 * `unknown` counts as usable, deliberately: T3 Code lets an unverified provider
 * run, and refusing to try because a probe was quiet would be worse than a
 * failed Turn that says why.
 */
export function canAsk(snapshot: AgentSnapshot | undefined): boolean {
  if (!snapshot) return false;
  return snapshot.installed && snapshot.signIn.status !== "out";
}

/**
 * Which Agent `!c` reaches: the first in the preference order that can answer.
 *
 * Refinement, not a gate: the first switched-on Agent answers, and once the
 * probe lands a signed-out one is stepped over. **Never waits** — an unprobed
 * `!c` asks anyway and lets the Agent's own error speak (ADR-0018).
 */
export function pickAgent(
  order: readonly AgentKind[],
  snapshots: AgentSnapshot[] | null,
): AgentKind | null {
  const first = order[0] ?? null;
  if (!snapshots || !first) return first;
  return order.find((kind) => canAsk(snapshots.find((s) => s.kind === kind))) ?? first;
}

/**
 * The one line the Palette shows instead of asking. T3 Code's banner copy.
 *
 * Takyon never runs an Agent's login (ADR-0017), so this ends at what to type.
 */
export function blockedReason(snapshot: AgentSnapshot | undefined): string | null {
  // Unprobed is not blocked. `!c` asks and lets the Turn fail with the Agent's
  // own sentence, which is more use than a row that refuses Enter in silence.
  if (!snapshot) return null;
  if (!snapshot.installed) {
    return snapshot.message ?? `${snapshot.label} (\`${snapshot.binary}\`) was not found on PATH.`;
  }
  if (snapshot.signIn.status === "out") {
    return snapshot.message ?? "Sign in via the CLI to authenticate again.";
  }
  return null;
}

/** The version as a card shows it. A bare semver gets a `v`, a tag does not. */
export function versionLabel(version: string | undefined): string | null {
  if (!version) return null;
  return /^\d/.test(version) ? `v${version}` : version;
}
