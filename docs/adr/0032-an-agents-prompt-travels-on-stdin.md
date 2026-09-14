---
status: accepted
---

# An Agent's prompt travels on stdin

Every Turn writes its prompt to the Agent's stdin and then closes stdin. No
prompt, and nothing else holding a line break, is ever passed as a command-line
argument. `AgentDriver::turn_args` builds the flags, and `AgentDriver::turn_input`
builds what goes on stdin.

## The problem

Up to v0.11 every driver put the prompt in argv as its last argument. That broke
on a test laptop where opencode was installed with `npm i -g opencode-ai`. npm
installs a batch file, `%APPDATA%\npm\opencode.cmd`, rather than an executable.
Every Turn failed with this message:

```
Could not start opencode: batch file arguments are invalid
```

When the program is a `.bat` or `.cmd`, Rust's standard library runs it through
`cmd.exe /e:ON /v:OFF /d /c`. It builds that command line in
`make_bat_command_line`, which is the fix for CVE-2024-24576 ("BatBadBut"), and
it refuses any argument containing `\r` or `\n`. `styled_prompt` always holds
`\n\n`, so every first Turn to Codex or opencode failed. A multi-line question to
Claude failed too, and so did every `!s`. Settings still showed Ready, because
the probes (`--version`, `models`, `login status`) have no line breaks.

argv also has length limits. `CreateProcessW` accepts at most 32,767 UTF-16
units and `cmd.exe` accepts 8,191 characters. `!s`'s prompt is about 26,000
characters, which is already past the second limit.

## Considered options

- **Escape the arguments ourselves and pass them with `raw_arg`.** This is what
  t3code does with `escapeWindowsShellArg` and `shell: true`, but t3code does it
  because Node refuses to spawn a `.cmd` at all (`spawn EINVAL` since 20.12).
  Rust already routes a `.cmd` through `cmd.exe` and escapes it safely. Porting
  the escaping would bypass that protection, so BatBadBut would become our bug
  to own. It would also leave the length limits in place.
- **Follow the shim to the real executable**, the way t3code's
  `ClaudeExecutable.ts` finds `claude.exe` under `node_modules`. That depends on
  each package's folder layout, it covers one Agent at a time, and it still
  leaves the length limits.
- **Write the prompt to stdin.** Each of the three CLIs already supports it. The
  argv rule and the length limits both go away, and no escaping of any kind is
  involved.

## How each Agent spells it

| Agent | Spelling |
|---|---|
| Claude | `-p` with no positional prompt. `--input-format` defaults to `text` |
| Codex | a trailing `-`, for `exec` and for `exec resume <id>`. `resume` reads stdin only when given an explicit `-` |
| opencode | no message argument. `run` reads stdin when it is not a TTY |

Claude overrides `turn_input` to return the bare prompt, because the answer style
already travels in `--append-system-prompt`, which is a single line. The other two
use the default `turn_input`, which is `styled_prompt`.

## Consequences

- stdin must be **closed**, not only written to. Claude and opencode read to EOF
  before they start. A Turn that kept the handle open would hang forever, and it
  would look like a slow model.
- The write happens on its own thread (`probe::write_input`). Writing 26,000
  characters while nobody reads stdout deadlocks once the child's output pipe
  fills.
- `turn::spawn` is the one Turn spawn path. `Turns::run`, the integration tests
  and `tests/zz_turn_live.rs` all call it, so a test cannot quietly drift from
  production.
- `tests/agents_cli.rs` runs every driver against a fake `echo.cmd` Agent with a
  30,000-character prompt that includes `%PATH%`, quotes and `&|^`. It is the test
  that would have caught the laptop's bug, and it runs on every Windows CI job.
- Each driver's unit tests assert that no argument contains a line break or the
  prompt. That rule also applies to any future Agent.
