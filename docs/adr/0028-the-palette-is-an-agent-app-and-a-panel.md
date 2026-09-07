---
status: accepted
pairs-with: ADR-0003
---

# On macOS the Palette is an agent app and a non-activating panel

Takyon on macOS is an **agent app** — `LSUIElement`, activation policy
`NSApplicationActivationPolicyAccessory` — with no Dock icon and no menu of its
own, and the Palette itself is a **non-activating `NSPanel`** that joins every
Space and floats over full-screen applications. Neither is expressible in
`tauri.conf.json`; both are `objc2` calls against the `NSWindow` handle after
Tauri creates it (ADR-0026).

This is what the reference implementation does. A launcher that cannot be summoned
over a full-screen editor, or that throws the user out of their Space when it
appears, fails the bet ADR-0003 is built on — that the Palette is available
instantly, wherever you are.

## The window, exactly

```
styleMask         .nonactivatingPanel, .fullSizeContentView, .borderless
level             .floating
collectionBehavior  .canJoinAllSpaces | .fullScreenAuxiliary
hidesOnDeactivate false
becomesKeyOnlyIfNeeded true
```

`.canJoinAllSpaces` is what stops the panel vanishing on a Space switch;
`.fullScreenAuxiliary` is what lets it draw over a full-screen app without
displacing it. A panel that steals focus is a missing `.nonactivatingPanel` or an
`NSApp.activate(ignoringOtherApps:)` call in the wrong place — Takyon makes
neither.

## Consequences

**Dismiss-on-focus-loss has to be rebuilt.** This is the largest single
consequence and `docs/plans/macos.md`'s "mostly Tauri already" for `window.rs`
was wrong because of it. A non-activating panel **never becomes the key window**,
so Tauri's `Focused(false)` does not fire for it and
`window::should_hide_on_focus_loss` has nothing to hang on. The macOS replacement
is `NSEvent.addGlobalMonitorForEvents` for `.leftMouseDown | .rightMouseDown`,
hiding the Palette when a click lands outside its frame.

The v0.1 stray-focus guard (`focus_loss_is_stray`, which ignores a focus event
arriving within milliseconds of `show`) still applies — a click that dismissed the
previous invocation must not dismiss the next one — so that logic is reused rather
than rewritten.

**`window.rs`'s placement maths ports unchanged.** `place_on_cursor_monitor`
already goes through Tauri's monitor APIs, including the two-pass re-centring for
mixed scale factors. That is most of the file and it needs nothing.

**ADR-0003's trim has no macOS half.** `SetProcessWorkingSetSize` has no
counterpart; `malloc_zone_pressure_relief` is a different mechanism with a
different effect. So `hide()` does not trim on macOS, and the idle-RSS half of
ADR-0003 is a Windows-only claim. **The 150 MB budget is unmeasured on macOS** and
must not be quoted as if it held: `bun run bench` on real hardware is the first
thing that runs in this port, before feature work, and whatever it reports is
written into ADR-0003 as an amendment. The three latency budgets port unchanged.

**The menu bar item is the only visible affordance, and we do not control it.**
With no Dock icon and no app menu, the `NSStatusItem` is the sole way in besides
the hotkey — and the user can auto-hide the menu bar, a notched MacBook silently
drops overflow items, and Bartender or Ice hide them deliberately. So:

- The item is always created. There is no "hide the menu bar icon" setting on
  macOS, unlike Windows' tray toggle.
- **Relaunching Takyon shows the Palette.** `tauri-plugin-single-instance` is
  already a dependency; its handler shows the window instead of starting a second
  copy. That is the one escape hatch nothing in the environment can take away, and
  it costs one line.

`tray-icon 0.24.2` already depends on `objc2-app-kit` and produces an
`NSStatusItem`, so the tray row of the port is close to free.

**UIAccess is dropped, and its problem does not exist here.** There is no
elevated-window foreground boundary on macOS to lose a race against, so
`uiaccess.rs` has no counterpart and `show()`'s `request_foreground` fallback is
Windows-only. The nearest macOS analogue — the Accessibility permission — is about
*sending* input, not about showing a window, and it is ADR-0030's concern.
