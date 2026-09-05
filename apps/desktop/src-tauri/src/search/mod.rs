//! Web search: the `!s` Mode (v0.9, ADR-0005).
//!
//! Two steps behind one Bang. A `SearchProvider` returns Hits — title, URL,
//! description — and `fetch` reads those pages over WinHTTP so `extract` can
//! reduce them to text. `synth` hands that text to whichever Agent `!c` would
//! have asked, so Takyon still holds no LLM account of its own (ADR-0017).
//!
//! Everything here is on the far side of a Bang. Nothing in this module may be
//! reachable from a Bangless line, which is ADR-0002 and is checkable by reading
//! `query.rs`: `Route::Web` is the only caller.

pub mod brave;
pub mod browser;
pub mod extract;
pub mod fetch;
pub mod ipc;
pub mod key;
pub mod synth;

use serde::Serialize;

/// The service named in the Palette row. One provider today (ADR-0005); the
/// trait is what makes TBC-0004's alternatives a one-file change.
pub const PROVIDER_LABEL: &str = "Brave Search";

/// How many Hits are asked for, and at most how many pages are read.
///
/// Ten is Brave's own page size. Reading more costs a fetch each and the
/// summariser reads less of every page for it.
pub const MAX_HITS: usize = 10;

/// One search result, before its page has been read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub title: String,
    pub url: String,
    /// The provider's own snippet, with its highlight markup stripped.
    pub description: String,
}

/// What went wrong, in words the Palette shows as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchError {
    /// No key stored. Its own case because the fix is a Settings page, not a
    /// retry, and the Palette says so rather than showing a failure.
    NoKey,
    /// The provider refused the key. Distinct from `NoKey`: a wrong key looks
    /// like a working one until it is used.
    BadKey,
    /// Rate limited. Free tier is per second, so this is expected, not broken.
    RateLimited,
    /// Anything else, already in the provider's or the OS's words.
    Failed(String),
}

impl SearchError {
    /// The sentence shown in the Palette. Ends in what to do where that is known.
    pub fn message(&self) -> String {
        match self {
            SearchError::NoKey => {
                format!("No {PROVIDER_LABEL} key. Add one in Settings → Web search.")
            }
            SearchError::BadKey => {
                format!("{PROVIDER_LABEL} refused the key. Check it in Settings → Web search.")
            }
            SearchError::RateLimited => {
                format!("{PROVIDER_LABEL} is rate limiting. Wait a moment and ask again.")
            }
            SearchError::Failed(why) => why.clone(),
        }
    }
}

/// One search service, as far as the rest of Takyon is concerned.
///
/// The whole SPI is one call. Fetching, extraction and synthesis are shared and
/// live beside this file, so a second provider is one file plus one line.
pub trait SearchProvider: Send + Sync {
    /// Display name, UI copy only.
    fn label(&self) -> &'static str;

    /// Hits for a query, at most `MAX_HITS`. Blocking: called off the main
    /// thread, from the Turn's own thread.
    fn search(&self, query: &str, key: &str) -> Result<Vec<Hit>, SearchError>;
}

/// The provider this build ships. One today, by name rather than by list —
/// a second one arrives with a preference, and TBC-0004 owns that decision.
pub fn provider() -> Box<dyn SearchProvider> {
    Box::new(brave::BraveProvider)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A missing key and a wrong key are different sentences, because they have
    /// different fixes and the Palette shows the sentence verbatim.
    #[test]
    fn v0_9_a_missing_key_and_a_refused_key_read_differently() {
        assert_ne!(SearchError::NoKey.message(), SearchError::BadKey.message());
        assert!(SearchError::NoKey.message().contains("Settings"));
        assert!(SearchError::BadKey.message().contains("Settings"));
    }

    /// A provider's own words survive to the surface rather than being replaced
    /// by a generic failure.
    #[test]
    fn v0_9_an_other_failure_is_shown_in_the_words_it_arrived_in() {
        let error = SearchError::Failed("The server name could not be resolved.".into());
        assert_eq!(error.message(), "The server name could not be resolved.");
    }
}
