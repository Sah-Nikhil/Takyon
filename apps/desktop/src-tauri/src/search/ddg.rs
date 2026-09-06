//! The DuckDuckGo provider: no key, no account (ADR-0021).
//!
//! `html.duckduckgo.com/html/` renders results without JavaScript, so the pages
//! come back through the same WinHTTP stack everything else uses and no browser
//! engine is involved (ADR-0005 stands).
//!
//! There is no JSON here. The endpoint is HTML meant for a browser, so this
//! parses markup, which is a maintenance cost TBC-0004 owns: a class rename at
//! DuckDuckGo breaks `!s` with no warning and no version to pin.

use super::{fetch, Hit, SearchError, SearchProvider, MAX_HITS};

/// The no-JavaScript endpoint. Host and path apart, so `fetch` takes them
/// separately.
const HOST: &str = "html.duckduckgo.com";
const PATH: &str = "/html/";

/// Display name, and the label its errors are written in.
pub const LABEL: &str = "DuckDuckGo";

pub struct DdgProvider;

impl SearchProvider for DdgProvider {
    fn label(&self) -> &'static str {
        LABEL
    }

    fn needs_key(&self) -> bool {
        false
    }

    fn search(&self, query: &str, _key: &str) -> Result<Vec<Hit>, SearchError> {
        let path = format!("{PATH}?q={}", fetch::percent_encode(query.trim()));
        let response = fetch::get(HOST, &path, &[("Accept", "text/html")])?;
        match response.status {
            200 => parse_hits(&response.body),
            // No key to be wrong, so every refusal is the same thing: the
            // endpoint declining to serve this request.
            code => Err(SearchError::Failed(format!("{LABEL} answered {code}."))),
        }
    }
}

/// Hits from one results page.
pub fn parse_hits(html: &str) -> Result<Vec<Hit>, SearchError> {
    let mut hits = Vec::new();
    for block in html.split(r#"class="result__a""#).skip(1) {
        let Some(href) = attribute(block, "href") else {
            continue;
        };
        let Some(url) = real_url(&href) else {
            continue;
        };
        hits.push(Hit {
            title: super::strip_markup(&anchor_text(block).unwrap_or_default()),
            url,
            description: block
                .find(r#"class="result__snippet""#)
                .and_then(|at| anchor_text(&block[at..]))
                .map(|text| super::strip_markup(&text))
                .unwrap_or_default(),
        });
        if hits.len() == MAX_HITS {
            break;
        }
    }
    Ok(hits)
}

/// The text of the first anchor a fragment opens with, tags and all.
fn anchor_text(fragment: &str) -> Option<String> {
    let open = fragment.find('>')? + 1;
    let rest = &fragment[open..];
    Some(rest[..rest.find("</a>")?].to_string())
}

/// The value of `name="..."` at the start of a fragment.
fn attribute(fragment: &str, name: &str) -> Option<String> {
    let start = fragment.find(&format!("{name}=\""))? + name.len() + 2;
    let rest = &fragment[start..];
    Some(rest[..rest.find('"')?].to_string())
}

/// The page a `//duckduckgo.com/l/?uddg=...` redirector points at.
///
/// Unwrapped rather than followed: the redirector costs a round trip through a
/// third site on every source click and every page read, and it names
/// duckduckgo.com where the row names the real host.
fn real_url(href: &str) -> Option<String> {
    let target = href.split("uddg=").nth(1)?;
    let target = target.split(['&', '"']).next()?;
    let url = percent_decode(&target.replace("&amp;", "&"));
    url.starts_with("http").then_some(url)
}

/// Percent-decoding, bytes then UTF-8. Invalid escapes are left as written.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&value[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two results, copied verbatim from a live response rather than written by
    /// hand: an invented fixture proves only that the parser matches the fixture.
    const PAGE: &str = r##"<div id="links" class="results">
      <div class="result results_links results_links_deep web-result ">
        <div class="links_main links_deep result__body">
          <h2 class="result__title">
            <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.ferrari.com%2Fen%2DEN%2Fformula1&amp;rut=84bc6c434f97111bcfc45478b5cac9714079b588807a3dd50094f61e931def01">Scuderia Ferrari HP Formula 1 - Ferrari.com</a>
          </h2>
          <a class="result__snippet" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.ferrari.com%2Fen%2DEN%2Fformula1&amp;rut=84bc6c434f97111bcfc45478b5cac9714079b588807a3dd50094f61e931def01">Visit the Official Website of the Scuderia <b>Ferrari</b> Formula 1: all the news on the Team</a>
        </div>
      </div>
      <div class="result results_links results_links_deep web-result ">
        <div class="links_main links_deep result__body">
          <h2 class="result__title">
            <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fen.wikipedia.org%2Fwiki%2FScuderia_Ferrari&amp;rut=b108545dfba90d838096d89eaa72bc188bc3c749c50ac1338c13c910bf50e2fb">Scuderia Ferrari - Wikipedia</a>
          </h2>
          <a class="result__snippet" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fen.wikipedia.org%2Fwiki%2FScuderia_Ferrari&amp;rut=b108545dfba90d838096d89eaa72bc188bc3c749c50ac1338c13c910bf50e2fb">Scuderia <b>Ferrari</b> is the racing division of luxury Italian auto manufacturer Ferrari &amp; the oldest team.</a>
        </div>
      </div>
    </div>"##;

    /// The href is a DuckDuckGo redirector, not the page. A Hit carrying the
    /// redirector would send every source click and every page read back through
    /// duckduckgo.com, which is both slower and a different site than the one the
    /// row names.
    #[test]
    fn v0_10_a_hit_carries_the_real_url_not_the_redirector() {
        let hits = parse_hits(PAGE).expect("a real page parses");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].url, "https://www.ferrari.com/en-EN/formula1");
        assert_eq!(hits[1].url, "https://en.wikipedia.org/wiki/Scuderia_Ferrari");
    }

    /// The endpoint is markup for a browser, so a row's text arrives with `<b>`
    /// highlighting and HTML entities. Rendered raw, the Palette shows the tags
    /// and a literal `&amp;`.
    #[test]
    fn v0_10_a_hit_carries_text_without_markup_or_entities() {
        let hits = parse_hits(PAGE).expect("a real page parses");
        assert_eq!(hits[0].title, "Scuderia Ferrari HP Formula 1 - Ferrari.com");
        assert_eq!(
            hits[0].description,
            "Visit the Official Website of the Scuderia Ferrari Formula 1: all the news on the Team"
        );
        assert_eq!(
            hits[1].description,
            "Scuderia Ferrari is the racing division of luxury Italian auto manufacturer Ferrari & the oldest team."
        );
    }
}
