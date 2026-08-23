---
status: accepted
---

# Launcher first, with lazy Promotion to a Chat Surface

Takyon is two things at once: a stateless launcher that must appear and
disappear in tens of milliseconds, and a surface for AI answers that stream in
over seconds and invite follow-ups. We decided the Palette stays religiously
stateless — it dies on Escape and remembers nothing — and that a conversation gets
its own separate window (the Chat Surface) only on Promotion, which happens when
the user asks a follow-up. A single question answered inline never creates one.

## Considered Options

- **Strict launcher**: AI output renders into a temporary panel that dies on
  Escape. Rejected: follow-up questions are common enough that losing the answer
  every time is hostile.
- **Always promote**: every AI query opens a chat window. Rejected: the common
  case is one question and one answer, and paying for a window each time makes the
  tool feel heavy — the exact failure mode we are building against.
- **Chat app with a launcher attached**: rejected outright; it inverts the
  priority and contaminates the hot path with session state.

## Consequences

Session state lives entirely in the Chat Surface, never in the Palette. Anything
that would require the Palette to remember something across openings should be
treated as a design smell and questioned.
