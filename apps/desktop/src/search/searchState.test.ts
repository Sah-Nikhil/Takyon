import { describe, expect, it } from "bun:test";

import { IDLE, reduceSearch, reduceTurn, started, type SearchState } from "./searchState";

const hit = (n: number) => ({
  title: `Source ${n}`,
  url: `https://example.com/${n}`,
  description: `Snippet ${n}.`,
});

/** A search that has reached its Turn, which is where answer text starts. */
function answering(turnId = 7): SearchState {
  const state = reduceSearch(started(), {
    searchId: 1,
    kind: "reading",
    sources: [hit(1), hit(2)],
  });
  return reduceSearch(state, {
    searchId: 1,
    kind: "answering",
    turnId,
    agent: "Claude Code",
  });
}

describe("reduceSearch", () => {
  it("v0.9 carries the phases in the order they happen", () => {
    let state = started();
    expect(state.phase).toBe("searching");

    state = reduceSearch(state, { searchId: 1, kind: "reading", sources: [hit(1)] });
    expect(state.phase).toBe("reading");
    expect(state.sources).toHaveLength(1);

    state = reduceSearch(state, {
      searchId: 1,
      kind: "answering",
      turnId: 4,
      agent: "Codex",
    });
    expect(state.phase).toBe("answering");
    expect(state.agent).toBe("Codex");
  });

  // The sources are on screen while the answer is still being written, so a
  // later phase must not clear them.
  it("v0.9 keeps the sources once the answer starts", () => {
    const state = answering();
    expect(state.sources.map((s) => s.url)).toEqual([
      "https://example.com/1",
      "https://example.com/2",
    ]);
  });

  it("v0.9 carries a failure message as written", () => {
    const state = reduceSearch(started(), {
      searchId: 1,
      kind: "failed",
      message: "No Exa key. Add one in Settings → Web search.",
    });
    expect(state.phase).toBe("failed");
    expect(state.error).toContain("Settings");
  });
});

describe("reduceTurn", () => {
  it("v0.9 appends answer deltas rather than replacing them", () => {
    let state = answering();
    state = reduceTurn(state, { turnId: 7, kind: "text", delta: "Ferrari " });
    state = reduceTurn(state, { turnId: 7, kind: "text", delta: "since 1950 [1]." });
    expect(state.answer).toBe("Ferrari since 1950 [1].");
    state = reduceTurn(state, { turnId: 7, kind: "done" });
    expect(state.phase).toBe("done");
  });

  // Every Turn shares one channel. Another Turn's tail rendered into this
  // answer is the failure that looks like a model going mad.
  it("v0.9 ignores events belonging to another Turn", () => {
    const state = reduceTurn(answering(7), {
      turnId: 99,
      kind: "text",
      delta: "not this search",
    });
    expect(state.answer).toBe("");
  });

  // Before a Turn exists there is no id to match, so nothing may be appended —
  // otherwise a Turn from a previous `!c` writes into a fresh search.
  it("v0.9 ignores Turn events before a Turn has been named", () => {
    expect(reduceTurn(IDLE, { turnId: 1, kind: "text", delta: "x" }).answer).toBe("");
  });

  it("v0.9 shows the Agent's own failure rather than a generic one", () => {
    const state = reduceTurn(answering(), {
      turnId: 7,
      kind: "failed",
      message: "Claude Code ended without answering.",
    });
    expect(state.phase).toBe("failed");
    expect(state.error).toBe("Claude Code ended without answering.");
  });
});

/**
 * ADR-0021's cost. A search that falls back from the keyed provider to the
 * keyless one sends `searching` twice, and the second names a different service.
 * The header has to follow it, or it tells the user the question went somewhere
 * it did not.
 */
it("a second searching event corrects the provider", () => {
  let state = reduceSearch(started(), { searchId: 1, kind: "searching", provider: "Exa" });
  expect(state.provider).toBe("Exa");

  state = reduceSearch(state, { searchId: 1, kind: "searching", provider: "DuckDuckGo" });
  expect(state.provider).toBe("DuckDuckGo");
  expect(state.phase).toBe("searching");
});

/** Falling back mid-search must not throw away hits already on screen. */
it("a fallback keeps sources already shown", () => {
  const sources = [{ title: "t", url: "https://e.x/a", description: "d" }];
  let state = reduceSearch(started(), { searchId: 1, kind: "reading", sources });
  state = reduceSearch(state, { searchId: 1, kind: "searching", provider: "DuckDuckGo" });
  expect(state.sources).toEqual(sources);
});
