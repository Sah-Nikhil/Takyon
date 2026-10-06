//! Websites in a Block list: reading hosts, matching them, the block page URL.
//!
//! Pure. Input is what a browser's address bar displays, which comes in four
//! shapes (ADR-0035 § Spike result): with scheme, without, host only, and a
//! bare drive path for `file:`. Never fetches anything.

use std::path::Path;

/// Host the address bar shows, lowercased. None for anything not a web page.
pub fn host_of(display: &str) -> Option<String> {
    let t = display.trim().to_ascii_lowercase();
    if t.contains(char::is_whitespace) {
        return None; // Search text, not an address.
    }
    let rest = match t.split_once("://") {
        Some(("http" | "https", rest)) => rest,
        Some(_) => return None,
        None => &t,
    };
    let authority = rest.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit('@').next()?;
    // `x:<digits>` is a port; any other colon is a scheme (`about:`, `c:`).
    let host = match host_port.split_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => host,
        Some(_) => return None,
        None => host_port,
    };
    let host = host.trim_end_matches('.');
    let valid = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '.';
    (!host.is_empty() && host.chars().all(valid)).then(|| host.to_string())
}

/// Whether `host` is `domain` or under it.
pub fn matches(host: &str, domain: &str) -> bool {
    host.strip_suffix(domain)
        .is_some_and(|prefix| prefix.is_empty() || prefix.ends_with('.'))
}

/// Domain a typed website stores as, for a `site:` item.
///
/// Needs a dot: `youtube` alone is a search, not a site.
pub fn domain_of(typed: &str) -> Option<String> {
    let host = host_of(typed.trim().trim_start_matches("*."))?;
    let domain = match host.strip_prefix("www.") {
        Some(rest) if rest.contains('.') => rest.to_string(),
        _ => host,
    };
    domain.contains('.').then_some(domain)
}

/// Address a blocked tab is sent to.
pub fn block_url(page: &Path, original: &str, end_ms: u64, goal: &str) -> String {
    let path = page.to_string_lossy().replace('\\', "/");
    let path: Vec<String> = path.split('/').map(|s| encode(s, b":")).collect();
    format!(
        "file:///{}?u={}&end={end_ms}&g={}",
        path.join("/"),
        encode(original, b""),
        encode(goal, b"")
    )
}

/// Whether the address bar shows Takyon's own block page.
pub fn is_block_page(display: &str) -> bool {
    block_query(display).is_some()
}

/// The URL a block page stands in for, if `display` is one and it is http(s).
pub fn original_from(display: &str) -> Option<String> {
    let u = block_query(display)?
        .split('&')
        .find_map(|p| p.strip_prefix("u="))?;
    let u = decode(u);
    let lower = u.to_ascii_lowercase();
    let web = lower.starts_with("http://") || lower.starts_with("https://");
    (web && host_of(&u).is_some()).then_some(u)
}

/// Query string of a block page display. Local drive path only, so a web
/// page at `/focus/blocked.html` never counts.
fn block_query(display: &str) -> Option<&str> {
    let t = display.trim();
    let rest = t.strip_prefix("file:///").unwrap_or(t);
    let b = rest.as_bytes();
    let local = b.len() > 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'/';
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let ours = decode(path)
        .to_ascii_lowercase()
        .ends_with("/focus/blocked.html");
    (local && ours).then_some(query)
}

/// Percent-encode all but RFC 3986 unreserved bytes and `keep`.
fn encode(value: &str, keep: &[u8]) -> String {
    let mut out = String::with_capacity(value.len());
    for &b in value.as_bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) || keep.contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Percent-decode to UTF-8. Bad escapes stay as written.
fn decode(value: &str) -> String {
    let b = value.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = (b[i] == b'%' && i + 3 <= b.len())
            .then(|| std::str::from_utf8(&b[i + 1..i + 3]).ok())
            .flatten()
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match hex {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(display: &str) -> Option<String> {
        host_of(display)
    }

    /// Shapes copied from the spike's real address bars.
    #[test]
    fn v0_16_host_is_read_from_every_address_bar_shape() {
        assert_eq!(host("example.com/spike-a?x=1"), Some("example.com".into()));
        assert_eq!(
            host("https://example.com/spike-a?x=1"),
            Some("example.com".into())
        );
        assert_eq!(host("example.com"), Some("example.com".into()));
    }

    /// The block page itself, browser pages, and half-typed search text are
    /// never a host: blocking them would loop or hijack typing.
    #[test]
    fn v0_16_anything_not_a_web_page_has_no_host() {
        for display in [
            "C:/Users/me/AppData/Local/v3sper/takyon/focus/blocked.html?u=https%3A%2F%2Fexample.com",
            "file:///C:/Users/me/focus/blocked.html?u=x",
            "chrome://settings",
            "edge://newtab",
            "about:blank",
            "about:newtab",
            "javascript:alert(1)",
            "view-source:example.com",
            "data:text/html,hi",
            "ftp://example.com",
            "how to cook pasta",
            "",
            "   ",
            "/just/a/path",
        ] {
            assert_eq!(host(display), None, "{display:?}");
        }
    }

    #[test]
    fn v0_16_ports_credentials_case_and_trailing_dots_are_stripped() {
        assert_eq!(host("Localhost:3000/x"), Some("localhost".into()));
        assert_eq!(
            host("http://user:pw@Example.COM:8080/"),
            Some("example.com".into())
        );
        assert_eq!(host("example.com./"), Some("example.com".into()));
        assert_eq!(
            host("WWW.YouTube.com/watch?v=1"),
            Some("www.youtube.com".into())
        );
        assert_eq!(host("127.0.0.1:8080"), Some("127.0.0.1".into()));
    }

    #[test]
    fn v0_16_a_domain_blocks_itself_and_its_subdomains_only() {
        assert!(matches("youtube.com", "youtube.com"));
        assert!(matches("m.youtube.com", "youtube.com"));
        assert!(matches("www.youtube.com", "youtube.com"));
        assert!(!matches("notyoutube.com", "youtube.com"));
        assert!(!matches("youtube.com.evil.net", "youtube.com"));
        assert!(!matches("youtube.com", "m.youtube.com"));
    }

    /// What "Block website: <text>" stores. `www.` dropped so the parent
    /// domain, and every subdomain with it, is what gets blocked.
    #[test]
    fn v0_16_typed_websites_normalise_to_a_domain() {
        for (typed, want) in [
            ("https://www.YouTube.com/watch?v=1", "youtube.com"),
            ("youtube.com", "youtube.com"),
            ("  Reddit.com/r/rust  ", "reddit.com"),
            ("*.reddit.com", "reddit.com"),
            ("m.youtube.com", "m.youtube.com"),
            ("www.com", "www.com"),
        ] {
            assert_eq!(domain_of(typed), Some(want.into()), "{typed:?}");
        }
        for typed in [
            "youtube",
            "",
            "about:blank",
            "localhost:3000",
            "how to cook",
        ] {
            assert_eq!(domain_of(typed), None, "{typed:?}");
        }
    }

    const PAGE: &str = r"C:\Users\John Doe\AppData\Local\v3sper\takyon\focus\blocked.html";
    const ORIGINAL: &str = "https://example.com/spike-a?x=1&y=two#frag";

    #[test]
    fn v0_16_block_url_is_a_file_url_carrying_original_end_and_goal() {
        let url = block_url(Path::new(PAGE), ORIGINAL, 1_760_000_000_000, "Write & ship");
        assert_eq!(
            url,
            "file:///C:/Users/John%20Doe/AppData/Local/v3sper/takyon/focus/blocked.html\
             ?u=https%3A%2F%2Fexample.com%2Fspike-a%3Fx%3D1%26y%3Dtwo%23frag\
             &end=1760000000000&g=Write%20%26%20ship"
        );
    }

    /// Chromium shows the block page without `file:///`, Gecko with; Chromium
    /// may also show the path decoded.
    #[test]
    fn v0_16_original_reads_back_from_every_block_page_display() {
        let url = block_url(Path::new(PAGE), ORIGINAL, 0, "");
        let chromium = url.strip_prefix("file:///").unwrap();
        let decoded_path = chromium.replace("%20", " ");
        for display in [url.as_str(), chromium, decoded_path.as_str()] {
            assert!(is_block_page(display), "{display}");
            assert_eq!(original_from(display), Some(ORIGINAL.into()), "{display}");
        }
    }

    /// A web page at `/focus/blocked.html`, or a `u` that is not http(s), is
    /// never followed: restore must not become an open redirect.
    #[test]
    fn v0_16_only_our_page_and_only_web_urls_are_restored() {
        assert!(!is_block_page(
            "evil.com/focus/blocked.html?u=https%3A%2F%2Fx.com"
        ));
        assert_eq!(
            original_from("evil.com/focus/blocked.html?u=https%3A%2F%2Fx.com"),
            None
        );
        assert_eq!(
            original_from("https://evil.com/focus/blocked.html?u=https%3A%2F%2Fx.com"),
            None
        );
        assert_eq!(
            original_from("C:/x/focus/blocked.html?u=javascript%3Aalert(1)"),
            None
        );
        assert_eq!(original_from("C:/x/focus/blocked.html?end=1"), None);
        assert!(is_block_page("C:/x/focus/blocked.html?end=1"));
        assert!(!is_block_page(
            "C:/x/focus/other.html?u=https%3A%2F%2Fx.com"
        ));
    }
}
