---
status: accepted
---

# The application list is rebuilt every login and never cached to disk

`AppSource` walks its four discovery paths once per process start, on the
deferred-init thread behind the hotkey, and holds the result in memory. Nothing
is written to disk. On the next login it walks again.

## Why not cache it

The obvious design is to persist the list and serve it instantly at the next
login, refreshing behind it. PowerToys Run shipped exactly that — `Win32.json`
and `UWP.json` — and then removed it in
[microsoft/PowerToys#6048](https://github.com/microsoft/PowerToys/issues/6048),
citing three bugs it could not fix while keeping the cache:

- applications installed while the launcher was not running never appeared
  (#5905);
- uninstalled applications kept appearing in results;
- cached UWP icon references outlived the packages that owned them, so packaged
  apps lost their icons (#5998).

Removing it cost them a **65% startup regression** — 11,324 ms with the cache,
18,695 ms without, on their own measurements — and they took that trade anyway.
What replaced it is an in-memory index plus `FileSystemWatcher` on the scanned
locations and `PackageCatalog` events for UWP.

## Why the trade is cheaper here

**The expensive half is already cached.** Icon extraction is what costs real
time, and `icons.bin` persists it keyed by target path and mtime
(`IMPLEMENTATION_PLAN.md` §6), so that work is paid once ever rather than once
per login. What remains is reading a few hundred directory entries.

Measured on the development machine, release build, 2026-08-25:

| | |
|---|---|
| Applications discovered | 1078 |
| Walk time | **~430 ms** (400, 401, 401, 416 ms across runs) |
| Debug build, for comparison | ~1510 ms |

That is an order of magnitude below PowerToys' figure, and it runs after the
hotkey is already live, so it is not on any budget. The exposure is a window of
roughly half a second after login during which the list is incomplete — covered
by `QueryResult::indexing`, which makes the Palette say "Indexing applications…"
rather than showing an empty list.

## When to revisit

**If the measured walk exceeds ~1 s**, add the cache — with the number behind it.
`cargo test v0_2_measure_the_real_walk -- --ignored --nocapture` reproduces the
measurement. A cache is invisible to everything outside `AppSource`: it changes
no IPC type and no UI, so deferring it costs nothing.

## Considered Options

- **Cache, refreshed every launch.** Closes the post-login window entirely.
  Costs a versioned on-disk format and reintroduces the narrow version of
  PowerToys' second bug: an application uninstalled since the last boot appears
  and fails silently on Enter.
- **In-memory plus `ReadDirectoryChangesW` watchers on the Start Menu roots.**
  PowerToys' final design. Kills the staleness class outright and lets an app
  installed mid-session appear without a restart. Pulls watcher code forward from
  v0.7; worth doing when that code exists rather than writing it twice.
