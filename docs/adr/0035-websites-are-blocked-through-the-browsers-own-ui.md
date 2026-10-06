---
status: accepted
---

# Websites are blocked through the browser's own UI

A Focus session blocks a website by reading the active tab's address bar through
UI Automation and, on a match, navigating that tab to a local block page. Takyon
never filters the network to do it.

## The problem

v0.16 adds Focus sessions (`docs/plans/v0.16-focus.md`), and a Block list can
name websites. Windows offers several places to stop a page loading, and each
one collides with a rule this project already holds:

- **No elevation, no service** (ADR-0007). The `hosts` file, WFP filters and a
  DNS sinkhole all need administrator rights at the moment they change, which is
  the start of every session.
- **Only Bangs leave the machine** (`CLAUDE.md`, ADR-0002). Anything that sits in
  the network path, such as a local proxy, puts Takyon between the user and every
  request they make, and a crash there leaves the user offline.
- **The user's own proxy is respected** (ADR-0019). A per-user proxy or PAC
  setting written by Takyon replaces the one the user configured.

Raycast Focus blocks sites with no extension in Safari, Chromium and Firefox
browsers, redirects them to a block page, and restores the tabs afterwards. Its
mechanism is unpublished. On macOS it is almost certainly AppleScript and
Accessibility, which is the same idea as this decision: drive the browser's own
UI.

## The decision

**UI Automation on the address bar, active tab only.**

- A supported browser is recognised by image name: Chrome, Edge, Brave, Vivaldi,
  Opera and Arc on Chromium; Firefox, Zen, LibreWolf and Waterfox on Gecko.
- While a session with sites runs and a supported browser is in the foreground,
  the `focus` thread polls that window's address bar every 500 ms through its
  `ValuePattern`. The displayed value is often scheme-less and trimmed, so it is
  normalised to a host before matching. A domain matches itself and its
  subdomains.
- A match sets the address bar's value to
  `file:///<install>/focus/blocked.html?u=<original>&end=<unix-ms>` and sends
  Enter, after re-checking that the browser is still in front and its address bar
  has focus. The block page is a local file that makes no request.
- The block page's title is always `Blocked · Takyon`. At session end Takyon
  walks each browser's tab strip for tab items with that name, selects each one,
  reads `u` from its address bar, and navigates back. As a fallback, the page's
  own timer returns to `u` once `end` has passed.
- Only the active tab is read. A blocked background tab is caught when it is
  activated, because activation changes the address bar. This avoids switching
  tabs under the user to inspect them.
- A browser Takyon cannot read is refused in words on the start form: "Can't
  block sites in <browser>". That covers unsupported browsers and elevated ones,
  because UIPI blocks UI Automation into a higher-integrity process.

Bindings come from the `windows` crate Takyon already uses (ADR-0019), with the
`Win32_UI_Accessibility` feature added. No new crate.

## Considered options

| Option | Elevation | Why not |
|---|---|---|
| Browser extension + native messaging | none | A per-browser install the user has to do, store review for each, and a second codebase in a second language. The most reliable option, so it is **the first fallback** |
| `hosts` file | admin | Needs an elevated helper on every session start (amends ADR-0007). Browser DNS caches delay it, and some secure-DNS configurations may bypass it |
| Per-user proxy or PAC (HKCU) | none | A crashed Takyon leaves the user offline. Collides with ADR-0019. Only sees hosts, via `CONNECT`, never paths |
| WFP filter or DNS sinkhole | admin or service | Needs Locked's service (ADR-0036) |

## Consequences

- **Browsers change their UI Automation trees.** Chromium's address bar is found
  by control type inside the toolbar, never by its localised name. Gecko's is
  found by `AutomationId` `urlbar-input`. A live `--ignored` test drives each
  installed browser, and CI's daily job runs it, so an update that breaks a
  lookup is noticed within a day, the same arrangement `ddg.rs` has.
- **There is a visible flash.** The blocked page starts loading before the
  500 ms poll catches it, so the user can see it for up to half a second. Raycast
  has the same window. Shorter polling costs CPU in the browser's accessibility
  tree on every tick.
- **Only hosts are blocked, not paths.** `youtube.com/watch` and
  `youtube.com/feed` are one site. Path rules would need the full URL, which the
  address bar does not always show. They are left for a later phase if wanted.
- **Every browser fork is an entry in a table.** A browser not on the list is
  refused rather than half-supported.
- **macOS reimplements this behind the same seam** with AppleScript and
  Accessibility (`v0.16-focus.md` § macOS). The decision, which is to drive the
  browser's own UI and never the network, carries over unchanged.
