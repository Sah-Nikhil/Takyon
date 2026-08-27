---
status: accepted
---

# OS state lives in the OS; app behaviour lives in our own storage

Whether Takyon starts at login is not a Takyon setting. It is a fact about the
machine, Windows owns it, and Takyon's job is to ask rather than to remember.

This rule was implemented in v0.1 and the code cited tesseract's ADR-0026 for it,
across repository boundaries, from three separate comments. That is a pointer to a
file this project does not control, in a repository a future reader may not have
open. The rule is restated here so Takyon owns its own reasoning; tesseract's
version remains the origin and the longer argument.

## Why not a row in `settings.db`

**Drift is guaranteed, not hypothetical.** Task Manager → Startup apps flips the
login registration behind the app's back, and it emits no event Takyon can observe.
A mirrored `launch_at_startup` column would therefore routinely display the
opposite of what the machine will do at the next login, and display it confidently,
in a switch, with nothing to tell the user it was lying. That is worse than having
no switch at all.

**It is not portable state.** Everything else in `settings.db` describes how Takyon
behaves. "Start at login" is a claim about one particular Windows profile, and a
settings file carried to a second machine has no business making it.

## The rule

**Can something other than Takyon change this? If yes, Takyon must ask rather than
remember.**

| | Owner | Read by | Written by |
|---|---|---|---|
| Start at login | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\com.v3sper.launcher`, plus the `Explorer\StartupApproved\Run` flag beside it | `@tauri-apps/plugin-autostart`'s `isEnabled()`, on every mount of Settings | that plugin's `enable()`/`disable()`, and anything else on the machine |
| "Turn off animations" | `localStorage` today, `settings.db` from v0.6 | `src/prefs.ts` | the Settings switch |
| Hotkey accelerator (v0.6) | `settings.db` | the registration at startup | the Settings control |
| Whether the question has been asked | the `first-run-complete` marker file in the ADR-0011 data directory | `firstrun::already_asked` | `firstrun::mark_asked` |

There is deliberately **no autostart field anywhere**: not in `settings.db`, not in
`api.ts` as cached state, not as a Tauri command of our own. `api.ts` re-exports
the plugin calls and nothing else.

The last row is the interesting one, because it is the boundary. *Whether the user
said yes* is OS state and is never stored. *Whether they were asked* is Takyon's own
history, nothing else can change it, and it is ours to keep. Conflating the two
would re-ask everyone who declined.

## Consequences

- Settings re-reads `isEnabled()` on every mount, so a change made in Task Manager
  is picked up the next time Settings opens. There is no notification of such a
  change and the switch can be stale while the window sits open. Accepted: the
  alternative is polling the registry forever to catch an event that happens a
  handful of times in a machine's life.
- After writing, the switch **re-reads rather than trusting the write**. If the
  registry write was refused — group policy, a locked hive — the control must show
  what the OS says, not what the user clicked. It must also *say* that it failed;
  v0.1 re-reads but reports nothing, which is recorded as a gap in
  [`../tbd/v0.1.md`](../tbd/v0.1.md) §3 and owned by v0.6.
- **Self-heal, not sync.** `tray::self_heal_autostart` re-registers the current
  `current_exe()` at startup, but only when `is_enabled()` already says on. It
  corrects a stale path after an update or a reinstall moves the binary — a failure
  that happens silently at boot, where nobody is watching — and it never re-enables
  something the user turned off. That guard is what keeps it inside this ADR rather
  than a backdoor around it.
- Windows' `StartupApproved` flag is handled by `auto-launch` 0.5.0: `is_enabled()`
  ANDs the `Run` value against it and `enable()` writes the approving blob. So a
  Task Manager disable reads as off, which is the honest answer. `disable()` does
  **not** clear the flag, which is why the NSIS uninstall hook deletes both values
  by name.

## What this does not license

Every other preference stays where it was. This is an exception with a stated
boundary — the ownership question above — not permission to put settings anywhere.

## Two tesseract controls that are deliberately not ported

Tesseract pairs autostart with `close_to_tray` and `start_minimized`. Neither has
an analogue here, and a future session should not add them:

- **Close to tray.** The Palette has no close button, no taskbar button
  (`skipTaskbar`) and no decorations, and it is hidden rather than destroyed
  (ADR-0003). `Alt+F4` already hides it. There is no ✕ whose meaning could be
  configured.
- **Start hidden in the tray.** The Palette starts hidden always, `visible: false`
  in `tauri.conf.json`, because a launcher that opens a window at login is a
  launcher that interrupts login. There is no other behaviour to offer.

Tesseract needs both because it is a document application whose window is the
point. Takyon's window is a modal that spends its life invisible.
