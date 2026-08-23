# Post-V1 backlog

Things deliberately cut from V1. Each entry: what, why it was deferred, and what
would have to be true to pull it forward.

## Revisit the warm/cold process model, with numbers
V1 ships "always warm, aggressively trimmed on hide": the WebView2 instance stays
alive so the Palette shows instantly, and its working set is released while hidden.

**Why revisit:** it was chosen on reasoning, not measurement. The alternatives —
always cold, warm-on-modifier, warm only during active hours — were never
benchmarked against it.

**What to produce:** a table of hotkey-to-first-pixel, hotkey-to-first-Entry, cold
boot to ready, and idle RSS, measured for each strategy on the same machine, plus
the numbers from the week-one spike for comparison. Decide from the table, and
record the outcome as an ADR.

## Analytics and crash reporting (PostHog)
Opt-in, default-off product analytics and crash reporting.

**Why deferred:** V1 sends nothing anywhere, so that ADR-0002's guarantee is a
checkable fact rather than a claim. Shipping telemetry alongside the promise, in
the same release, would undermine both.

**Pull forward when:** V1 is in other people's hands and the absence of crash
visibility is actually costing us bugs.

**Constraints (from ADR-0010, non-negotiable):** opt-in and default off; never any
query content — not Bangless queries, `!s` searches, `!c` prompts, file paths,
clipboard content, or names of launched applications; and its own ADR written when
the shape is real.

## Full theming
V1 ships follow-system appearance plus a manual light/dark override. Full theming
means user-definable palettes, a theme picker, and shareable theme files.

**Why deferred:** custom theming only pays off if there is a community to share
themes, which is downstream of the still-open open-source question. The whole
visible surface is one input and ten rows.

**Pull forward when:** the open-source decision lands on "open", or users ask.

**Reference:** t3code was suggested as a model for how to structure this. Its
top-level `packages/` has no theming package — anything relevant is inside
`apps/`. Worth reading properly at the time rather than now; that repo moves fast
and notes taken today will be stale.

## Bang chaining
Chaining one bang into another (e.g. `!s <query> | !c summarise this`).

**Why deferred:** the parser and the result-passing contract are both simpler if a
line has exactly one mode. Chaining also forces a decision about intermediate
result formats that we can't make well before we've used single bangs daily.

**Pull forward when:** we find ourselves manually copying a `!s` result into a `!c`
prompt more than occasionally.

## Headless-browser search fallback
Driving a headless browser for pages that only render content via JavaScript.

**Why deferred:** bundling Chromium (~300 MB, 300 ms–2 s launch) contradicts the
idle-RAM and startup goals, and SERP scraping invites bot-detection maintenance and
ToS problems. V1 uses a search API for the URL list and plain HTTP + Readability
extraction for page content.

**Pull forward when:** extraction failure rate on real queries is high enough to
hurt answer quality, and the failures are concentrated in JS-only pages.

## macOS support
**Why deferred:** Windows-first is the wedge. Every platform-specific subsystem
(file index, app enumeration, global hotkey, clipboard) needs a second
implementation, so the abstraction boundaries matter more than the second backend.

**Pull forward when:** V1 is stable on Windows and the platform seams have proven
they hold.
