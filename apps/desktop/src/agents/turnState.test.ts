import { describe, expect, it } from "bun:test";

import { reduce, stopped, type TurnState } from "./turnState";

const idle: TurnState = { phase: "idle", answer: "" };

describe("reduce", () => {
  it("v0.8 appends deltas rather than replacing the answer", () => {
    let state = reduce(idle, { turnId: 1, kind: "started", session: "s-1" });
    expect(state.phase).toBe("answering");
    expect(state.session).toBe("s-1");

    state = reduce(state, { turnId: 1, kind: "text", delta: "The sky " });
    state = reduce(state, { turnId: 1, kind: "text", delta: "is blue." });
    expect(state.answer).toBe("The sky is blue.");
  });

  // The session is what a follow-up resumes, and Claude reports it on the first
  // event only. Losing it on `done` would break Promotion.
  it("v0.8 keeps the session when done does not repeat it", () => {
    const answering = reduce(idle, { turnId: 1, kind: "started", session: "s-1" });
    const done = reduce(answering, { turnId: 1, kind: "done" });
    expect(done).toMatchObject({ phase: "done", session: "s-1" });
  });

  it("v0.8 prefers the session done carries when it has one", () => {
    const done = reduce(idle, { turnId: 1, kind: "done", session: "s-2" });
    expect(done.session).toBe("s-2");
  });

  // A Turn that fails halfway has still said something. Throwing it away hides
  // what went wrong.
  it("v0.8 keeps a partial answer when the turn fails", () => {
    let state = reduce(idle, { turnId: 1, kind: "text", delta: "Half an " });
    state = reduce(state, {
      turnId: 1,
      kind: "failed",
      reason: "agentError",
      agent: "Codex",
      message: "rate limited",
    });
    expect(state).toMatchObject({
      phase: "failed",
      answer: "Half an ",
      error: "rate limited",
    });
  });

  // v0.11.1: the reducer words a failure through `turnFailureCopy`, and keeps
  // the line under the headline apart so the view can set it smaller.
  it("v0.11.1 words a failure reason and keeps its detail", () => {
    const state = reduce(idle, {
      turnId: 1,
      kind: "failed",
      reason: "launcherBroken",
      agent: "opencode",
      binary: String.raw`C:\npm\opencode.cmd`,
      detail: "'node' is not recognized as an internal or external command,\noperable program",
    });
    expect(state.phase).toBe("failed");
    expect(state.error).toContain("Reinstall Node.js, then opencode.");
    expect(state.errorDetail).toBe("'node' is not recognized as an internal or external command,");
  });

  // ADR-0041: a fallback discards the failed Agent's session, so a follow-up
  // cannot resume it on the Agent that took over, and names who answers now.
  it("v0.11.2 a fallback drops the failed session and names the next Agent", () => {
    let state = reduce(idle, { turnId: 1, kind: "started", session: "claude-s" });
    state = reduce(state, {
      turnId: 1,
      kind: "fallback",
      to: "codex",
      label: "Codex",
      reason: "exited",
      agent: "Claude Code",
      code: 1,
      detail: "usage limit reached",
    });
    expect(state).toMatchObject({
      phase: "asking",
      answer: "",
      agent: "codex",
      agentLabel: "Codex",
      notice: "Claude Code couldn't answer, so Codex is.",
    });
    expect(state.session).toBeUndefined();

    state = reduce(state, { turnId: 1, kind: "started", session: "codex-s" });
    state = reduce(state, { turnId: 1, kind: "text", delta: "Answer." });
    state = reduce(state, { turnId: 1, kind: "done" });
    expect(state).toMatchObject({ phase: "done", session: "codex-s", agent: "codex" });
    expect(state.notice).toContain("Codex");
  });

  // Every Agent out of usage is the case the user hits at the end of a week:
  // the sentence says what to do, and the last Agent's own reason sits under it.
  it("v0.11.2 an exhausted chain says to sign in to an Agent with usage left", () => {
    const state = reduce(idle, {
      turnId: 1,
      kind: "failed",
      reason: "exhausted",
      tried: ["Claude Code", "Codex", "opencode"],
      last: { reason: "agentError", agent: "opencode", message: "Rate limit exceeded" },
    });
    expect(state.phase).toBe("failed");
    expect(state.error).toContain("No Agent could answer.");
    expect(state.error).toContain("Tried Claude Code, Codex, opencode.");
    expect(state.error).toContain("usage left");
    expect(state.errorDetail).toBe("Rate limit exceeded");
  });

  // Stop between a fallback and the first follow-up must not hand the
  // conversation back to the Agent that failed, nor drop the answering session.
  it("v0.11.2 Stop keeps the answering Agent and its session", () => {
    let state = reduce(idle, {
      turnId: 1,
      kind: "fallback",
      to: "codex",
      label: "Codex",
      reason: "notFound",
      agent: "Claude Code",
    });
    state = reduce(state, { turnId: 1, kind: "started", session: "codex-s" });
    state = reduce(state, { turnId: 1, kind: "text", delta: "Half" });
    expect(stopped(state)).toEqual({
      phase: "idle",
      answer: "",
      session: "codex-s",
      agent: "codex",
      agentLabel: "Codex",
    });
  });
});
