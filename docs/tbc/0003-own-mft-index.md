---
status: resolved
pairs-with: ADR-0007
---

# TBC-0003 — File index acquisition

**Resolved before any code was written.** The original bet — build our own
NTFS MFT/USN index — was retired once we measured what unelevated competitors
actually achieve. See ADR-0007. What follows is the live version of the note.

## The bet

Files are acquired by an unelevated, scoped recursive directory walk plus a
filesystem watcher, into a memory-mapped inverted index. The assumption: **query
latency comes from the index structure, not the acquisition mechanism**, so
giving up MFT/USN costs us a slower initial index and less-than-total volume
coverage, and buys us no service, no elevation, no privileged IPC, and non-NTFS
support for free.

Reference point: Raycast indexes 288,592 entries in ~25 s unelevated, out of
1.8 M files present in the profile — deliberately partial, and evidently good
enough to ship.

## How we'd know we were wrong

- Initial walk exceeds **60 s** on a typical machine, or visibly competes with
  login for I/O.
- Watcher event loss is frequent enough that rescans stop being rare, making the
  incremental-update premise false (the same failure mode USN wrap would have
  caused — the problem moved, it didn't disappear).
- Users routinely search for files outside the default roots, so curated scope
  reads as "broken" rather than "focused".
- `!e` p95 exceeds **20 ms**, which would indict the index structure — note this
  triggers a data-structure fix, not a return to MFT.

## Alternatives

| Option | Improvement if we switch | Added complexity | Switching cost |
|---|---|---|---|
| **MFT/USN as an opt-in "whole volume" mode** | ~1 s initial index, total volume coverage | High — requires a LocalSystem service (admin is needed on *every* volume open), privileged IPC to authenticate, plus NTFS internals: resident vs non-resident attributes, hard links, reparse points, journal wrap | **15–25 days**, plus a permanent privileged attack surface and an installer that must register a service |
| **Everything IPC/SDK** | Best-in-class results immediately; someone else owns NTFS correctness | Low technically, high strategically — a hard dependency on a third-party proprietary app that every user must install and run | **3–5 days** behind the `FileIndex` trait. Needs a licensing review if Takyon ships proprietary |
| **Windows Search (`SystemIndex`)** | No index-building work at all; content search comes free | Low code; inherits staleness, frequent disablement, and 10s–100s of ms queries | **3–5 days.** Worth keeping as a fallback for roots we don't walk |

## Verdict if triggered

If coverage is the complaint, widen the default roots and improve the settings UI
before touching acquisition — that is a day of work against fifteen. Only if users
genuinely need instant whole-volume search should MFT/USN return, and then as an
**opt-in accelerator with its own service**, so the unelevated path stays the
default and the privileged path is something a user chooses knowingly.
