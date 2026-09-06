/**
 * Running one search from React.
 *
 * Two subscriptions, one state. Progress arrives on `onSearch` and the answer
 * on `onTurn`, and both are filtered by id: a cancelled search can still be
 * mid-stream, and rendering its tail into the next answer is the failure that
 * looks like a model going mad.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import type { SearchMessage, TurnMessage } from "@takyon/shared";

import { agentCancel, onSearch, onTurn, webCancel, webSearch } from "@/api";
import { IDLE, reduceSearch, reduceTurn, started, type SearchState } from "./searchState";

export type { SearchPhase, SearchState } from "./searchState";

/** Start searches and watch one at a time. Starting a second cancels the first. */
export function useSearch() {
  const [state, setState] = useState<SearchState>(IDLE);
  const active = useRef<number | null>(null);
  /** The Turn id, mirrored out of state so cleanup can read it synchronously. */
  const turn = useRef<number | null>(null);

  useEffect(() => {
    const offSearch = onSearch((message: SearchMessage) => {
      if (message.searchId !== active.current) return;
      setState((previous) => {
        const next = reduceSearch(previous, message);
        turn.current = next.turnId ?? null;
        return next;
      });
    });
    const offTurn = onTurn((message: TurnMessage) => {
      setState((previous) => reduceTurn(previous, message));
    });
    return () => {
      offSearch();
      offTurn();
    };
  }, []);

  // Only on unmount. A search whose window has gone has nobody to answer, and
  // its Turn is a process and a bill.
  useEffect(
    () => () => {
      if (active.current !== null) void webCancel(active.current);
      if (turn.current !== null) void agentCancel(turn.current);
    },
    [],
  );

  const search = useCallback(async (query: string) => {
    if (active.current !== null) void webCancel(active.current);
    if (turn.current !== null) void agentCancel(turn.current);
    active.current = null;
    turn.current = null;
    setState(started());
    try {
      active.current = await webSearch(query);
    } catch (e) {
      setState({
        ...started(),
        phase: "failed",
        error: e instanceof Error ? e.message : String(e),
      });
    }
  }, []);

  const cancel = useCallback(() => {
    if (active.current !== null) void webCancel(active.current);
    if (turn.current !== null) void agentCancel(turn.current);
    active.current = null;
    turn.current = null;
    setState(IDLE);
  }, []);

  return { state, search, cancel };
}
