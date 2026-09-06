---
status: accepted
amends: 0005
resolves: TBC-0004
---

# `!s` searches with DuckDuckGo by default, and Exa when a key is stored

Two providers behind the existing `SearchProvider` trait. DuckDuckGo needs no key
and no account, and answers every query on a machine that has not been set up.
Exa needs a key; when one is stored it is asked first, and **any failure falls
through to DuckDuckGo rather than surfacing**. Brave is removed from selection.

This amends ADR-0005, which chose Brave. ADR-0005's actual decision — a search API
plus our own HTTP extraction, never a bundled browser — is unchanged and still
holds. Only the choice of service moves.

## Why Brave went

TBC-0004 was written watching for exactly this and its first trigger fired: Brave's
free tier now requires a card on file. The tier itself still exists at 2,000
queries a month, so this is not a pricing collapse, but a card is a real barrier
for the person building this and a worse one for anyone `!s` ever ships to, where
**every user needs their own key**. ADR-0005's parenthetical "whose free tier
covers personal use" is no longer true as written.

`brave.rs` is kept behind the trait, selected by nothing. It costs nothing to
carry and is the fastest way back if Exa's terms move next.

## Why these two

The question was asked as "what else is free", and the honest answer split in two.

**DuckDuckGo, because it needs nothing.** `html.duckduckgo.com/html/` renders
results without JavaScript, so it comes back through the same WinHTTP stack the
page reads already use. No key, no account, no card, no second engine, and no
change to ADR-0005 or ADR-0019. A live test proves it answers.

**Exa, because it is better when paid for.** It is built for LLM retrieval and
returns page text alongside each result, which is the row in TBC-0004's own table
marked "arguably less code than today". Its free tier needs no card.

**Driving a browser was considered and rejected again.** Not for ADR-0005's stated
reason — that argument was about *bundling* an engine, and Takyon already ships
WebView2, so an offscreen WebView2 driven through `webview2-com` would add no
binaries. What survives is that Google is the only target that needs a browser at
all, its terms forbid automated Search access, its SERP DOM churns, and consent
walls and CAPTCHAs fail at a time nobody chooses. DuckDuckGo's no-JavaScript
endpoint gets the same outcome for a day's work instead of three, with no window
lifecycle and no second process.

## Why failure is the switch, not the key alone

Considered: the key as a deterministic switch (no key means DuckDuckGo, a key
means Exa, an Exa failure is an error), and an explicit picker in Settings.

Chosen: Exa first when keyed, silently retrying on DuckDuckGo on any failure,
including Exa returning nothing. The reason is that `!s` should not have a dead
end. A spent quota, an outage or a mistyped key becomes slightly worse answers
rather than a red row, and there is always something to read.

**The cost is real and is accepted.** A wrong key never announces itself. It
presents as "still working", indefinitely, and nothing in the product says
otherwise. Anyone debugging "why are my answers worse" has to know this rule
exists. It is written on the Settings page for that reason.

The second cost is that one query can reach two services. The Palette's outbound
header names a provider, so `search::search` announces once per provider actually
contacted and the header is repainted when the fallback fires. A search that falls
back therefore emits `searching` twice, and `searchState` treats the second as a
correction rather than a restart. Without that the header claims the question went
somewhere it did not, which is the one thing that surface exists to be exact about.

## Consequences

`SearchError::NoKey` stops being reachable in normal use: a missing key selects
DuckDuckGo before any provider is asked. It survives for the case where the
fallback fails too, and its message names Exa because only a keyed provider can
produce it.

The new maintenance cost is HTML parsing. `ddg.rs` reads `result__a` and
`result__snippet` out of markup meant for a browser, so a class rename at
DuckDuckGo breaks `!s` with no version to pin and no error to read. The live
`#[ignore]` test `v0_10_a_real_keyless_search_returns_coherent_hits` is the
tripwire; the unit tests keep passing against a fixture frozen the day it was
captured, which is exactly the failure to plan for. Run the live tests before a
release.

The one clear win is that v0.9's exit criterion is now reachable on this machine.
It could not be met while it depended on a key nobody had.
