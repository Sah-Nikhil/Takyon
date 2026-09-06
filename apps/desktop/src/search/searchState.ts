/**
 * One search, reduced from two event channels (v0.9 task 5).
 *
 * Progress arrives on `EVENT_SEARCH` and the answer on `EVENT_TURN`, so this
 * folds both into one state. The phases exist because each says something
 * different and true: `searching` is the moment data left the machine, and the
 * Palette says so in colour (task 7).
 */

import type { SearchHit, SearchMessage, TurnMessage } from "@takyon/shared";

export type SearchPhase =
  | "idle"
  | "searching"
  | "reading"
  | "answering"
  | "done"
  | "failed";

export interface SearchState {
  phase: SearchPhase;
  /** Hits, as soon as they are known. On screen while the answer is written. */
  sources: SearchHit[];
  /** The synthesised answer so far. Deltas, appended in arrival order. */
  answer: string;
  /** Which Agent is writing it, once one has been picked. */
  agent?: string;
  /**
   * The service the query is at. Corrected mid-search when `!s` falls back from
   * the keyed provider to the keyless one (ADR-0021), so the outbound header
   * never names a service that did not answer.
   */
  provider?: string;
  error?: string;
  /** The Turn carrying the answer, so it can be cancelled. */
  turnId?: number;
}

export const IDLE: SearchState = { phase: "idle", sources: [], answer: "" };

/** The state a fresh search starts in. */
export const started = (): SearchState => ({
  phase: "searching",
  sources: [],
  answer: "",
});

/** Fold one progress event in. */
export function reduceSearch(state: SearchState, message: SearchMessage): SearchState {
  switch (message.kind) {
    case "searching":
      // Sources are kept: a fallback repaints the header, it does not restart
      // the search from the Palette's point of view.
      return { ...state, phase: "searching", provider: message.provider };
    case "reading":
      return { ...state, phase: "reading", sources: message.sources };
    case "answering":
      return {
        ...state,
        phase: "answering",
        agent: message.agent,
        turnId: message.turnId,
      };
    case "failed":
      return { ...state, phase: "failed", error: message.message };
  }
}

/**
 * Fold one Turn event in.
 *
 * Events for every Turn share one channel, so anything that is not this
 * search's Turn is dropped — a cancelled search still mid-stream would
 * otherwise write its tail into the next answer.
 */
export function reduceTurn(state: SearchState, message: TurnMessage): SearchState {
  if (state.turnId !== message.turnId) return state;
  switch (message.kind) {
    case "started":
      return state;
    case "text":
      return { ...state, phase: "answering", answer: state.answer + message.delta };
    case "done":
      return { ...state, phase: "done" };
    case "failed":
      return { ...state, phase: "failed", error: message.message };
  }
}
