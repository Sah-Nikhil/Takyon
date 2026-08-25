---
status: accepted
---

# Start Menu shortcuts are read raw and never `Resolve`d

`sources/apps/lnk.rs` loads each `.lnk` with `IPersistFile`, reads the stored path
with `SLGP_RAWPATH`, expands `%VAR%` references itself, and checks the target
exists. It never calls `IShellLinkW::Resolve`.

## Why not, given `Resolve` is the documented way

Two separate failures, and the second is the dangerous one.

**`Resolve` searches.** Given a shortcut whose target has moved it hunts the
volume for a match, and for a UNC target it goes to the network and blocks until
the connection times out. That is seconds, per dead shortcut, on a walk budgeted
at a few hundred milliseconds for the whole machine. `SLR_NO_UI | SLR_NOUPDATE |
SLR_NOSEARCH` exists to suppress exactly this, and any call that omits them is a
hang waiting for one stale network shortcut.

**`Resolve` can invoke Windows Installer.** An *advertised* shortcut — what MSI
packages install — resolves by asking the installer to verify the component, which
can raise a repair dialog. On a background discovery thread nobody is watching,
that is a modal dialog appearing from an app the user has not interacted with.

## What is given up

A shortcut whose target has moved is dropped rather than repaired. `Resolve` would
sometimes find it; this will not.

That is the right trade for a launcher. The plan's own instruction is to "resolve
and drop dead ones at index time, not launch time" — the point is that a row which
can only fail must never be offered, and an existence check achieves that without
the two failure modes above. A shortcut pointing at something genuinely gone is
broken in the Start Menu too, and Takyon showing it would be showing a row whose
only outcome is an error.

## Considered Options

- **`Resolve` with the suppression flags.** Correct, and one forgotten flag turns
  a fast walk into a multi-second hang with no visible cause. Not calling the
  function at all cannot be got wrong later.
- **`Resolve` on launch rather than at index time.** Moves the cost to the one
  moment latency is most visible, and still risks the installer dialog — now while
  the user is waiting for an application to start.
