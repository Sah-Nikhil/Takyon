//! The Brave Search provider (ADR-0005).
//!
//! One GET to `/res/v1/web/search` with the key in `X-Subscription-Token`. The
//! response is parsed rather than deserialised into a mirror of Brave's schema:
//! only three fields are used, and a shape change elsewhere must not break `!s`.

use super::{fetch, strip_markup, Hit, SearchError, SearchProvider, MAX_HITS};

/// The endpoint. Host and path apart, so `fetch` can take them separately.
const HOST: &str = "api.search.brave.com";
const PATH: &str = "/res/v1/web/search";

pub struct BraveProvider;

impl SearchProvider for BraveProvider {
    fn label(&self) -> &'static str {
        super::PROVIDER_LABEL
    }

    fn needs_key(&self) -> bool {
        true
    }

    fn search(&self, query: &str, key: &str) -> Result<Vec<Hit>, SearchError> {
        if key.trim().is_empty() {
            return Err(SearchError::NoKey);
        }
        let path = format!(
            "{PATH}?q={}&count={MAX_HITS}",
            fetch::percent_encode(query.trim())
        );
        let response = fetch::get(
            HOST,
            &path,
            &[
                ("Accept", "application/json"),
                ("X-Subscription-Token", key.trim()),
            ],
        )?;
        match response.status {
            200 => parse_hits(&response.body),
            401 | 403 => Err(SearchError::BadKey),
            429 => Err(SearchError::RateLimited),
            code => Err(SearchError::Failed(format!(
                "{} answered {code}.",
                super::PROVIDER_LABEL
            ))),
        }
    }
}

/// Hits from a Brave response body.
///
/// An empty `web.results` is not an error: a query with no answers is a real
/// outcome and the Palette says so.
pub fn parse_hits(body: &str) -> Result<Vec<Hit>, SearchError> {
    let parsed: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| SearchError::Failed(format!("{} sent no JSON.", super::PROVIDER_LABEL)))?;

    let results = parsed
        .get("web")
        .and_then(|web| web.get("results"))
        .and_then(|results| results.as_array());
    let Some(results) = results else {
        return Ok(Vec::new());
    };

    Ok(results
        .iter()
        .filter_map(|hit| {
            let url = hit.get("url")?.as_str()?.trim();
            // A Hit with no URL cannot be opened or read, so it is not a Hit.
            if url.is_empty() {
                return None;
            }
            Some(Hit {
                title: strip_markup(hit.get("title").and_then(|t| t.as_str()).unwrap_or(url)),
                url: url.to_string(),
                description: strip_markup(
                    hit.get("description").and_then(|d| d.as_str()).unwrap_or(""),
                ),
            })
        })
        .take(MAX_HITS)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real response shape, trimmed to the three fields `!s` reads.
    const BODY: &str = r#"{
      "query": {"original": "ferrari in f1"},
      "web": {"results": [
        {"title": "<strong>Ferrari</strong> in F1",
         "url": "https://example.com/ferrari",
         "description": "Scuderia <strong>Ferrari</strong> is the oldest team."},
        {"title": "F1 standings",
         "url": "https://example.org/standings",
         "description": "Constructor points."}
      ]}
    }"#;

    #[test]
    fn v0_9_hits_carry_the_title_url_and_description() {
        let hits = parse_hits(BODY).expect("a valid body parses");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].url, "https://example.com/ferrari");
        assert_eq!(hits[1].title, "F1 standings");
    }

    /// The provider highlights matched terms. Rendered raw that markup shows up
    /// as angle brackets in the Palette.
    #[test]
    fn v0_9_the_providers_highlight_markup_is_stripped() {
        let hits = parse_hits(BODY).expect("a valid body parses");
        assert_eq!(hits[0].title, "Ferrari in F1");
        assert_eq!(hits[0].description, "Scuderia Ferrari is the oldest team.");
    }

    /// A Hit's text is written by whoever owns the page. A right-to-left
    /// override makes a title draw itself reversed, which is how a row is made
    /// to name a site it did not come from.
    #[test]
    fn v0_9_a_hit_cannot_carry_control_or_bidi_characters() {
        // Escaped in the JSON rather than written literally: rustc refuses a
        // bidi character in source, which is this same defence one layer down.
        let body = concat!(
            r#"{"web":{"results":[{"title":"gnp.exe\u202E","#,
            r#""url":"https://e.x/a","description":"line\u0000one two"}]}}"#
        );
        let hits = parse_hits(body).expect("parses");
        assert_eq!(hits[0].title, "gnp.exe");
        assert!(!hits[0].title.chars().any(char::is_control));
        assert_eq!(hits[0].description, "lineone two");
    }

    /// No answers is an outcome, not a failure — the Palette says so rather than
    /// showing an error for a question nobody has written about.
    #[test]
    fn v0_9_a_response_with_no_results_is_empty_rather_than_an_error() {
        assert_eq!(parse_hits(r#"{"web":{"results":[]}}"#), Ok(Vec::new()));
        assert_eq!(parse_hits(r#"{"query":{"original":"x"}}"#), Ok(Vec::new()));
    }

    /// A Hit with no URL can neither be opened nor read, so it is dropped rather
    /// than rendered as a row that does nothing.
    #[test]
    fn v0_9_a_hit_without_a_url_is_dropped() {
        let body = r#"{"web":{"results":[{"title":"No link"},{"title":"Real","url":"https://a.b"}]}}"#;
        let hits = parse_hits(body).expect("parses");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, "https://a.b");
    }

    /// Never more than the page asked for, whatever the provider sends.
    #[test]
    fn v0_9_no_more_hits_than_the_page_size() {
        let rows: Vec<String> = (0..40)
            .map(|i| format!(r#"{{"title":"t{i}","url":"https://e/{i}"}}"#))
            .collect();
        let body = format!(r#"{{"web":{{"results":[{}]}}}}"#, rows.join(","));
        assert_eq!(parse_hits(&body).expect("parses").len(), MAX_HITS);
    }

    /// A body that is not JSON at all fails in the provider's name, so the
    /// Palette can say which service misbehaved.
    #[test]
    fn v0_9_a_non_json_body_fails_by_name() {
        let error = parse_hits("<html>502 Bad Gateway</html>").expect_err("not JSON");
        assert!(error.message().contains(super::super::PROVIDER_LABEL));
    }

    /// An empty key never reaches the network: it is the state a fresh install
    /// is in, and a request would spend a round trip to learn that.
    #[test]
    fn v0_9_an_empty_key_fails_before_any_request() {
        assert_eq!(
            BraveProvider.search("ferrari", "   "),
            Err(SearchError::NoKey)
        );
    }
}
