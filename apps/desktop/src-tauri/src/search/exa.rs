//! The Exa provider: the keyed half of `!s` (ADR-0021, superseding ADR-0005's
//! choice of Brave).
//!
//! One POST to `/search` with the key in `x-api-key`. Exa returns page text
//! alongside each result, which is asked for and used as the snippet; the pages
//! are still read by `fetch` because the summariser wants more than a snippet.
//!
//! Parsed rather than deserialised into a mirror of Exa's schema: three fields
//! are used, and a shape change elsewhere must not break `!s`.

use super::{fetch, strip_markup, Hit, SearchError, SearchProvider, MAX_HITS};

/// Display name, and the label its errors are written in.
pub const LABEL: &str = "Exa";

/// Where a key comes from.
pub const SIGNUP_URL: &str = "https://dashboard.exa.ai/api-keys";

const HOST: &str = "api.exa.ai";
const PATH: &str = "/search";

/// Snippet length asked of Exa. Long enough to be a useful row, short enough
/// that ten of them do not dominate the response.
const SNIPPET_CHARS: usize = 400;

pub struct ExaProvider;

impl SearchProvider for ExaProvider {
    fn label(&self) -> &'static str {
        LABEL
    }

    fn needs_key(&self) -> bool {
        true
    }

    fn search(&self, query: &str, key: &str) -> Result<Vec<Hit>, SearchError> {
        if key.trim().is_empty() {
            return Err(SearchError::NoKey);
        }
        let body = serde_json::json!({
            "query": query.trim(),
            "numResults": MAX_HITS,
            "contents": { "text": { "maxCharacters": SNIPPET_CHARS } },
        })
        .to_string();
        let response = fetch::post(
            HOST,
            PATH,
            &[
                ("Accept", "application/json"),
                ("Content-Type", "application/json"),
                ("x-api-key", key.trim()),
            ],
            &body,
        )?;
        match response.status {
            200 => parse_hits(&response.body),
            401 | 403 => Err(SearchError::BadKey),
            429 => Err(SearchError::RateLimited),
            code => Err(SearchError::Failed(format!("{LABEL} answered {code}."))),
        }
    }
}

/// Hits from an Exa response body.
///
/// An empty `results` is not an error here, though the caller treats it as one
/// worth falling back on (`search::search`).
pub fn parse_hits(body: &str) -> Result<Vec<Hit>, SearchError> {
    let parsed: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| SearchError::Failed(format!("{LABEL} sent no JSON.")))?;

    let Some(results) = parsed.get("results").and_then(|r| r.as_array()) else {
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
                description: snippet(hit.get("text").and_then(|t| t.as_str()).unwrap_or("")),
            })
        })
        .take(MAX_HITS)
        .collect())
}

/// One paragraph's worth of a page's text, for the row.
///
/// Exa returns the page body, not a search snippet, so an unbounded copy would
/// put a whole article in a Palette row and in every IPC message.
fn snippet(text: &str) -> String {
    let cleaned = strip_markup(text);
    match cleaned.char_indices().nth(SNIPPET_CHARS) {
        None => cleaned,
        Some((cut, _)) => {
            let trimmed = cleaned[..cut].trim_end();
            let end = trimmed.rfind(' ').unwrap_or(trimmed.len());
            format!("{}…", &trimmed[..end])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exa's response shape, trimmed to the fields `!s` reads.
    const BODY: &str = r#"{
      "requestId": "abc",
      "results": [
        {"title": "Scuderia Ferrari",
         "url": "https://en.wikipedia.org/wiki/Scuderia_Ferrari",
         "publishedDate": "2024-01-01",
         "text": "Scuderia Ferrari is the racing division of Ferrari.",
         "score": 0.42},
        {"title": "Ferrari &amp; F1",
         "url": "https://www.formula1.com/en/teams/ferrari",
         "text": "Team page."}
      ]
    }"#;

    #[test]
    fn v0_10_hits_carry_the_title_url_and_text() {
        let hits = parse_hits(BODY).expect("a valid body parses");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].url, "https://en.wikipedia.org/wiki/Scuderia_Ferrari");
        assert_eq!(
            hits[0].description,
            "Scuderia Ferrari is the racing division of Ferrari."
        );
        assert_eq!(hits[1].title, "Ferrari & F1");
    }

    /// Exa returns page bodies rather than snippets. Copied whole, one article
    /// fills a Palette row and every IPC message carrying it.
    #[test]
    fn v0_10_a_long_text_is_cut_to_a_snippet_on_a_word_boundary() {
        let long = "wordy ".repeat(200);
        let body = format!(
            r#"{{"results":[{{"title":"t","url":"https://e.x/a","text":"{long}"}}]}}"#
        );
        let hits = parse_hits(&body).expect("parses");
        let snippet = &hits[0].description;
        assert!(snippet.chars().count() <= SNIPPET_CHARS + 1);
        assert!(snippet.ends_with('…'));
        // The cut lands between words, not inside one: "wor…" is the failure.
        assert!(snippet.trim_end_matches('…').ends_with("wordy"));
    }

    /// A Hit's text is written by whoever owns the page. A right-to-left
    /// override makes a title draw itself reversed, which is how a row is made
    /// to name a site it did not come from.
    #[test]
    fn v0_10_a_hit_cannot_carry_control_or_bidi_characters() {
        // Escaped in the JSON rather than written literally: rustc refuses a
        // bidi character in source, which is this same defence one layer down.
        let body = concat!(
            r#"{"results":[{"title":"gnp.exe\u202E","#,
            r#""url":"https://e.x/a","text":"line\u0000one two"}]}"#
        );
        let hits = parse_hits(body).expect("parses");
        assert_eq!(hits[0].title, "gnp.exe");
        assert!(!hits[0].title.chars().any(char::is_control));
        assert_eq!(hits[0].description, "lineone two");
    }

    #[test]
    fn v0_10_a_hit_without_a_url_is_dropped() {
        let body = r#"{"results":[{"title":"No link"},{"title":"Real","url":"https://a.b"}]}"#;
        let hits = parse_hits(body).expect("parses");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, "https://a.b");
    }

    #[test]
    fn v0_10_no_more_hits_than_the_page_size() {
        let rows: Vec<String> = (0..40)
            .map(|i| format!(r#"{{"title":"t{i}","url":"https://e/{i}"}}"#))
            .collect();
        let body = format!(r#"{{"results":[{}]}}"#, rows.join(","));
        assert_eq!(parse_hits(&body).expect("parses").len(), MAX_HITS);
    }

    /// A body that is not JSON fails in the provider's name, so the Palette can
    /// say which service misbehaved.
    #[test]
    fn v0_10_a_non_json_body_fails_by_name() {
        let error = parse_hits("<html>502 Bad Gateway</html>").expect_err("not JSON");
        assert!(error.message().contains(LABEL));
    }

    /// An empty key never reaches the network. It is the state a fresh install
    /// is in, and `search::search` reads it as "use DuckDuckGo" before this.
    #[test]
    fn v0_10_an_empty_key_fails_before_any_request() {
        assert_eq!(ExaProvider.search("ferrari", "   "), Err(SearchError::NoKey));
    }
}
