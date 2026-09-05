# Takyon Implementation Plan

The "how" companion to [`ROADMAP.md`](./ROADMAP.md) (what, in what order) and
[`CONTEXT.md`](./CONTEXT.md) + [`docs/adr/`](./docs/adr/) (why). Written to be
handed to a coding agent one phase at a time: read the ROADMAP section for the
checklist, this document for the shape of the code, the ADRs when something is
ambiguous.

**Rule for whoever picks this up, human or agent:** don't re-derive settled
decisions. If `CONTEXT.md` defines a term or an ADR settles a tradeoff, it's
fixed. If you hit a genuine gap this document doesn't cover, flag it rather than
improvising — several ADRs exist precisely because the obvious default was wrong
here. `docs/tbc/` records which decisions we expect to revisit and what switching
each would cost.

**Nothing below is built.** There is no `apps/` directory yet.

---

## 1. Structure

```
takyon/
├── apps/desktop/
│   ├── src/                       React 19 + Vite 7 + Tailwind v4
│   │   ├── api.ts                 THE seam — the only file that calls invoke()
│   │   ├── palette/               the Palette
│   │   ├── settings/              settings window
│   │   └── agents/                Agent status copy and the Turn hook
│   └── src-tauri/
│       ├── src/
│       │   ├── lib.rs             builder chain, plugin registration
│       │   ├── window.rs          warm/trim, show/hide, monitor placement
│       │   ├── hotkey.rs          global shortcut, rebinding, conflict reporting
│       │   ├── tray.rs            tray icon, autostart self-heal
│       │   ├── query.rs           the pipeline (§3)
│       │   ├── rank.rs            matching, Frecency, the Stability rule
│       │   ├── bang.rs            parser + Mode registry
│       │   ├── agents/            AgentDriver trait, three drivers, Turns (v0.8)
│       │   ├── sources/           apps · files · clipboard · calc · recents
│       │   ├── index/             walker · watcher · store (mmap)
│       │   ├── store/             settings · frecency · clips (SQLite)
│       │   └── icons.rs           extraction into one mmapped blob
│       └── capabilities/          Tauri capability files
└── packages/shared/               TS types mirroring the IPC contract
```

The workspace exists ahead of need so the macOS seams are in place (ROADMAP
answer, round 4). **The seams that matter are the Rust traits below, not the
directory layout.**

---

## 2. Core types and traits

One type crosses the IPC boundary for results. Everything else is internal.

```rust
pub struct Entry {
    pub id:       EntryId,          // stable across restarts — it is the Frecency key
    pub title:    String,
    pub subtitle: Option<String>,
    pub kind:     EntryKind,        // App = System > Folder > File > Recent > Clip; Calc wins outright
    pub icon:     Option<IconRef>,  // offset into the icon blob, never a path
    pub score:    f32,
    pub actions:  Vec<ActionId>,
}
```

`EntryId` **must be stable** or Frecency silently resets: use the resolved target
path for an App, the full path for a File, the row id for a Clip. Never a hash of
the display name — display names change when apps update.

**Amended at v0.3.** For an App the id is the target path *plus its launch
arguments where it has them*, joined by `|` (illegal in a Windows path). The
argument-free form is byte-identical to what v0.2 wrote, so nothing already
learned is invalidated. The reason is a host binary: nine Start Menu shortcuts on
the development machine run `cmd.exe` and the arguments are what make them nine
different applications — they collapsed onto one id and fifteen distinctly-named
applications disappeared. Measured in [`docs/tbd/v0.2.md`](./docs/tbd/v0.2.md)
§9; the rule is [ADR-0014](./docs/adr/0014-durable-identity-wins-a-collision.md).
The working directory is *not* part of the id.

`subtitle` is populated by the Source but **shown only when it disambiguates** —
[ADR-0016](./docs/adr/0016-the-second-line-is-disambiguation.md). The pipeline
clears it on every Entry whose title is unique in the list being returned.

```rust
pub trait Source: Send + Sync {
    fn id(&self) -> SourceId;
    /// Must return within `budget` or return nothing. See §3.
    fn query(&self, q: &Query, budget: Duration) -> Vec<Entry>;
    fn actions(&self, entry: &Entry) -> Vec<Action>;
}

pub trait FileIndex: Send + Sync {
    fn search(&self, q: &str, limit: usize) -> Vec<FileHit>;
    fn generation(&self) -> u64;          // bumped on any rescan
    fn status(&self) -> IndexStatus;      // Ready | Building { pct } | Stale
}

pub trait SearchProvider: Send + Sync {          // v0.9, as built
    fn label(&self) -> &'static str;
    // Blocking, not async: it runs on a Turn's own thread, and the rest of the
    // Rust here is thread-based. WinHTTP is blocking too (ADR-0019).
    fn search(&self, query: &str, key: &str) -> Result<Vec<Hit>, SearchError>;
}

pub trait GameLibrary {                          // v0.3
    fn launcher(&self) -> GameLauncher;          // Steam | Epic | …
    fn games(&self) -> Vec<Game>;                // { launcher, id, name }
}

pub trait ClipboardStore: Send + Sync {          // v0.5
    fn record(&self, clip: Clip) -> Result<()>;
    fn search(&self, q: &str, limit: usize) -> Result<Vec<Clip>>;
    fn sweep(&self, retention: Retention) -> Result<u64>;
}
```

`GameLibrary` is the one trait here that is not a Source. Game launchers all
answer the same two questions — which games are installed, what id starts each —
and each invented its own format to answer them: Steam a VDF tree, Epic a
directory of JSON manifests, GOG a SQLite database, Battle.net a protobuf. The
trait is where the sharing stops, deliberately; the bodies share nothing. What it
buys is that a launcher is a module plus a line in `games::all()`, and that
`LaunchTarget` carries **one** `Game { launcher, id }` variant rather than one per
store. The id is the launcher's own, so a game that moves drive keeps its
Frecency. Xbox and Game Pass need no implementor — those are MSIX packages with
AUMIDs and `appsfolder.rs` already lists them.

**No Source knows anything about the UI.** Sources return Entries; ranking and
rendering are separate concerns. This is what keeps TBC-0002's escape hatch — a
native Palette with no webview — affordable, and it's the single most important
architectural constraint here.

---

## 3. The query pipeline

**One `invoke` per keystroke, never one per Source** (ADR-0009). The Rust side
fans out, merges, ranks and returns once.

```
input ──▶ bang::parse ──┬─▶ Bangless ─▶ fan out to Sources (rayon, 20 ms budget)
                        │                        │
                        │                        ▼
                        │      merge ─▶ rank ─▶ stability ─▶ top 12 ─▶ subtitles
                        │
                        └─▶ Bang(mode, rest) ─▶ that Mode alone, its own semantics
```

**Budget-bounded fan-out.** Each Source gets a deadline. A Source that misses it
contributes nothing *for that keystroke* — no partial results, no late insertion.
This is what makes the Stability rule cheap rather than a special case.

**The Stability rule** (ROADMAP v0.3), concretely:

```rust
struct StabilityLock { query: String, top: EntryId, locked_at: Instant }
```

100 ms after the last keystroke, the current top Entry's id is locked for that
exact query string. Later results for the same string may append below but may
not displace the locked top. A new keystroke clears the lock. This is directly
unit-testable and should have a test from the day ranking exists — it is the rule
that stops the user launching the wrong thing.

**Built at v0.3.** `first_seen_ms` rather than an `Instant`, so "now" is a
monotonic millisecond count a test can supply; a wall clock here would make the
rule untestable. Three cases, all asserted: a settled top holds against a better
late answer, a new query string ranks freshly, and inside the delay the list
still reorders — the guarantee is about a *stopped* query, not a frozen first
answer. An empty query clears the lock, or a stale one would apply to whatever is
typed next.

If the locked Entry is absent from a later result set, nothing is promoted. There
is no case for synthesising a row that the Sources no longer return.

**Sequence numbers.** `query(q, seq)` carries a monotonic counter; the frontend
discards any response whose `seq` is lower than the newest it has seen. Without
this a slow keystroke's results can overwrite a fast one's.

### Matching (ROADMAP v0.3)

Word-boundary prefix + executable basename + acronym + user aliases. Scoring
tiers, highest wins, then multiplied by the Frecency weight:

| Tier | Score | Example |
|---|---|---|
| Alias exact | 1000 | user maps `ps` → Photoshop |
| Exact full name | 900 | `code` → **Code**, outright |
| Shipped keyword | 850 | `wifi` → the Network settings page |
| Full-name prefix | 800 | `adobe` → **Adobe** Photoshop |
| Later-word-boundary prefix | 700 | `photo` → Adobe **Photo**shop |
| Executable basename prefix | 650 | `devenv` → devenv.exe (Visual Studio) |
| Acronym of initials | 600 | `vsc` → **V**isual **S**tudio **C**ode |

**Amended again at v0.3.** The curated `ms-settings:` keywords (task 8) were put
on the *alias* rung, which is the user's own naming and outranks everything. Live,
`disk` then reached the **Storage** page above an application called **Disk
Cleanup** — a keyword we shipped beating a name the user can see. Keywords have
their own rung at 850: above a name prefix, below an exact name, and always below
an alias the user wrote. This also took the `Display` page off 1000 and onto its
honest exact-name 900.

**Amended at v0.2.** The original table listed "Full-name prefix (900)" and
"First-word-boundary prefix (800)" as separate rungs, but they describe the same
set — a needle prefixing the first word necessarily prefixes the title — so tier
800 was unreachable and the ladder had five rungs, not six. The repair promotes an
*exact* name match to its own rung, which is unambiguously right: typing `code`
must not surface "Code Composer Studio" above "Code". Both original worked
examples keep their meaning and their relative order.

Note that `code` reaches Visual Studio Code on the **later-word** rung, not the
executable rung, because "Code" is a word of the display name. The executable rung
earns its place on the apps whose binary is named nothing like the product —
`devenv` for Visual Studio, `wt` for Windows Terminal, `subl` for Sublime Text.

No fuzzy subsequence in V1 — deferred by decision, see `docs/plans/post-v1.md`.
`EntryKind` ordering is applied after scoring: **Apps always sort above
documents**, never interleaved by raw score. The tier ladder is
`App = System < SystemTask < Folder < File < Recent < Clip` (Calc wins outright).

**Calc winning outright is load-bearing and has a price, paid since v0.4.** An
expression is unambiguous, so nobody typing `17*23` meant an application — but a
*bare number* is not unambiguous, and `2022` is both a number and half of "Adobe
Photoshop 2022". The tier means the answer takes the row and Enter with it. The
calculator's own conservatism is therefore the only thing standing between the
tier and a launcher that argues with you: `sources/calc/mod.rs` is where that
policy lives, and `docs/tbd/v0.4.md` §1 carries the collision it does not solve.

The calculator also carries a **Policy** — `automatic` or `explicit` — deciding
whether it may answer a Bangless query at all, or only input prefixed with `=`.
It is not a Mode: `CONTEXT.md` reserves that word for what a Bang selects. Held
in Rust because it is enforced on the keystroke path, pushed from the frontend
until `settings.db` exists at v0.6.

**A tier is not the only lever.** `EntryKind::weight` multiplies the
Frecency-lifted score — App 1.0, System and SystemTask 0.8 — so two Kinds can
share a tier and still not compete evenly. This is PowerToys' plugin-weight model
rather than a hard gate; §10 of `docs/tbd/v0.3.md` carries the live report that
produced the number and what each candidate value costs.

**Curated settings pages share the App tier; control-panel tasks do not.** A
settings page (v0.3 task 8) is a launch destination, not a document — you type
`bluetooth` to go to it — so it competes with apps on match quality and Frecency
rather than one gating the other. The 198 control-panel tasks are a different
animal: long sentences ("Change the way currency is displayed") that can only ever
match at word-prefix, and having 198 of them in the App tier for every query was
noise. They are `EntryKind::SystemTask`, below every app and above documents.

The shared tier corrects the task-8 plan's "ranked below applications":
observed live, that pinned `DisplaySwitch` (a bare `System32` exe matching only by
its filename stem, 650) above the `Display` settings page (exact name, 900), and
no amount of use could move it, because Frecency reorders within a tier and never
across one. The "apps above documents" rule is about incidental files surfaced in
the default view, not a destination the user asked for by name. Their ids are
`ms-settings:<page>` and `system:<task name>`, stable across reinstalls. A settings
page launches through its `ms-settings:` URI; a control-panel task has no
reparseable name (it is positional in the shell namespace), so its absolute PIDL
is captured at enumeration and launched by `SEE_MASK_IDLIST` — `LaunchTarget::Uri`
and `LaunchTarget::ShellItem` respectively.

**The weight was added on top, from a second live report.** Typing `dis` for
Discord selected the Display page: both are seven letters matching a three-letter
prefix, so both scored 796.5, and a 0.3% Frecency gap decided the top row — a coin
flip, not a ranking. A shared tier says "neither gates the other"; it does not say
"a hair decides". The 0.8 weight makes a system entry earn the top row decisively
or not at all. Because the Frecency lift saturates at ×1.6, 0.8 means a settings
page outranks only an application never launched from Takyon; that is close to the
tier this phase removed, and was chosen knowing so. It becomes a setting at v0.6.

The App-vs-document ordering itself is a **still-open refinement**: "apps above
documents" is really a default-view rule, and even there a 1:1 app/document match
should be resolved by the query rather than by a blind tier. Deferred until
recents and files exist to test it against (v0.7); recorded in `docs/tbd/v0.3.md`.

### Frecency

`weight = Σ 0.5^(age_days / 30)` — a 30-day half-life. Stored decayed with a
`decayed_at` stamp and lazily re-decayed on read, so there is no background job
and no clock-skew problem. Negative elapsed time is clamped: a clock that jumps
backwards would otherwise raise `0.5` to a negative power and *grow* a score.

**Composition, added at v0.3.** The match score is multiplied by
`1 + 0.6 · w/(w+1)`, saturating rather than linear — so one launch is worth a
great deal, the hundredth almost nothing, and the lift is bounded at 1.6×. That
bound is the product rule stated as a number: a well-used Entry climbs roughly
one rung, so a much-used acronym match can pass an exact-name match nobody has
ever chosen, and nothing can climb further than that. Weight zero returns the
base score untouched, which keeps a cold install ranked purely on matching.

Both constants are guesses to be tuned from real use, exactly like the
half-life. **[TBC-0009](./docs/tbc/0009-frecency-constants.md)** carries the
triggers, what to read out of `frecency.db` given there is no telemetry, and
the catch that a stored score already has the old half-life baked into it.

**Frecency is applied in the pipeline, never inside a Source.** Sources return
match quality alone, which keeps the ladder testable without a usage database and
keeps ADR-0009's "nothing UI-aware in a Source" honest. The consequence is that a
Source must hand up more than the Palette shows — `SOURCE_SHORTLIST` (64) against
`MAX_ENTRIES` (12) — or a much-used Entry is discarded one step before its lift.

Only launches are recorded. Open and Run as administrator write usage; Open file
location and Copy path do not, because those are things people do while looking
*for* something rather than choosing it.

---

## 4. Storage

All under `%LOCALAPPDATA%\v3sper\launcher\` (ADR-0011 — the slug is fixed and
independent of the display name). One SQLite database per concern, WAL mode.

```sql
-- settings.db
CREATE TABLE settings   (key TEXT PRIMARY KEY, value TEXT NOT NULL);  -- JSON values, typed in Rust
CREATE TABLE aliases    (alias TEXT PRIMARY KEY, target TEXT NOT NULL);
CREATE TABLE roots      (path TEXT PRIMARY KEY, enabled INTEGER NOT NULL);
CREATE TABLE exclusions (pattern TEXT PRIMARY KEY);
CREATE TABLE blocklist  (exe TEXT PRIMARY KEY);   -- clipboard capture exclusions
-- settings keys in use at v0.5: clips.retention (see Retention), clips.bang ("1"/"0")

-- frecency.db
CREATE TABLE usage (
  entry_id   TEXT PRIMARY KEY,
  kind       TEXT    NOT NULL,
  count      INTEGER NOT NULL,
  last_used  INTEGER NOT NULL,
  score      REAL    NOT NULL,
  decayed_at INTEGER NOT NULL
);

-- clips.db  — PRAGMA secure_delete = ON
CREATE TABLE clips (
  id         INTEGER PRIMARY KEY,
  created_at INTEGER NOT NULL,
  kind       TEXT    NOT NULL,
  source_exe TEXT,            -- plaintext, and a known metadata leak (ADR-0008)
  len        INTEGER NOT NULL,
  nonce      BLOB    NOT NULL,
  ciphertext BLOB    NOT NULL
);
```

**Clipboard encryption** (ADR-0006, ADR-0008): AES-256-GCM per row with a
per-row nonce. The 32-byte key lives in `creds\clip.key.dpapi`, wrapped with
Windows DPAPI and bound to the user account. Field-level, not SQLCipher.

**`settings` and `blocklist` opened at v0.5, not v0.6.** The retention sweep runs
at startup, before any window exists to push a preference in, so the chosen
window has to live somewhere Rust reads first. A default held only in the
frontend would sweep over a chosen `forever` on every launch. `prefs.rs` owns the
`settings` table and holds one key, `clips.retention`; v0.6 extends it rather
than creating it.

**Retention sweeps must actually destroy data.** `DELETE` alone leaves
recoverable ciphertext in free pages and live content in the WAL. Every sweep
runs with `PRAGMA secure_delete = ON` and follows with
`PRAGMA wal_checkpoint(TRUNCATE)`. "The row is deleted" and "the secret is gone"
are different claims and this feature needs the second one.

---

## 5. The file index (v0.7)

Unelevated scoped directory walk plus `ReadDirectoryChangesW` watchers, no
service and no raw volume access (ADR-0007, superseding ADR-0004).

**On-disk format** — `index\<generation>.tkx`, memory-mapped, never parsed:

```
header      magic "TKX1" · format_version · generation · root_count · entry_count
arena       UTF-8 path strings, NUL-separated
entries     [ name_off:u32, parent:u32, flags:u8 ]
postings    lowercased-name trigram → sorted entry-id list
```

Query: intersect the postings for the query's trigrams, then verify each
candidate with the real matcher from §3. Queries shorter than three characters
skip the postings and scan the (small) recent set instead.

`format_version` bumps force a full rebuild. Reading is `mmap` + offset
arithmetic, so startup cost is a page fault, not a parse.

**Watcher overflow is the correctness problem, not an edge case.** A
`ReadDirectoryChangesW` buffer overflow returns `ERROR_NOTIFY_ENUM_DIR`, meaning
events were dropped. On that signal: bump the generation, mark the affected
subtree stale, rescan just that subtree. **Never serve a known-stale index
silently** — `IndexStatus::Stale` must surface in the UI. An index that quietly
misses files is worse than no index, because the user learns not to trust it.

Default roots and exclusions are a *product* decision with a settings UI, not a
constant — see TBC-0005, which is the least-evidenced call in the whole design.

**Built at v0.7.** Three amendments, all measured rather than argued:

**Short queries take a linear prefix scan, not the recent set.** This section
sent queries under three characters to "the (small) recent set", assuming a full
scan was too slow. It is 646 µs over 26,846 names — 3% of the budget — and it
finds a two-letter folder the recent set has never seen. Prefix, never substring:
two letters matched anywhere would return most of the index.

**Watcher deltas land in an overlay, not in the mapped file.** Rewriting 2.5 MB
per event would put a disk write on the path a `git checkout` fires thousands of
times. `index/overlay.rs` holds additions and deletions in memory; a query reads
mapped hits minus deletions plus additions, and a rebuild folds it back in and
empties it. A deletion hides everything beneath it, because Windows sends one
event for a directory and none for its contents.

**`Stale` is set before the rescan, never after.** A rescan that cannot run must
leave the index saying so rather than reporting Ready — which is the difference
between this section's rule and a version of it that only holds when nothing goes
wrong.

The `FileIndex` trait has two implementors: the walk, and Windows Search behind a
settings toggle that is **off by default**. Its coverage cannot be relied on even
where the drive is nominally indexed — TBC-0005's amendment carries the
measurement — so it is asked after the local index has answered, only when that
answer came up short, and never on the Bangless path.

---

## 6. Icons

Lazy extraction into a single memory-mapped `icons.bin`, keyed by target path +
mtime. Extraction runs off the UI thread; a missing icon renders a placeholder
and never blocks a row.

**Pre-warm the top ~50 Entries by Frecency after login**, so icon pop-in only
ever happens for things you rarely open — where you aren't looking closely
anyway.

**Extraction is `IShellItemImageFactory::GetImage`, not `SHGetFileInfo`** (added
at v0.2). One API covers a Win32 executable, a `.lnk` and a UWP package alike, so
the icon path does not fork by application kind — a packaged app is named to it as
`shell:AppsFolder\<aumid>`, the same string used to launch one. Pass
`SIIGBF_ICONONLY`: without it the shell returns a *thumbnail*, which for a
shortcut to a document is a picture of the document.

### How the bytes reach the webview (added at v0.2)

`IconRef` is an opaque key, and §2's "an offset into the icon blob" is an
implementation detail of the Rust side. WebView2 cannot read a mapped file — it
fetches URLs — so the transport is a **custom URI scheme**: Rust registers
`takyon-icon` with `register_asynchronous_uri_scheme_protocol`, the query response
carries one short key per row, and `api.ts` turns it into a URL with
`convertFileSrc`.

The alternative, base64 data URIs inside the query response, was rejected on the
numbers: twelve rows of 64px PNG is roughly 35–65 KB of base64 through the IPC
serialiser **on every keystroke**, measured against the 30 ms first-Entry budget,
and no row can paint until its icon has serialised. With a scheme the rows paint
first, each icon arrives on its own fetch, and WebView2 caches it by URL — so the
second time a query is typed the icons are already decoded in the renderer.

Two details are load-bearing:

- **Asynchronous**, not the synchronous handler. A cache miss extracts from the
  shell, and the synchronous form runs on a thread WebView2 needs — a slow
  extraction there stalls page rendering.
- The key contains the source file's **mtime**, so a given URL's bytes can never
  change, and the response is served `immutable` with a one-year max-age.

The scheme name appears in three places that cannot see each other — `icons.rs`,
the CSP in `tauri.conf.json`, and `api.ts` — and a mismatch shows up only as "no
icons, ever", with nothing in any log. A Rust test asserts all three agree.

---

## 7. Window and process model

Per ADR-0003, one Palette window is created at startup and hidden, never
destroyed. On hide, release the working set (`SetProcessWorkingSetSize(-1, -1)`);
on show, do no allocation and no window creation.

**The trim walks the whole process tree, not this process** (v0.1). Trimming only
the Rust host would release a few megabytes: essentially all the resident memory
this ADR is trading away lives in WebView2's browser, renderer and GPU processes.
Those are *descendants* rather than children — the renderer's parent is the
browser process, not us — so the walk is recursive, from one process-table
snapshot, with a visited set because Windows recycles pids and a table can contain
a cycle. It runs on a background thread so hide stays instant.

- `tauri-plugin-global-shortcut` — `Alt+Space` default, rebindable, and a taken
  hotkey must be **reported**, not silently swallowed.
- `tauri-plugin-single-instance` — required *because of* autostart.
- `tauri-plugin-autostart` — on by default via first-run prompt. **Never
  registered in dev builds** (`#[cfg(not(debug_assertions))]` plus
  `import.meta.env.DEV` on the switch): a debug registration writes a `Run` key
  pointing at `target\debug\` that survives uninstalling the real app. Autostart
  state is read from the OS via `isEnabled()`, never mirrored into settings.
- **UIAccess helper.** A separate signed `uiAccess="true"` executable installed to
  a trusted location, which the main unelevated process asks to bring the Palette
  to the foreground. Without it the Palette will not appear over an elevated
  terminal. Dev builds run without it and accept that limitation.
  The protocol is one named pipe, `\\.\pipe\com.v3sper.launcher.uiaccess`,
  carrying eight bytes: an `HWND`. The helper acts on it **only if that window
  belongs to the process that launched the helper** — that ownership check is the
  whole authorisation model, and it caps the damage from the pipe's permissive
  default DACL at "foregrounds Takyon's own Palette". The helper exits with its
  parent, so a privileged listener never outlives the app. The request is made on
  a background thread and only after a cheap `GetForegroundWindow` check shows the
  ordinary path failed, so a normal show never touches the pipe. Full reasoning
  and the signing requirements: `docs/plans/uiaccess-signing.md`.
- **The `Run` value is named `com.v3sper.launcher`**, via
  `tauri-plugin-autostart`'s `Builder::app_name()`. Without that override the
  plugin keys it off `productName`, i.e. "Takyon" — which is exactly the coupling
  ADR-0011 exists to prevent.

Deferred init: the hotkey is live within ~50 ms of launch; index, icons and
databases open afterward.

---

## 8. The IPC contract

`api.ts` is the only file that calls `invoke()`. Every command in one reviewable
place — which is also what keeps the ADR-0002 guarantee checkable.

```ts
export const query       = (q: string, seq: number) => invoke<QueryResult>("query", { q, seq });
export const activate    = (entryId: string, actionId: string) => invoke<void>("activate", { entryId, actionId });
export const actionsFor  = (entryId: string) => invoke<Action[]>("actions_for", { entryId });
export const indexStatus = () => invoke<IndexStatus>("index_status");
export const dismiss     = () => invoke<void>("dismiss");
```

As of v0.1 the implemented surface is smaller, and deliberately so — a declared
type with no Rust behind it is a fixture that can never drift *into* correctness:

```ts
export const dismiss          = () => invoke<void>("dismiss");
export const openSettings     = () => invoke<void>("open_settings");
export const hotkeyStatus     = () => invoke<HotkeyStatus>("hotkey_status");
export const reportFirstPixel = (showId: number) => invoke<void>("report_first_pixel", { showId });
```

v0.2 adds the query surface, plus two commands that exist only because the
**native window** has a size the webview cannot see:

```ts
export const query           = (q: string, seq: number) => invoke<QueryResult>("query", { q, seq });
export const actionsFor      = (entryId: string) => invoke<Action[]>("actions_for", { entryId });
export const activate        = (entryId: string, actionId: string) => invoke<void>("activate", { entryId, actionId });
export const setActionMenu   = (actions: number | null) => invoke<void>("set_action_menu", { actions });
export const setBannerHeight = (height: number) => invoke<void>("set_banner_height", { height });
```

The window is content-sized (TBC-0006) and Rust resizes it inside `query`, from a
row count it already has. The last two exist because two pieces of content are
*not* rows and Rust cannot measure either: the `Ctrl+K` menu, which is taller than
a one-row Palette and would otherwise be clipped by the window's bottom edge; and
the hotkey-failure banner, which is wrapping text whose height the layout engine
decides from the font, the DPI and the window width. A constant reserved for the
banner on the Rust side was 16px short at 150% scaling, and the flex column took
the difference out of the Entry list. **The side that laid it out is the side that
reports it.**

Autostart is not a command here at all: it goes straight to the plugin, because
the OS owns that state and mirroring it would guarantee drift.

v0.7 adds the file surface. `file_index_status` is spelled `index_status` in the
sketch above; the longer name won because there is more than one index in this
process and "status" alone stopped being unambiguous once applications had one:

```ts
export const fileIndexStatus  = () => invoke<FileIndexReport>("file_index_status");
export const setFilesBangless = (on: boolean) => invoke<void>("set_files_bangless", { on });
export const setFilesFallback = (on: boolean) => invoke<void>("set_files_fallback", { on });
export const setFilesRoots    = (roots: string[], excludes: string[]) => invoke<void>("set_files_roots", { roots, excludes });
export const openedCount      = () => invoke<number>("opened_count");
export const clearOpened      = () => invoke<number>("clear_opened");
```

**`file_index_status` is not a field on `QueryResult`.** The state changes on the
walk's schedule rather than the user's, so riding the keystroke path would ship
the same three words on every keypress. It is asked when `!e` is typed and
re-asked while the index is not Ready — which is the only time it has anything to
say.

Types live in `packages/shared` and mirror the Rust structs. **Contract tests
assert that Rust's serialised output matches these types** — that is the one
test that catches fixture drift, which is the silent failure mode of the mocked
visual layer (TBC-0007).

---

## 9. Bang parsing

```
line := bang? rest
bang := '!' ident (WS | EOL)
```

Position 0 only. A Bang consumes the whole line — the rest is that Mode's raw
query, never a ranked search (ADR-0002 depends on this being trivially
checkable). No chaining in V1.

`!` alone opens the picker. **Unknown Bang falls through to Bangless**, treating
the line literally and showing a hint row — provisional, and one of the open
questions in `docs/plans/bang-registry.md`, which is where the Bang design
resumes before v0.9.

**A Bang is never the only door** (v0.5). Clipboard history is reachable by
typing its name, through a `Command` Entry, and `!v` is a toggleable shortcut
over the same surface. A feature whose only entry point is a Bang is a feature
nobody finds: `docs/plans/bang-registry.md` already says the `!` picker is the
only discovery mechanism, and it is not built. New Modes should assume the same
shape — a Command that opens them, and a Bang for people who know it.

**Built at v0.5, not v0.9** — `!v` needed it. `bang.rs` implements the grammar
above and registers one Bang; `query.rs` routes on it before anything else,
which is what keeps ADR-0002 checkable by reading one function. Not built: the
`!` picker (a bare `!` currently falls through), the hint row, and user-defined
Bangs. Those stay parked with the registry.

**`!e` joined at v0.7**, and it is the second Bang rather than the first Mode with
a toggle: `clips.bang` can turn `!v` off because the Clipboard History command is
another door, and file search has no such command, so turning `!e` off would
remove the feature rather than an accelerator. `files.bangless` governs whether
file Entries appear in *Bangless* results — the reverse question — and is off by
default. A Bang with no query is its own state here for the first time: `!e`
alone lists what Takyon has opened rather than showing nothing, which answers one
of the registry's parked questions for one Bang without settling it for all.

---

## 10. Performance

| Metric | Budget |
|---|---|
| Hotkey → first pixel | < 50 ms |
| Hotkey → first Entry, Bangless | < 30 ms |
| Idle RSS, warm and trimmed | < 150 MB |
| Login → hotkey responsive | < 500 ms |
| `!e` p95 | < 20 ms |

`bun run bench` measures all of them and **a regression is a failing test**. It
must include a first-show measurement **after 30+ minutes idle** — a benchmark
run in a tight loop will completely miss the case where Windows has reclaimed the
trimmed working set, which is exactly the case a real user hits.

**What the harness measures, stated rather than implied.** Both ends of every span
are stamped in Rust, on one clock (`src-tauri/src/bench.rs`); the frontend only
echoes an id back after a double `requestAnimationFrame`. So "hotkey to first
pixel" is *hotkey handler entry to the IPC call following the frame the renderer
committed*: it **includes** one IPC hop and **excludes** DWM's final present.
Reconciling a `performance.now()` with an `Instant` would have produced a
plausible number with no defined meaning, which is the usual way a latency claim
becomes fiction. The residual gap is a constant, closed once by a 240fps capture
and recorded in TBC-0002.

Memory is summed across the **whole process tree** for the same reason the trim
walks it: a reading that sees only the main process reports roughly the Rust
binary and quietly claims the budget was met.

The v0.1 numbers get written into TBC-0002 as the first real evidence for or
against the warm-window model. That is what v0.1 is *for*.

---

## 11. Testing

Three layers, because no single one can verify a launcher. Use the `/tdd` skill.

1. **Rust unit tests** — matching tiers, Frecency decay, the Stability lock,
   index round-trips, watcher-overflow handling. Pure logic, no UI, no Tauri.
2. **Visual regression** — the React UI in the plain Vite dev server with `api.ts`
   mocked, screenshotted by Playwright against fixtures. Requires that no
   component calls `invoke()` directly. (Playwright as a dev dependency is
   unrelated to ADR-0005, which forbids *shipping* a browser engine.)
3. **Manual script per phase** — hotkey, focus-loss dismissal, tray,
   multi-monitor, UIAccess over an elevated window. Genuinely not automatable
   cheaply; write the script as part of the phase.

A **debug-only flag must show the Palette without stealing focus**, or
dismiss-on-focus-loss destroys the window every time you try to inspect it.

---

## 12. Deliberately not specified here

- **~~`!c` and the Chat Surface internals.~~** Settled and built at v0.8: a fresh
  process per Turn resumed by session id, the Scratch directory, and tools off
  inline. `docs/plans/v0.8-agents.md` carries the reasoning, ADR-0017 the rule
  that shapes it. The **permission UI** is what remains (`docs/tbd/v0.8.md` §1).
- **The Bang registry beyond V1's four.** See `docs/plans/bang-registry.md`.
- **A plugin API.** Designing one before three real plugins exist is guesswork.
- **The colour palette.** The mark is locked; colour is not (`docs/brand.md`).
  Needed by v0.6, not before.
