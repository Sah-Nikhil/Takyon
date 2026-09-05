---
status: accepted
---

# Agents are driven, never authenticated

Takyon runs Claude Code, Codex and opencode as subprocesses. It never holds an
account, an API key or an OAuth token for any of them, and it never writes their
credential files. Signing in happens in the Agent's own CLI. Takyon's only job is
to read the Sign-in state back and say something useful about it.

This was already settled for Claude Code — "the user's own `claude` CLI as a
subprocess, Takyon never holds an LLM account or key of its own" has been in
`CLAUDE.md` since v0.1. This ADR generalises it to every Agent and records why
the alternative was rejected after we went and looked at what a comparable
product actually does.

## What T3 Code does

T3 Code (`pingdotgg/t3code`, MIT) ships six Agent drivers. Five of them —
Claude, Codex, Cursor, Grok, opencode — have **no sign-in flow at all**. Only
Antigravity, where Google's OAuth is the only way in and there is no CLI to
delegate to, carries a real `ProviderAuthController`: 539 lines of flow state,
callback handling, a paste-the-redirect-URL fallback, and a logout that has to
stop every running session first.

For the other five, T3 Code probes and explains:

| Agent | How it reads Sign-in state | What it says when signed out |
|---|---|---|
| Claude | capabilities probe → subscription type, account email | `Claude Agent CLI (\`claude\`) was not found on PATH.` |
| Codex | `codex app-server` → account record | ``Codex CLI is not authenticated. Run `codex login` and try again.`` |
| opencode | `opencode serve` → connected provider count | falls through to the shared copy |

with one shared fallback line — **`Sign in via the CLI to authenticate again.`** —
and a status headline built from three facts: installed, enabled, Sign-in state
(`providerStatus.ts`). That is the whole surface.

We are copying that, deliberately and in full.

## Why not our own OAuth

**The token would become ours.** An in-app PKCE flow ends with Takyon holding a
refresh token it must store, encrypt, rotate and eventually leak. Every argument
in ADR-0006 and ADR-0008 about clipboard secrets applies again, for a credential
that is worth more.

**The file format is not a contract.** Writing `~/.claude.json`, `~/.codex/` or
opencode's auth store means depending on an undocumented layout that each CLI is
free to change in a patch release. The failure mode is a Takyon update looking
fine and an Agent that silently cannot authenticate.

**It buys nothing the CLI does not already do.** All three ship a working
browser-based login. Reimplementing it is work whose best possible outcome is
parity.

## Why not spawn the login command ourselves

Tempting, and not refused on principle — see `docs/tbc/0012`. It is out of v0.9
because a launcher has no console attached, so `claude /login` has nowhere to
draw its prompt, and getting a real terminal in front of the user
(`wt.exe` / `conhost.exe`, neither guaranteed present) is its own small project
with its own failure modes. v0.9 ships the T3 Code surface exactly; the terminal
path is the amendment, not the design.

## The rule

**Takyon may read an Agent's Sign-in state. It may never change it.**

Concretely, in `agents/`:

- No driver writes to any path an Agent owns.
- No driver reads a credential file. Sign-in state comes from asking the Agent —
  `claude`'s stream-json `init` event, `codex app-server`'s account record,
  `opencode`'s connected-provider list — never from parsing a token store.
- `ANTHROPIC_API_KEY` and the like are passed through from the environment
  untouched and never read, logged or stored.
- A signed-out Agent produces a Sign-in state and a sentence, never an error
  dialog and never a retry loop.

## The working directory is part of this

An Agent with tools, running in a directory the user did not choose, is a worse
outcome than a signed-out one. So every Turn runs in the Scratch directory
(`%LOCALAPPDATA%\v3sper\launcher\scratch\`) unless the user named a directory in
Settings. The default is empty by construction, so the worst case for a
mis-scoped Agent is that it finds nothing.

**Tools belong to the Turn, not to a window.** The first answer — the one you get
by reflex, one keystroke from the global hotkey — runs with tools off. Every
follow-up after it has them, because asking again is a deliberate act. There is
no second window to draw that line any more (v0.9 keeps the whole conversation in
the Palette), so the line is drawn where it always belonged.

## The model and the effort are locked, and Takyon does not choose them

Settings holds one model and one effort level per Agent, picked from what that
Agent itself reports — `claude`'s documented `--model` aliases, `codex debug
models`, `opencode models`. **That pair is the only one a Turn can use.** There is
no per-query override, no Bang syntax for it, and `agent_ask` reads both from
`settings.db` rather than accepting them from the frontend, so the webview cannot
send a model even if some future surface tried to.

Two reasons it is a lock rather than a default. A launcher that silently upgrades
you to a more expensive model is spending someone else's money, and an answer
whose quality changes without the question changing is impossible to reason
about. And a picker built from the Agent's own list cannot drift: nothing here
carries a catalogue to go stale, except Claude, which has no models command and
where the documented aliases stand in (`docs/tbd/v0.9.md` §5).

Nothing is locked until someone chooses. A fresh install sends no `--model` and
no effort flag at all — the Agent's own default, which is the only honest answer
before anyone has expressed a preference.
