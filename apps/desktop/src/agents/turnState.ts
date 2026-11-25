/**
 * One Turn's state, and how an event folds into it.
 *
 * Split from `useTurn.ts` so it can be tested without pulling in `api.ts` and,
 * through it, Tauri. The hook is the wiring; this is the rule.
 */

import type { AgentKind, TurnFailure, TurnMessage } from "@takyon/shared";

import { fallbackNotice, turnFailureCopy } from "./status";

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
  /** The Agent answering, once a fallback moved the Turn off the asked one. */
  agent?: AgentKind;
  /** That Agent's display label. */
  agentLabel?: string;
  /** Who handed over to whom, worded. Kept once set: it explains the answer. */
  notice?: string;
}

export const IDLE: TurnState = { phase: "idle", answer: "" };

/**
 * The state after Stop. The Turn ends; the conversation does not: the Agent
 * that answered and its session stay, or the next follow-up asks the one that
 * just failed (ADR-0041).
 */
export function stopped({ session, agent, agentLabel }: TurnState): TurnState {
  return { ...IDLE, session, agent, agentLabel };
}

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
    case "fallback":
      // Nothing was written, so there is no answer to keep. The session is
      // dropped: it belongs to the Agent that failed, and resuming it elsewhere fails.
      return {
        ...previous,
        phase: "asking",
        answer: "",
        session: undefined,
        agent: message.to,
        agentLabel: message.label,
        notice: fallbackNotice(message.from, message.to),
      };
  }
}
