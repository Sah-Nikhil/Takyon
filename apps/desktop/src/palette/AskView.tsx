/**
 * The `!c` conversation, inside the Palette (v0.8 task 7).
 *
 * **One window.** A follow-up continues here rather than opening a second one:
 * Promotion (ADR-0001) is the Palette becoming a conversation, not a new
 * surface. Escape goes back one step, exactly as it does over `!v`.
 *
 * The tools rule survives the collapse by moving from the window to the Turn.
 * The first answer — the one you get by reflex, one keystroke from the global
 * hotkey — runs with tools off in the Scratch directory. Every follow-up after
 * it is an explicit act, and carries tools (ADR-0017).
 */

import { useEffect, useRef, useState } from "react";
import type { AgentKind, AgentSnapshot } from "@takyon/shared";

import { agentSummary, HEALTH_DOT } from "@/agents/status";
import { useTurn } from "@/agents/useTurn";

interface Message {
  role: "you" | "agent";
  text: string;
}

export function AskView({
  agent,
  question,
  snapshot,
  onClose,
}: {
  agent: AgentKind;
  question: string;
  snapshot: AgentSnapshot | undefined;
  onClose: () => void;
}) {
  const { state, ask, cancel } = useTurn();
  /** Everything already answered. The live Turn is rendered from `state`. */
  const [history, setHistory] = useState<Message[]>([]);
  /** The question being answered right now, above the streaming reply. */
  const [pending, setPending] = useState(question);
  const [session, setSession] = useState<string | undefined>();
  const [draft, setDraft] = useState("");
  const transcriptRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const busy = state.phase === "asking" || state.phase === "answering";
  const label = snapshot?.label ?? agent;

  /*
    Once, for the question that opened this view. Guarded by a ref rather than by
    an empty dependency list alone: StrictMode runs mount effects twice in `bun
    run dev`, and a Turn is a process and a bill.
   */
  const asked = useRef(false);
  useEffect(() => {
    if (asked.current) return;
    asked.current = true;
    // Tools off: this is the reflex Turn (ADR-0017).
    void ask({ agent, prompt: question, tools: false });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Follows the stream. Without it the answer grows below the fold and the
  // window looks like it stopped after the first sentence.
  useEffect(() => {
    const el = transcriptRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [history, pending, state.answer]);

  const send = () => {
    const text = draft.trim();
    if (!text || busy) return;
    setDraft("");
    // The finished Turn joins the transcript here rather than in an effect: it
    // is the only moment it is certainly complete, and one copy is enough.
    const answer = state.answer || state.error;
    setHistory((all) => [
      ...all,
      { role: "you" as const, text: pending },
      ...(answer ? [{ role: "agent" as const, text: answer }] : []),
    ]);
    setPending(text);
    const resume = state.session ?? session;
    setSession(resume);
    // Tools on: a follow-up is an explicit act, not a reflex.
    void ask({ agent, prompt: text, session: resume, tools: true });
  };

  return (
    <div
      className="flex h-full w-full flex-col overflow-hidden rounded-xl border border-white/10 bg-plate/95 shadow-2xl backdrop-blur-xl"
      onKeyDown={(e) => {
        if (e.key !== "Escape") return;
        // Stops here rather than dismissing the window: Escape means back one
        // step, exactly as it does over an open surface or menu.
        e.stopPropagation();
        e.preventDefault();
        onClose();
      }}
    >
      <header className="flex items-center gap-2.5 border-b border-white/5 px-4 py-3">
        <span
          aria-hidden
          className={`size-2 shrink-0 rounded-full ${HEALTH_DOT[snapshot?.health ?? "warning"]}`}
        />
        <span className="shrink-0 text-[13px] text-fg/70">{label}</span>
        <span className="ms-auto shrink-0 text-[12px] text-fg/40" role="status">
          {state.phase === "asking"
            ? "Thinking…"
            : state.phase === "answering"
              ? "Answering…"
              : state.phase === "failed"
                ? "Stopped"
                : agentSummary(snapshot).headline}
        </span>
      </header>

      <div ref={transcriptRef} className="flex-1 space-y-4 overflow-y-auto px-4 py-3">
        {history.map((message, i) => (
          <Bubble key={i} role={message.role} text={message.text} />
        ))}
        <Bubble role="you" text={pending} />
        <Bubble
          role="agent"
          text={state.answer || (state.phase === "failed" ? "" : `Asking ${label}…`)}
        />
        {state.phase === "failed" && (
          <p className="text-[13px] text-amber-300" role="alert">
            {state.error ?? `${label} stopped without answering.`}
          </p>
        )}
      </div>

      <div className="flex items-center gap-3 border-t border-white/5 px-4 py-2.5">
        <input
          ref={inputRef}
          autoFocus
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              send();
            }
          }}
          placeholder={busy ? "Waiting for an answer…" : "Ask a follow-up"}
          className="h-8 w-full bg-transparent text-[13px] text-fg outline-none placeholder:text-fg/35"
        />
        {busy ? (
          <button
            type="button"
            onClick={cancel}
            className="shrink-0 rounded-md border border-white/10 px-2.5 py-1 text-[12px] text-fg/70 hover:text-fg"
          >
            Stop
          </button>
        ) : (
          <span className="shrink-0 text-[12px] text-fg/35">Esc to go back</span>
        )}
      </div>
    </div>
  );
}

function Bubble({ role, text }: Message) {
  if (!text) return null;
  const you = role === "you";
  return (
    <div className={you ? "flex justify-end" : "flex justify-start"}>
      <div
        className={
          you
            ? "max-w-[80%] whitespace-pre-wrap rounded-xl bg-white/10 px-3 py-1.5 text-[13px] leading-relaxed"
            : "max-w-[85%] whitespace-pre-wrap text-[13.5px] leading-relaxed text-fg/90"
        }
      >
        {text}
      </div>
    </div>
  );
}
