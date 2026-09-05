/**
 * One Turn's state, and how an event folds into it.
 *
 * Split from `useTurn.ts` so it can be tested without pulling in `api.ts` and,
 * through it, Tauri. The hook is the wiring; this is the rule.
 */

import type { TurnMessage } from "@takyon/shared";

export type TurnPhase = "idle" | "asking" | "answering" | "done" | "failed";

export interface TurnState {
  phase: TurnPhase;
  /** The answer so far. Deltas appended in arrival order, never replaced. */
  answer: string;
  /** The Agent's session, once it has one. What a follow-up resumes. */
  session?: string;
  error?: string;
}

export const IDLE: TurnState = { phase: "idle", answer: "" };

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
      return { ...previous, phase: "failed", error: message.message };
  }
}
