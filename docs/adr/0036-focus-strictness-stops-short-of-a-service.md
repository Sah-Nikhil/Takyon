---
status: accepted
---

# Focus strictness stops short of a service

A Focus session is either **Soft** or **Strict**, chosen per session. A third
level, **Locked**, which the user cannot end at all, is designed here and
deliberately not built. It needs a Windows service, and ADR-0007 rules that out
until it is amended.

## The problem

The user wanted to choose how hard a Focus session is to escape, from Raycast's
level (nothing stops you) up to "cannot be ended". Raycast itself has no strict
mode. Pause, Complete and Cancel are always one action away, and the one lever is
setting Snooze to none.

What makes a level real on Windows is who can kill what. A process can always be
killed by the user who started it, from Task Manager, so anything running as
the user is escapable by that user. A lock that holds against the user has to
run as someone else.

## The decision

**Soft**, Raycast's level:

- Snooze one blocked target for 1, 3, 5 or 10 minutes, or never. The default is 3.
- Pause and Complete are free.
- Quitting Takyon ends blocking until it is relaunched.

**Strict**, which works by friction plus persistence:

- No snooze.
- Pausing or completing early takes retyping the goal exactly and then a 30 s
  countdown.
- Quit from the tray asks to confirm.
- The running session lives in `focus.db`, so relaunching or rebooting resumes
  it with the right time left, and autostart makes a reboot no escape.
- Two hurdles default on, both switchable in Settings:
  - shutdown, restart and sign-out are held behind Windows' "preventing
    shutdown" screen with the session's reason (`ShutdownBlockReasonCreate`);
  - idle sleep is held off (`SetThreadExecutionState`).

Task Manager still ends a Strict session until the next launch. That is the
documented ceiling, and Settings says it in words rather than implying more.

**Locked** appears in Settings as a Coming soon row and in
`docs/plans/post-v1.md`. Building it means:

- **A Windows service running as `LocalSystem`**, registered by the installer.
  NSIS already runs elevated to place the UIAccess helper, so registration
  itself is cheap.
- **The service enforces app blocking and restarts Takyon if it is killed.** It
  runs in session 0 and cannot use UI Automation on the user's desktop, so
  website blocking still runs in Takyon itself. The service's job there is only
  to keep Takyon alive.
- **A new ADR amending ADR-0007**, signing for the service binary, and a
  security review of a privileged process that takes commands from an
  unprivileged one.

## Considered options

- **Two user-level processes that restart each other.** This is exactly how
  malware persists. Defender already scores Takyon's running process as
  `Trojan:Win32/Bearfoos.A!ml` (`docs/tbd/v0.9.md` §11), and this would make it
  certain. Ruled out permanently, not deferred.
- **Locked in v0.16.** The service, the installer change, the ADR and the
  review are more work than the rest of Focus combined, and they would have
  stalled the phase.
- **Holding back user-initiated sleep.** Impossible. Applications lost the
  ability to veto suspend (`PBT_APMQUERYSUSPEND`) in Vista. Sleep pauses the
  session instead.

## Consequences

- **Locked is only friction for an administrator.** On a home PC the user is
  usually the admin and can stop the service or uninstall. Its value is the
  effort it costs, not a guarantee, and its copy must say so.
- **The shutdown hurdle always loses to Shut down anyway.** That is Windows'
  rule, and the right one.
- **Strict's persistence makes `focus.db` load-bearing at startup.** It is read
  in deferred init right after the hotkey, inside the 500 ms login budget, so
  it has to stay small. One running row, read by index.
- **The strictness level is stored per session**, so a stats view can tell Soft
  sessions from Strict ones. Adding Locked later adds a value, not a column.
