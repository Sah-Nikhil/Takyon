---
status: superseded by ADR-0007
---

# File search uses our own MFT/USN index, reached through `!e`

> **Superseded in part.** The `!e` gating and the `FileIndex` trait stand. The
> MFT/USN acquisition mechanism does not — it assumed elevation was a one-time
> cost, when it is required on every volume open. See
> [ADR-0007](./0007-userspace-walk-no-elevation-no-service.md) for the evidence
> and the replacement.

File search is gated behind `!e` rather than mixed into Bangless results, and it
is served by an index we build ourselves: a one-time read of the NTFS Master File
Table (requiring a single elevation on first run), kept live afterwards by
subscribing to the USN change journal. The index is stored as a compact
memory-mapped file so resident memory stays low and the OS page cache does the
work. V1 indexes filenames and folder names only, not file contents.

Gating behind a Bang is what makes this affordable: the index does not need to
exist until the user's first `!e`, so it costs nothing at idle for people who
never search files, and Bangless relevance stays clean — typing `report` means
"launch something called report", not "here are 4,000 matching files".

## Considered Options

- **Everything's IPC/SDK** (voidtools): world-class results in about a week, but
  requires every user to install and run a separate proprietary application with a
  background service, and reduces Takyon to a front-end for someone else's
  product — untenable if this is ever sold.
- **Windows Search (`SystemIndex`)**: already present and indexes content, but is
  frequently disabled, often stale or partial, and answers in tens to hundreds of
  milliseconds. Inheriting it means inheriting the reason Windows search feels
  bad. Retained as a fallback for non-NTFS volumes.
- **Files in the Bangless list** (as Spotlight and Raycast do): rejected for the
  relevance and idle-cost reasons above, with two mitigations — recently-opened
  files are a cheap Bangless Source requiring no index, and a default-off setting
  lets users opt into full file results without `!e`.

## Consequences

Sits behind a `FileIndex` trait so the Windows Search fallback and any future
macOS backend can be swapped in. This is the largest single piece of work in V1;
budget accordingly.
