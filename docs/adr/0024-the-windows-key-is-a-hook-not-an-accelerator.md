# ADR-0024: The Windows key is a hook, not an accelerator

**Status:** accepted (v0.10)

## Context

Raycast for Windows opens on a tap of the Windows key, and that is the binding
people who have used it expect. Takyon's binding is `Alt+Space`, which is
contested twice over — PowerToys Run takes it by default and Windows has used it
for the classic window system menu since 3.0 — so a second, uncontested way in is
worth having on its own merits.

The Windows key cannot be registered the way every other binding is.
`tauri-plugin-global-shortcut` wraps `RegisterHotKey`, which binds *chords*: a
virtual key plus modifiers. The Windows key **is** a modifier, so there is no
chord to register. And the shell opens the Start menu on the key's **release**,
when no other key was pressed between the down and the up.

## Decision

A `WH_KEYBOARD_LL` low-level keyboard hook, in `superkey.rs`, **off by default**.

The mechanism is to stop the tap looking like a tap:

- On `LWIN`/`RWIN` **down**, inject an undefined virtual key (`0xE8`, which
  Microsoft's table reserves and nobody handles). The shell now sees a chord and
  is not owed a Start menu on the release.
- On **up**, if no real key intervened, this was a tap: hand the toggle to a
  worker over a channel.
- Everything else falls through untouched, which is what keeps `Win+R`, `Win+E`,
  `Win+L` and every other system chord working.

**Nothing is ever swallowed, in either direction.** Eating the release would also
stop Start, and it is the obvious implementation — but it leaves the OS believing
the Windows key is still logically held, so the next click becomes a `Win+click`
and the state only clears when the key is pressed again. Injecting instead means
the worst failure is the Start menu opening *as well as* the Palette: visible,
reportable, and not a modifier stuck invisibly down.

**Off by default**, unlike Raycast. Three reasons, in order:

1. The hook is on **every keystroke in the system**. Windows silently unhooks a
   callback that exceeds `LowLevelHooksTimeout` (300 ms), and the symptom is the
   binding dying mid-session with nothing anywhere to say so. The callback
   therefore compares a virtual-key code, sets a bool and sends on a channel — no
   allocation, no lock, no logging — and even so it is not something to switch on
   for someone who did not ask.
2. It does not survive an elevated foreground window without the UIAccess
   helper, whose certificate is still v0.1's outstanding item. `Alt+Space` has
   the same limitation, so this is not a regression — but a *default* that stops
   working in front of an admin console is worse than an opt-in that does.
3. Replacing the Start menu is a large thing to do to someone's machine on their
   behalf. The accelerator keeps working either way, so nothing is lost by
   asking.

## Consequences

- `set_super_hotkey` returns **whether the hook is installed**, not whether the
  preference was stored, and the preference is only written when the two agree. A
  switch reading on against a hook that is not there is the worst of the three
  states this control can be in.
- The hook runs on **its own thread with its own message loop**.
  `SetWindowsHookExW` binds the hook to the installing thread and delivers the
  callback through that thread's queue; installed from a worker with no loop it
  reports success and never fires once. Same class of silent failure as creating
  a window from the main thread, and just as hard to read from outside.
- The Keyboard page gained a switch, so the chord became a dropdown. Six chips
  were the whole control when the chord was the whole setting; they are not once
  something sits above them.
- **macOS is the mirror image and is not built.** There the Windows key has no
  analogue and `Alt+Space` is the convention, so the macOS target replaces that
  instead — `docs/plans/post-v1.md`. Nothing in `superkey.rs` is portable; the
  non-Windows `arm` returns `false` and says so.
- The hook is re-armed at every start by `superkey::restore`, after
  `hotkey::register` and never instead of it. A refused hook must not cost anyone
  their accelerator.
