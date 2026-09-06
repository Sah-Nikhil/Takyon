---
status: accepted
amended-by: 0021
---

# `!s` uses a search API plus HTTP extraction, never a bundled browser

> **Amended by [ADR-0021](0021-duckduckgo-keyless-with-exa-when-keyed.md).** The
> decision below — a search API plus our own extraction, never a bundled browser
> — stands unchanged. The *provider* does not: Brave's free tier now requires a
> card, and `!s` searches with DuckDuckGo by default and Exa when a key is
> stored. Read "Brave Search" below as "the search API of the day".

An Arc-Search-style answer needs two distinct things: a list of URLs, and the text
of those pages. We get the URL list from a search API (Brave Search, whose free
tier covers personal use) and the page text by fetching each URL over plain HTTP
in parallel and running a Readability-style extraction. No browser engine is
bundled or driven.

## Considered Options

- **Playwright / headless Chromium**: adds roughly 300 MB of browser binaries and
  300 ms–2 s per launch to an application whose entire thesis is being small and
  fast, while shipping a second engine alongside the WebView2 we already embed.
  Scraping Google or Bing SERPs additionally means permanent maintenance against
  consent walls and bot detection, and violates their terms — a real problem if
  Takyon is ever sold.
- **Forking or borrowing from Helium** (imputnet/helium): it is a full Chromium
  fork, not an embeddable library, so there are no "bits" to lift — only the
  Chromium source tree. Its own code is GPL-3.0, which would force Takyon to
  be GPL and eliminate the proprietary option, which remains undecided.

## Consequences

Only the SERP step needs a paid/keyed service; page reading is free and fully
parallel. A headless fallback for JavaScript-only pages is deliberately deferred —
see `docs/plans/post-v1.md`.
