/**
 * Running one Turn from React.
 *
 * Events for every Turn arrive on one channel, so the first thing this does is
 * drop anything whose `turnId` is not the Turn it started — a cancelled Turn can
 * still be mid-stream, and rendering its tail into the next answer is the
 * failure that looks like a model going mad.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import type { AgentKind, TurnMessage } from "@takyon/shared";

import { agentAsk, agentCancel, onTurn } from "@/api";
import { IDLE, reduce, type TurnState } from "./turnState";

export type { TurnPhase, TurnState } from "./turnState";

/** Start Turns and watch one at a time. Starting a second cancels the first. */
export function useTurn() {
  const [state, setState] = useState<TurnState>(IDLE);
  const active = useRef<number | null>(null);

  useEffect(() => {
    return onTurn((message: TurnMessage) => {
      if (message.turnId !== active.current) return;
      setState((previous) => reduce(previous, message));
    });
  }, []);

  // Only on unmount. A Turn whose window has gone has nobody to answer.
  useEffect(
    () => () => {
      if (active.current !== null) void agentCancel(active.current);
    },
    [],
  );

  const ask = useCallback(
    async (args: { agent: AgentKind; prompt: string; session?: string; tools: boolean }) => {
      if (active.current !== null) void agentCancel(active.current);
      active.current = null;
      setState({ phase: "asking", answer: "", session: args.session });
      try {
        active.current = await agentAsk(args);
      } catch (e) {
        setState({
          phase: "failed",
          answer: "",
          error: e instanceof Error ? e.message : String(e),
        });
      }
    },
    [],
  );

  const cancel = useCallback(() => {
    if (active.current !== null) void agentCancel(active.current);
    active.current = null;
    setState(IDLE);
  }, []);

  return { state, ask, cancel };
}
