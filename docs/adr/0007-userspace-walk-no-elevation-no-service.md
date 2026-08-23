---
status: accepted
supersedes: ADR-0004
---

# File indexing runs unelevated: scoped userspace walk, no service

ADR-0004 chose a self-built NTFS MFT/USN index. That decision assumed elevation
was a one-time cost. It isn't: opening a raw volume handle and reading the USN
journal requires administrator privileges on *every* open, which is why Everything
ships a Windows service. We revisited after measuring what the competition
actually does.

**Takyon indexes files with an unelevated, scoped recursive directory walk
plus a filesystem watcher, into a memory-mapped inverted index. No service, no
elevation, no raw volume access.** MFT/USN survives as an optional post-V1
accelerator behind the `FileIndex` trait, not as the V1 mechanism.

## Why the reversal

Evidence from Raycast 2.0.5 as installed on the development machine: no service
registered, never elevated, `%LOCALAPPDATA%\Raycast\index\stats.json` reporting
288,592 entries indexed in 24.7 seconds — roughly 11,700 entries/second, which is
directory-walk throughput, not MFT-parse throughput (an MFT parse of that many
records is one to two seconds). Its `watch.json` carries an event id and
timestamp, indicating a filesystem watcher rather than a USN cursor.

The decisive realisation: **MFT/USN does not buy query latency.** Query latency is
a property of the index data structure, and an inverted index over a few hundred
thousand entries answers in well under a millisecond however those entries were
acquired. MFT/USN buys exactly two things — whole-volume completeness, and a
~1-second initial index instead of ~25 seconds. A one-time 25-second background
walk is not a user-visible cost. A LocalSystem service is a permanent one.

The second decisive number: the development machine's user profile contains
1,803,451 files; Raycast indexes 288,592 of them. **Completeness is not the
product requirement we assumed it was.** Indexing a curated set of locations
beats indexing everything, both for relevance and for cost.

## Consequences

- No installer-level privilege requirement, no privileged IPC surface, and
  non-NTFS volumes work by the same path as NTFS ones.
- Which roots are indexed becomes a **product decision with a settings UI**, not
  an implementation detail. Sensible defaults (Desktop, Documents, Downloads,
  code directories) with user-editable roots and exclusions.
- Watchers drop events under load exactly as USN journals wrap — the buffer
  overflows and Windows reports that enumeration is required. The correctness
  requirement is unchanged: detect the gap definitively and trigger a rescan
  rather than silently serving a stale index.
- V1 gets materially smaller. This was the largest single piece of work in the
  plan and it roughly halves.

## The separate finding: UIAccess

Raycast ships `Raycast.UIAccess.exe`. Windows' `uiAccess="true"` manifest flag
lets an unelevated process take foreground above elevated windows — without it,
the Palette will not appear over an administrator terminal, which is a visible
failure for a launcher. It requires a **signed binary installed to a trusted
location** (Program Files), so:

- Code signing moves from v1.0 to roughly v0.1.
- Portable / no-install mode cannot work, independently of any other reason.
