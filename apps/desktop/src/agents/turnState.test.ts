import { describe, expect, it } from "bun:test";

import { reduce, type TurnState } from "./turnState";

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
    state = reduce(state, { turnId: 1, kind: "failed", message: "rate limited" });
    expect(state).toMatchObject({
      phase: "failed",
      answer: "Half an ",
      error: "rate limited",
    });
  });
});
