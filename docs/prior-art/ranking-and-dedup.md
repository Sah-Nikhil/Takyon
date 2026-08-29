# Prior art — how Raycast and PowerToys rank and de-duplicate apps

Reverse-engineered 2026-08-29 to settle Takyon's ranking and duplicate-handling
decisions against the two Windows launchers we actually use. Raycast for Windows
2.1.1.0 (all local DBs SQLite-SEE encrypted, so this is read from the migration
SQL and JS bundle, not live rows) and PowerToys (open source, MIT, HEAD
`7bf87a3`, read directly). Where a claim is a code reading contradicted by
observed behaviour, that is called out — the live tool wins.

## The one finding that matters

**Neither tool has Takyon's duplicate problem, because neither creates the
duplicate.** The difference is at index time and match time, not a
collapse-after-the-fact step:

- **Raycast indexes shell-registered apps** (Start Menu, AUMIDs, packaged) and
  does not walk `PATH` for bare executables. So it has one "File Explorer" and no
  separate "explorer" row. Takyon's `path.rs` surfaces `c:\windows\explorer.exe`
  as its own row on top of the `File Explorer` AUMID — a row Raycast never makes.
- **Raycast matches display name + curated keywords, weighting the raw exe
  basename low.** Observed: `chrome` returns Google Chrome only, never Helium
  (a Chromium fork whose binary is `chrome.exe`). The fork's name is "Helium";
  "chrome" appears only in its path, and the sensitivity gate drops a path-only
  match that deep. Takyon's exe-stem rung matched the fork by its binary name —
  fixed in 0.2.5 by dropping exe-stem-only matches when anything matches by name,
  which reproduces Raycast's behaviour.

The agent's static read of Raycast's dedup key suggested both copies stay in the
list, disambiguated visually. Live testing contradicted that: Raycast shows one.
The reconstruction was right about the *code* and wrong about *what reaches the
index*. Lesson kept: verify a competitor claim against the running product.

## PowerToys — de-duplication (read in source)

Both PowerToys Run and the newer Command Palette dedup with the same key:

```
Win32ProgramEqualityComparer:
  equal iff (Name, ExecutableName, FullPath).ToUpperInvariant() all equal
```

`FullPath` is in the key, so this collapses only the *identical file discovered
twice* (e.g. one shortcut found by both Start Menu and Desktop scans). It does
**not** merge:

| case | result |
|---|---|
| two `node.exe` in different directories | both kept |
| System32 vs SysWOW64 twin of one exe | both kept — no SysWOW64 preference exists anywhere in the repo |
| a fork's `chrome.exe` (Helium) | both kept |
| the same shortcut in two scanned roots | deduped to one |

There is no cross Win32↔UWP consolidation — an app installed as both an exe and a
Store package shows twice. PowerToys is not a model for merging semantically-equal
apps; its key means "byte-identical," nothing more.

## Ranking — strong three-way convergence

All three independently landed on the same spine: **a hard tier ladder where the
tier dominates absolutely, and frecency only breaks ties within a tier — it never
crosses a tier boundary.** This is exactly Takyon's model (`rank.rs` `TIER_*`
constants) and exactly the Stability rule's intent, so the design is confirmed
rather than merely plausible.

| | Takyon | CmdPal | Raycast |
|---|---|---|---|
| tier ladder, tier wins | `TIER_*` | `tier*10_000_000 + within` | match-class first |
| frecency = within-tier tiebreak only | yes | yes | yes (with a deliberate `frecency > 1` override at two steps) |
| real time decay | half-life 30 d | half-life **3 d** | exponential decay |

**CmdPal frecency** (worth reading against TBC-0009):

```
recency   = 2^(-ageDays / 3)
frequency = log2(uses + 1)
weight    = clamp(recency * (10 + 10*frequency), 0, 70)
```

keyed by `"{Name}|{FullPath}"`. **Raycast frecency** is smaller still: a single
`REAL frecency_date` float per item, no visit-count column — one moving virtual
timestamp encodes frequency and recency together, and the score is derived at
read time with exponential decay against a sampling date. Default score when no
row exists is 1.

**PowerToys Run** is the cautionary baseline: one additive scalar
(`score + SelectedCount*5`), usage is a **flat lifetime click count with no
decay** (every past launch adds `+5` forever), and a blunt relative cutoff
(`keep Score > 0.75 * maxScore`). CmdPal dropped all three for the tier gate.
Treat CmdPal as the reference and Run as what to avoid.

## Worth borrowing

1. **Split identity from ranking** (both Raycast and our own layout). The app
   index carries identity only — no scores; frecency lives in its own store keyed
   by a stable id. We already do this.
2. **The tier ladder as the ordering spine.** Confirmed by all three. Keep it.
3. **Raycast's frecency shape** — one `REAL` timestamp, no count column, score
   derived at read — is simpler than our decayed-score + `decayed_at` pair. Not
   worth a rewrite now, but the cleaner target if TBC-0009 reopens.
4. **Half-life is an open guess and ours may be sedate.** CmdPal ships 3 days
   against our 30. A datapoint for TBC-0009, not a correction.
5. **Match display name + a few curated keywords, weight the raw exe basename
   low.** This is what stops a fork surfacing under upstream's name. Our exe-stem
   gate (0.2.5) is the same idea; Raycast validates it.
6. **Disambiguate rather than merge, where a duplicate is legitimate.** Raycast
   shows the parent directory as a subtitle when two apps share a display name,
   escalating to the full path when name+folder also collide. Our
   version-beside-title (0.2.4) is the same strategy with a better signal for the
   two-installs case.
7. **Sensitivity as a shipped knob** (`low` / `medium` / `high`). Raycast's medium
   gate is `score >= 1.5*(len - skipped - 2) + 4`.
8. **Search-term learning gated by recency.** Raycast honours "you typed `ff`
   then launched Firefox" only if that association is under ~17 days old.

## Worth avoiding

1. **PowerToys' dedup key is not semantic.** It merges byte-identical files only,
   so it keeps every case we care about. Do not copy it as a merge strategy.
2. **PowerToys Run's flat, never-decaying click count** and its
   `0.75 * maxScore` relative cutoff. Both are why a strong top hit buries decent
   alternatives.
3. **SQLite-SEE** (Raycast's at-rest encryption) is commercial and proprietary —
   a licensing cost and a trap for a launcher whose licence is still open
   (ADR-0005). Encrypting every DB also cost this whole exercise its readability;
   keep the app index plaintext under DPAPI-protected ACLs and encrypt only
   sensitive content.
4. **Raycast's single-app resolver falls back to alphabetical** when two
   same-named binaries have equal (or zero) frecency. Weak. If Takyon ever
   auto-picks one app for a hotkey or bang, prefer a Source-origin signal (a
   signed Start-Menu shortcut over a raw `PATH` exe) as the tiebreak — neither
   reference tool has this and both are worse for lacking it.

## What still points at Takyon's own decisions

- The **learned collapse** feature (v0.3 task 1b) was retired on the strength of
  this comparison. It solved a problem neither reference tool has — merging a
  duplicate they never create — and it could only act *after* two learned
  launches, so a fresh machine showed both rows. Replaced by the same index/match
  discipline the reference tools use: a Windows-dir binary the shell already lists
  as an app is dropped at discovery (`WINDOWS_DIR_APP_DUPLICATES` — `explorer`
  with `calc`/`notepad`), the exe-stem gate keeps a fork off upstream's name, and
  two genuinely different same-named executables stay two rows disambiguated by
  version. The full retirement note is in
  [`../tbd/v0.3.md`](../tbd/v0.3.md) §7.
- **What Raycast actually indexes** was inferred from behaviour, not read from its
  discovery code (the agent focused on ranking and dedup). If the exact
  include/exclude rule matters later, it is worth a dedicated pass.
