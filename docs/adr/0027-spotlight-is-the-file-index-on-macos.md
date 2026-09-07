---
status: accepted
supersedes-on-macos: ADR-0007
---

# Spotlight is the file index on macOS

On macOS, Takyon does not walk directories and does not run a filesystem watcher.
It queries **Spotlight through the CoreServices `MDQuery` C API with
`kMDQuerySynchronous`**, behind the existing `FileIndex` trait, as a
`SpotlightIndex` implementor.

ADR-0007 chose a scoped userspace walk plus `ReadDirectoryChangesW` over an
MFT/USN index. Every argument it made was **about Windows**: elevation on every
raw-volume open, a LocalSystem service, non-NTFS volumes. None of those apply
here. macOS ships a complete, always-running, permissionless file index, and the
reference implementation for this product category — Raycast — simply queries it
rather than building a second one.

This supersedes ADR-0007 **on macOS only**. Windows keeps the walk, and ADR-0007
stands there unchanged.

## Why `MDQuery` and not `NSMetadataQuery`

`NSMetadataQuery` is the Foundation wrapper and it is **asynchronous**: it
delivers results by notification, typically 100 ms or more for a first batch.
`Source::query` is synchronous with a deadline. Bolting one to the other means
either an empty first return with results popping in afterwards, or blocking the
query pipeline on a notification.

`MDQuery` is the underlying CoreServices API and it takes a `kMDQuerySynchronous`
flag that blocks through the gather phase and returns a complete result set. Same
index, same coverage, and it fits the existing `Source` contract with **no
architectural change at all** — the deadline `query.rs` already passes is the
bound, exactly as it is for the walked index.

It is reachable through `core-foundation` (already in `Cargo.lock` at 0.10.1) with
`extern "C"` declarations against the Metadata framework. No new crate.

## Considered Options

- **Port ADR-0007 literally**: walk plus FSEvents, Spotlight as a fallback. This
  is what `docs/plans/v0.12-macos.md` originally listed, at roughly 1,110 lines. It also
  requires an FSEvents dependency the tree does not have.
- **`NSMetadataQuery` as primary**: right index, wrong shape — see above.
- **`MDQuery` synchronous as primary, no walk and no watcher.** Chosen. Roughly
  150 lines against 1,110.

## Consequences

**Scope becomes a query predicate rather than a walk boundary.** The
`files_roots` setting maps onto `MDQuery`'s native search scopes, so the Files
settings page keeps meaning exactly what it means on Windows — the user's chosen
roots — rather than becoming meaningless or hidden. `files_excludes` becomes a
post-filter on the result set, which is cheap at these counts.

**Exclusions the user set in System Settings win, and we do not surface them.**
Spotlight has its own Search Privacy list. A file excluded there is invisible to
`MDQuery` and Takyon has no way to know why. Accepted: it is the user's own
setting, and fighting it would be worse.

**ADR-0007's watcher-overflow correctness requirement disappears on macOS.** There
is no buffer to overflow and no rescan to trigger, because there is no index of
ours to go stale. That requirement stays live on Windows.

**Live-entry counts in Settings have no macOS meaning.** The Windows page shows
how many entries each root contributed. Spotlight does not report that, and
running a count query per root to fake it would be work for a number nobody acts
on. The macOS page shows the roots without counts.

**TCC is not triggered.** Spotlight metadata queries return paths without a
Full Disk Access prompt, which keeps ADR-0007's no-elevation, no-prompt posture
intact — the property that made ADR-0007 the right call in the first place, now
obtained for free.

**Spotlight can be off.** A user who has disabled indexing on a volume, or
disabled Spotlight entirely, gets no file Entries. This is the one honest
regression against a walk we control. It is also rare — far rarer than Windows
Search being off, which is why the Windows inversion is a toggle rather than a
default (`docs/plans/v0.13-os-index.md`).

**Windows gets the same option, later and behind a setting.** `index/wsearch.rs`
already queries Windows Search through OLE DB as an off-by-default fallback.
Promoting it to a peer of the walk is a second implementor of the seam this ADR
forces into existence — cheaper after the port than before it. That work is
`docs/plans/v0.13-os-index.md` and is deliberately not part of this decision.
