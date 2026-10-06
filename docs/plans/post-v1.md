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

## Focus: Locked strictness
A Focus session the user cannot end early or escape through Task Manager.

**Why deferred:** a lock that holds against the user has to run as someone else,
which means a `LocalSystem` service registered by the installer. That amends
ADR-0007 and needs signing and a security review: more work than the rest of
v0.16 combined. ADR-0036 has the design.

**Pull forward when:** Strict sessions are being escaped through Task Manager
often enough that the user asks for more, and the installer is already signed.

## Focus: scheduled sessions
Focus sessions that start themselves at set times, such as weekdays 9:00 to 12:00
with Social blocked, without anyone opening the Palette. Shown in Settings →
Focus as Coming soon from v0.16.

**Why deferred:** it needs a scheduler running while the Palette is hidden, and
it is a separate feature that changes nothing in v0.16's design.

**Pull forward when:** v0.16 has shipped and people start the same session at
the same time every day.

## Focus: Allow list
Block everything except what is listed. Designed in
[`v0.16-focus.md`](./v0.16-focus.md) § Allow list, including the one schema
choice made in advance.

**Why deferred:** on Windows the always-allowed set (shell, IMEs, accessibility
tools) has to be right or the user loses their own input method.

**Pull forward when:** there is a machine with an IME to verify it on.

## Focus: Agent classification of apps
Offer to sort installed apps that no Focus category covers into categories by
asking the user's Agent.

**Why deferred:** it sends the names of installed apps to an Agent, which is an
outbound request with no Bang behind it. That needs its own ADR against the "only
Bangs leave the machine" promise, and an explicit button rather than anything
automatic.

## Focus: built-in categories through the updater
New built-in Focus categories arriving between releases, inside the v1.0
updater's existing check.

**Why deferred:** there is no updater yet. A separate remote category feed was
rejected because it would be an outbound request with no Bang. Riding on the
updater adds no new network path.

## Focus on macOS
**Why deferred:** Focus is Windows-first, like everything else.
[`v0.16-focus.md`](./v0.16-focus.md) § macOS lists the APIs: `NSWorkspace`
notifications and `NSRunningApplication` for apps, AppleScript and Accessibility
for browsers, IOKit power assertions for idle sleep.

**Pull forward when:** v0.12's port is running.

## macOS support
**Why deferred:** Windows-first is the wedge. Every platform-specific subsystem
(file index, app enumeration, global hotkey, clipboard) needs a second
implementation, so the abstraction boundaries matter more than the second backend.

**Pull forward when:** V1 is stable on Windows and the platform seams have proven
they hold.

**No longer a sketch.** The seams held, the crate compiles for
`aarch64-apple-darwin`, and every architectural decision the port needs is made —
ADR-0026 through ADR-0030, plus TBC-0013 and TBC-0014.
[`docs/plans/v0.12-macos.md`](./v0.12-macos.md) is the build plan, with the row table, the
build order and the traps. Two of its consequences are their own phases:
[`v0.13-os-index.md`](./v0.13-os-index.md) and [`v0.14-clipboard-kinds.md`](./v0.14-clipboard-kinds.md).
[`v0.11-path-hydration.md`](./v0.11-path-hydration.md) is needed by both platforms and can
land first.
