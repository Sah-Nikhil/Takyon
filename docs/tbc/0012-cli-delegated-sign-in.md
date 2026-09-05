---
status: open
pairs-with: ADR-0017
---

# TBC-0012 — Sign-in is delegated to the Agent's own CLI

## The bet

Takyon reads an Agent's Sign-in state and, when it is signed out, prints the
sentence that says what to run. It does not run it. This is T3 Code's surface
copied exactly (ADR-0017), and the bet is that a user who has installed
`claude`, `codex` or `opencode` has a terminal and knows how to use it.

## The assumption under it

**Whoever installs a coding-agent CLI can run one more command in the same shell
they installed it from.** Every one of these Agents is installed by
`npm i -g`, `bun add -g`, `winget` or an install script — there is no path to
having them that does not go through a terminal.

## What would disprove it

- A Takyon user who has an Agent installed, is signed out, and does not act on
  the sentence. One report is an anecdote; the shape of the report is the signal
  — "I didn't know where to type that" disproves the assumption, "I did it and
  Takyon still said signed out" is a probe bug.
- Any Agent shipping a Windows installer that does not leave a terminal in the
  user's hands.
- The sentence itself failing: `codex login` opens a browser, and if that browser
  flow needs the terminal to stay open we are telling the user to do something
  more fragile than it sounds.

## The amendment, if it is disproved

A **Sign in** button on the Agent's Settings card that opens a real console
window running the Agent's login command, then watches for the Sign-in state to
change and re-probes.

The mechanics, so this is a decision and not a wish:

- **Windows Terminal (`wt.exe`) first, `conhost.exe` second.** `wt.exe` is
  present on Windows 11 and absent on plenty of Windows 10 machines; `conhost` is
  always there and is uglier. Both are launched through `ShellExecuteW` like
  every other launch (v0.2 task 7), so Takyon does not hand the child its
  handles.
- **Takyon does not read the console.** It watches the Sign-in state by
  re-probing on an interval while the window is open, which is the same probe the
  card already runs. Screen-scraping a terminal to detect success is exactly the
  fragility ADR-0017 refuses.
- **The button never appears for an Agent that is not installed.** "Not found on
  PATH" is a different sentence with a different fix.
- **No elevation, ever.** None of the three login flows needs it, and a launcher
  that asks for a UAC prompt to sign in to a chat tool has lost the argument.

## What switching costs

Small — one command, one button, one poll. It is deliberately deferred rather
than cheap-and-skipped: shipping the exact T3 Code surface first means we find
out whether the sentence is enough before building the thing that assumes it is
not.
