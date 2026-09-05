import { describe, expect, it } from "bun:test";
import type { AgentSnapshot } from "@takyon/shared";

import { agentSummary, blockedReason, canAsk, pickAgent, versionLabel } from "./status";

const base: AgentSnapshot = {
  kind: "claude",
  label: "Claude Code",
  binary: "claude",
  installed: true,
  health: "ready",
  signIn: { status: "in" },
  efforts: ["low", "medium", "high"],
};

describe("agentSummary", () => {
  it("v0.8 reads the account label into the headline", () => {
    expect(
      agentSummary({
        ...base,
        signIn: { status: "in", label: "Claude Pro Subscription", account: "you@example.com" },
      }),
    ).toEqual({
      headline: "Authenticated · Claude Pro Subscription",
      detail: "you@example.com",
    });
  });

  /// Not installed beats sign-in state: there is nothing to be signed in to.
  it("v0.8 prefers not-found over every other state", () => {
    const summary = agentSummary({
      ...base,
      installed: false,
      health: "error",
      signIn: { status: "in", label: "Claude Pro Subscription" },
      message: "Claude Code (`claude`) was not found on PATH.",
    });
    expect(summary.headline).toBe("Not found");
    expect(summary.detail).toContain("`claude`");
  });

  it("v0.8 gives a signed-out agent the agent's own sentence", () => {
    expect(
      agentSummary({
        ...base,
        health: "error",
        signIn: { status: "out" },
        message: "Codex CLI is not authenticated. Run `codex login` and try again.",
      }),
    ).toEqual({
      headline: "Not authenticated",
      detail: "Codex CLI is not authenticated. Run `codex login` and try again.",
    });
  });

  /// Installed but silent is "Needs attention", not "Not authenticated" — the
  /// difference is the whole point of the third sign-in state.
  it("v0.8 separates an unverified agent from a signed-out one", () => {
    expect(
      agentSummary({ ...base, health: "warning", signIn: { status: "unknown" } }).headline,
    ).toBe("Needs attention");
  });

  it("v0.8 says something before the first probe returns", () => {
    expect(agentSummary(undefined).headline).toBe("Checking agent status");
  });
});

describe("canAsk", () => {
  it("v0.8 lets an unverified agent try rather than refusing", () => {
    expect(canAsk({ ...base, health: "warning", signIn: { status: "unknown" } })).toBe(true);
  });

  it("v0.8 refuses a missing or signed-out agent", () => {
    expect(canAsk({ ...base, installed: false })).toBe(false);
    expect(canAsk({ ...base, signIn: { status: "out" } })).toBe(false);
    expect(canAsk(undefined)).toBe(false);
  });
});

describe("blockedReason", () => {
  it("v0.8 is null when the agent can answer", () => {
    expect(blockedReason(base)).toBeNull();
  });

  /// Takyon never runs the login itself (ADR-0017), so the line ends at what to
  /// type. Falls back to T3 Code's shared sentence when the agent said nothing.
  it("v0.8 falls back to the shared sign-in sentence", () => {
    expect(blockedReason({ ...base, signIn: { status: "out" } })).toBe(
      "Sign in via the CLI to authenticate again.",
    );
  });

  /// Unprobed is not blocked: `!c` asks and the Agent's own error is the answer.
  /// A row that swallowed Enter for three process spawns read as broken.
  it("v0.8 does not block on an agent it has not probed yet", () => {
    expect(blockedReason(undefined)).toBeNull();
  });

  it("v0.8 names the missing command", () => {
    expect(blockedReason({ ...base, installed: false })).toBe(
      "Claude Code (`claude`) was not found on PATH.",
    );
  });
});

describe("pickAgent", () => {
  const snapshots: AgentSnapshot[] = [
    { ...base, kind: "claude" },
    { ...base, kind: "codex", label: "Codex", binary: "codex", installed: false },
    { ...base, kind: "opencode", label: "opencode", binary: "opencode", signIn: { status: "out" } },
  ];

  it("v0.8 takes the first preference that can answer", () => {
    expect(pickAgent(["opencode", "codex", "claude"], snapshots)).toBe("claude");
    expect(pickAgent(["claude", "codex", "opencode"], snapshots)).toBe("claude");
  });

  /// The row has to name someone, and the first preference is the honest guess.
  it("v0.8 falls back to first preference while probing and when none can", () => {
    expect(pickAgent(["codex", "claude"], null)).toBe("codex");
    const allOut = snapshots.map((s) => ({ ...s, signIn: { status: "out" as const } }));
    expect(pickAgent(["opencode", "claude", "codex"], allOut)).toBe("opencode");
  });

  /// Nothing switched on is the one state with no Agent to name.
  it("v0.8 has nothing to pick when every agent is switched off", () => {
    expect(pickAgent([], snapshots)).toBeNull();
    expect(pickAgent([], null)).toBeNull();
  });
});

describe("versionLabel", () => {
  it("v0.8 prefixes a bare semver and leaves a tag alone", () => {
    expect(versionLabel("2.1.261")).toBe("v2.1.261");
    expect(versionLabel("nightly-2026-09-01")).toBe("nightly-2026-09-01");
    expect(versionLabel(undefined)).toBeNull();
  });
});
