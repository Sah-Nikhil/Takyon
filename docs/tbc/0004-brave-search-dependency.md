---
status: resolved
pairs-with: ADR-0005
resolved-by: ADR-0021
---

> **Resolved by [ADR-0021](../adr/0021-duckduckgo-keyless-with-exa-when-keyed.md).**
> The first trigger below fired: Brave's free tier now requires a card on file.
> `!s` moved to DuckDuckGo by default, with Exa when a key is stored, which is a
> blend of two rows in the Alternatives table — "Exa / Tavily" for the keyed half
> and something the table did not list for the keyless one. The verdict's own
> advice held: the `SearchProvider` trait made it a two-file change, and the day
> it cost was spent on an HTML parser rather than on plumbing.
> Kept for the reasoning, and because the same exposure now applies to Exa.

# TBC-0004 — Brave Search API as the `!s` retrieval layer

## The bet

`!s` gets its URL list from the Brave Search API and reads pages itself over plain
HTTP. The assumption is that a **single third-party search dependency is
acceptable** because the free tier covers personal use, the JSON shape is stable,
and the expensive half of the work (reading and extracting pages) is ours and free.

The exposure is that this is the one hard external dependency in the product, on a
commercial API whose pricing, terms and availability we don't control — and it
sits behind a headline feature.

## How we'd know we were wrong

- Free-tier limits change, or the rate limit (queries per second) makes `!s` feel
  slow at normal usage.
- Result quality is visibly worse than Google for the queries we actually run.
- Pricing at distribution scale becomes untenable — relevant only if Takyon
  ships to other people, where **every user needs their own key**, which is itself
  an onboarding problem worth watching independently.
- Readability-style extraction fails often enough on real results that answers
  degrade — this is the trigger for the deferred headless fallback, not for
  changing providers.

## Alternatives

| Option | Improvement if we switch | Added complexity | Switching cost |
|---|---|---|---|
| **Exa / Tavily** | Built for LLM retrieval: returns cleaned page content directly, so our extraction step largely disappears | Low — arguably less code than today | **1–2 days** behind a `SearchProvider` trait. More expensive per query, no meaningful free tier |
| **SerpAPI / Google CSE** | Google-quality results | Low code | **1–2 days.** Google CSE has a small free tier then costs per query; SerpAPI is priced well above Brave |
| **User picks their provider** | Removes the dependency as a product risk; users bring whatever key they have | Medium — several response shapes to normalise, plus settings UI | **3–5 days.** The right answer if this ships to other people |
| **Self-hosted SearXNG** | No key, no cost, no terms | High — the user runs a service, or we ship one | **4–7 days**, and turns a launcher into an app with infrastructure |

## Verdict if triggered

Put a `SearchProvider` trait in from day one — it costs nothing now and makes
every row above a one-to-two day change. If Takyon ships to other people,
move to **user-picks-provider** with Brave as the documented default, since
"bring your own key" is already the model `!c` uses for Claude and consistency is
worth more than convenience here.
