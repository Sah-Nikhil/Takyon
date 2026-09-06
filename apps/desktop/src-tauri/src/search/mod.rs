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
pub mod ddg;
pub mod exa;
pub mod extract;
pub mod favicon;
pub mod fetch;
pub mod ipc;
pub mod key;
pub mod synth;

use serde::Serialize;

/// Brave's label. Brave is no longer selected by anything (ADR-0021) but the
/// provider is kept behind the trait, so its own strings stay with it.
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
            // Only a keyed provider reaches these three, and Exa is the only
            // one selected (ADR-0021). Reached at all only when the fallback
            // failed too, since `search` swallows them otherwise.
            SearchError::NoKey => {
                format!("No {} key. Add one in Settings → Web search.", exa::LABEL)
            }
            SearchError::BadKey => format!(
                "{} refused the key. Check it in Settings → Web search.",
                exa::LABEL
            ),
            SearchError::RateLimited => format!(
                "{} is rate limiting. Wait a moment and ask again.",
                exa::LABEL
            ),
            SearchError::Failed(why) => why.clone(),
        }
    }
}

/// Text from a provider, with anything that would let it lie about itself gone.
///
/// Tags, HTML entities, control characters and the bidi overrides. U+202E draws
/// a title's own text reversed, which is how a row is made to name a site it did
/// not come from. Detail in `docs/tbd/v0.9.md` §9.
pub fn strip_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut inside = false;
    for ch in decode_entities(text).chars() {
        match ch {
            '<' => inside = true,
            '>' => inside = false,
            _ if inside => {}
            '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}' => {}
            _ if ch.is_control() => {}
            _ => out.push(ch),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The handful of entities a SERP actually uses, plus numeric escapes. Not a
/// general HTML decoder and does not need to be.
fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest[..rest.len().min(12)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix('#')
                .and_then(|n| match n.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => n.parse().ok(),
                })
                .and_then(char::from_u32),
        };
        match decoded {
            Some(ch) => {
                out.push(ch);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// One search service, as far as the rest of Takyon is concerned.
///
/// The whole SPI is one call. Fetching, extraction and synthesis are shared and
/// live beside this file, so a second provider is one file plus one line.
pub trait SearchProvider: Send + Sync {
    /// Display name, UI copy only.
    fn label(&self) -> &'static str;

    /// Whether `search` is worth calling without a stored key.
    fn needs_key(&self) -> bool;

    /// Hits for a query, at most `MAX_HITS`. Blocking: called off the main
    /// thread, from the Turn's own thread.
    fn search(&self, query: &str, key: &str) -> Result<Vec<Hit>, SearchError>;
}

/// Which service answered, and what it found.
#[derive(Debug, PartialEq, Eq)]
pub struct Answered {
    pub provider: &'static str,
    pub hits: Vec<Hit>,
}

/// The keyed provider, tried first when a key is stored (ADR-0021).
pub fn keyed() -> Box<dyn SearchProvider> {
    Box::new(exa::ExaProvider)
}

/// The keyless provider. Always available, and the fallback for every failure.
pub fn keyless() -> Box<dyn SearchProvider> {
    Box::new(ddg::DdgProvider)
}

/// Search, falling back from the keyed provider to the keyless one (ADR-0021).
///
/// `announce` fires once per provider actually contacted, so a fallback repaints
/// the outbound header rather than naming a service that did not answer. Empty
/// counts as failure: the other index may have answers. Trade in the ADR.
pub fn search(
    keyed: &dyn SearchProvider,
    keyless: &dyn SearchProvider,
    query: &str,
    key: Option<&str>,
    mut announce: impl FnMut(&'static str),
) -> Result<Answered, SearchError> {
    if let Some(key) = key.filter(|k| !k.trim().is_empty()) {
        announce(keyed.label());
        match keyed.search(query, key) {
            Ok(hits) if !hits.is_empty() => {
                return Ok(Answered {
                    provider: keyed.label(),
                    hits,
                })
            }
            // Deliberately swallowed. Failure-as-switch means a wrong key reads
            // as slightly worse answers rather than an error; the trade is
            // recorded in ADR-0021 and surfaced only in the header.
            _ => {}
        }
    }
    announce(keyless.label());
    let hits = keyless.search(query, "")?;
    Ok(Answered {
        provider: keyless.label(),
        hits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake, so the fallback is tested without a network or a key.
    struct Fake {
        label: &'static str,
        result: Result<Vec<Hit>, SearchError>,
    }

    impl Fake {
        fn hit(label: &'static str) -> Self {
            Fake {
                label,
                result: Ok(vec![Hit {
                    title: label.into(),
                    url: format!("https://{label}/"),
                    description: String::new(),
                }]),
            }
        }

        fn failing(label: &'static str) -> Self {
            Fake {
                label,
                result: Err(SearchError::BadKey),
            }
        }

        fn empty(label: &'static str) -> Self {
            Fake {
                label,
                result: Ok(Vec::new()),
            }
        }
    }

    impl SearchProvider for Fake {
        fn label(&self) -> &'static str {
            self.label
        }
        fn needs_key(&self) -> bool {
            true
        }
        fn search(&self, _query: &str, _key: &str) -> Result<Vec<Hit>, SearchError> {
            self.result.clone()
        }
    }

    /// With no key stored the keyed provider is never contacted, so a fresh
    /// install searches without anyone signing up for anything.
    #[test]
    fn v0_10_without_a_key_only_the_keyless_provider_is_asked() {
        let mut announced = Vec::new();
        let answered = search(
            &Fake::hit("Keyed"),
            &Fake::hit("Keyless"),
            "ferrari",
            None,
            |p| announced.push(p),
        )
        .expect("the keyless provider answers");
        assert_eq!(announced, vec!["Keyless"]);
        assert_eq!(answered.provider, "Keyless");
    }

    /// A stored key promotes the keyed provider and nothing else is contacted.
    #[test]
    fn v0_10_a_stored_key_is_answered_by_the_keyed_provider_alone() {
        let mut announced = Vec::new();
        let answered = search(
            &Fake::hit("Keyed"),
            &Fake::hit("Keyless"),
            "ferrari",
            Some("exa-key"),
            |p| announced.push(p),
        )
        .expect("the keyed provider answers");
        assert_eq!(announced, vec!["Keyed"]);
        assert_eq!(answered.provider, "Keyed");
    }

    /// The trade in ADR-0021. A refused key is not an error, it is a fallback —
    /// and the header must name the service that actually answered, or it says
    /// the query went somewhere it did not.
    #[test]
    fn v0_10_a_failing_keyed_provider_falls_back_and_announces_twice() {
        let mut announced = Vec::new();
        let answered = search(
            &Fake::failing("Keyed"),
            &Fake::hit("Keyless"),
            "ferrari",
            Some("wrong-key"),
            |p| announced.push(p),
        )
        .expect("the keyless provider answers");
        assert_eq!(announced, vec!["Keyed", "Keyless"]);
        assert_eq!(answered.provider, "Keyless");
    }

    /// No answers from the keyed provider is worth retrying, not reporting: the
    /// other index may well have some.
    #[test]
    fn v0_10_an_empty_keyed_result_falls_back_rather_than_answering_nothing() {
        let answered = search(
            &Fake::empty("Keyed"),
            &Fake::hit("Keyless"),
            "ferrari",
            Some("exa-key"),
            |_| {},
        )
        .expect("the keyless provider answers");
        assert_eq!(answered.provider, "Keyless");
    }

    /// When the fallback itself fails there is nowhere left to go, so the error
    /// reaches the Palette rather than being swallowed too.
    #[test]
    fn v0_10_a_failing_fallback_surfaces_its_error() {
        let error = search(
            &Fake::failing("Keyed"),
            &Fake::failing("Keyless"),
            "ferrari",
            Some("wrong-key"),
            |_| {},
        )
        .expect_err("nothing answered");
        assert_eq!(error, SearchError::BadKey);
    }

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
