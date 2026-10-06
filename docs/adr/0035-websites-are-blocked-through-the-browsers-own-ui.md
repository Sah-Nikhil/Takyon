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
  `file:///<data>/focus/blocked.html?u=<original>&end=<unix-ms>&g=<goal>` and sends
  Enter, after re-checking that the browser is still in front and its address bar
  has focus. The block page is a local file that makes no request.
- The block page's title is always `Blocked · Takyon`. After a session ends
  early, the next time each browser comes to the front Takyon walks its tab
  strip for tab items with that name, selects each one, reads `u` from its
  address bar, and navigates back; Takyon never raises a browser itself. A
  session that expires needs no walk: each page returns to `u` once `end` has
  passed.
- Only the active tab is read. A blocked background tab is caught when it is
  activated, because activation changes the address bar. This avoids switching
  tabs under the user to inspect them.
- A browser Takyon cannot read is refused in words on the start form: "Can't
  block sites in <browser>". That covers unsupported browsers and elevated ones,
  because UIPI blocks UI Automation into a higher-integrity process.

Bindings come from the `windows` crate Takyon already uses (ADR-0019), with the
`Win32_UI_Accessibility` feature added. No new crate.

## Spike result, 2026-10-06: go

A throwaway Rust probe on `windows` 0.61.3 (the version Takyon pins), run
against the five browsers installed on the dev machine, each on a temporary
profile. Every step passed in all five:

| | Chrome | Edge | Helium | Firefox 157 (Store) | Zen |
|---|---|---|---|---|---|
| Address bar lookup | `ClassName` `OmniboxViewViews` | same | same | `AutomationId` `urlbar-input` | same |
| Displayed value at `https://example.com/spike-a?x=1` | `example.com/spike-a?x=1` | `https://example.com/spike-a?x=1` | `example.com/spike-a?x=1` | `example.com/spike-a?x=1` | `example.com` |
| Displayed value on the block page | `C:/…/blocked.html?u=…` | same | same | `file:///C:/…/blocked.html?u=…` | same |
| Set value + Enter navigates | yes | yes | yes | yes | yes |
| Tab strip lists `Blocked · Takyon` by name | yes | yes | yes | yes | yes |
| `SelectionItemPattern.Select` switches tab | yes | yes | yes | yes | yes |
| Restore from `u` lands on the original URL | yes | yes | yes | yes | yes |

Costs: about 0.1 ms per address-bar read on a cached element, 11 to 29 ms to
find the bar again from the window, and 10 to 28 ms to walk the tab strip. A
500 ms poll is negligible.

What the spike changed in the design:

- **Chromium is found by `ClassName` `OmniboxViewViews`.** Its `AutomationId`
  (`view_1012`, `view_1017`) differs between builds and its name is localised.
- **The displayed value has four shapes**: with a scheme, without one, host
  only, and a bare drive path for `file:`. Host extraction and the `u=` parse
  accept all four.
- **Never act while the address bar has keyboard focus.** A half-typed `youtu`
  reads as the value, with `HasKeyboardFocus` true, in Chromium. Gecko's
  cached element went stale while focused and read `""`. Either way the rule is
  the same: skip the tick when the bar has focus or reads empty, and re-find the
  element on an empty read.
- **Gecko hides the whole toolbar while a window-modal dialog is up.** A fresh
  Zen profile's welcome screen (`zen-welcome-video`), Firefox's "Welcome to
  Firefox" terms dialog and its "Make Firefox your primary browser?" prompt
  each left no address bar in the tree. Nobody can browse past a modal either,
  so a browser with no address bar in its tree is unreadable for that tick and
  nothing more. The live test seeds its throwaway Firefox profile with
  `browser.shell.checkDefaultBrowser=false`; the terms dialog is accepted by
  hand, once per profile, never by Takyon.
- **Gecko's address-bar element goes stale.** Zen threw `0x800401FD` (object
  not connected) on `SetFocus` with an element found a moment earlier. Every
  navigate re-finds the bar, and checks `HasKeyboardFocus` before sending Enter,
  re-finding again if focus moved.
- **Firefox from the Microsoft Store is an MSIX package** under
  `C:\Program Files\WindowsApps\Mozilla.Firefox_…\VFS\…\firefox.exe`. The image
  name is still `firefox.exe`, so detection by image name holds, but any code
  that looks for Firefox by install path misses it.

## Considered options

| Option | Elevation | Why not |
|---|---|---|
| Browser extension + native messaging | none | A per-browser install the user has to do, store review for each, and a second codebase in a second language. The most reliable option, so it is **the first fallback** |
| `hosts` file | admin | Needs an elevated helper on every session start (amends ADR-0007). Browser DNS caches delay it, and some secure-DNS configurations may bypass it |
| Per-user proxy or PAC (HKCU) | none | A crashed Takyon leaves the user offline. Collides with ADR-0019. Only sees hosts, via `CONNECT`, never paths |
| WFP filter or DNS sinkhole | admin or service | Needs Locked's service (ADR-0036) |

## Consequences

- **Browsers change their UI Automation trees.** Chromium's address bar is found
  by `ClassName` `OmniboxViewViews`, never by its localised name or its
  per-build `AutomationId`. Gecko's is found by `AutomationId` `urlbar-input`.
  A live `--ignored` test drives each installed browser, and CI's daily job runs
  it, so an update that breaks a lookup is noticed within a day, the same
  arrangement `ddg.rs` has.
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
