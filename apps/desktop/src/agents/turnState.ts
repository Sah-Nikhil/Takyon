/**
 * One Turn's state, and how an event folds into it.
 *
 * Split from `useTurn.ts` so it can be tested without pulling in `api.ts` and,
 * through it, Tauri. The hook is the wiring; this is the rule.
 */

import type { TurnFailure, TurnMessage } from "@takyon/shared";

import { turnFailureCopy } from "./status";

export type TurnPhase = "idle" | "asking" | "answering" | "done" | "failed";

export interface TurnState {
  phase: TurnPhase;
  /** The answer so far. Deltas appended in arrival order, never replaced. */
  answer: string;
  /** The Agent's session, once it has one. What a follow-up resumes. */
  session?: string;
  /** A failure's headline, worded by `turnFailureCopy`. */
  error?: string;
  /** The line under it: stderr's tail, the missing command, or nothing. */
  errorDetail?: string;
}

export const IDLE: TurnState = { phase: "idle", answer: "" };

/** `error` and `errorDetail` for a failed Turn. Shared with `searchState.ts`. */
export function failureFields(failure: TurnFailure) {
  const copy = turnFailureCopy(failure);
  return { error: copy.headline, errorDetail: copy.detail ?? undefined };
}

export function reduce(previous: TurnState, message: TurnMessage): TurnState {
  switch (message.kind) {
    case "started":
      return { ...previous, phase: "answering", session: message.session };
    case "text":
      return { ...previous, phase: "answering", answer: previous.answer + message.delta };
    case "done":
      // The session is kept when `done` does not repeat it: Claude reports it on
      // the first event only, and losing it here would break Promotion.
      return { ...previous, phase: "done", session: message.session ?? previous.session };
    case "failed":
      // The answer so far is kept too. A Turn that failed halfway has still said
      // something, and throwing it away hides what went wrong.
      return { ...previous, phase: "failed", ...failureFields(message) };
  }
}
