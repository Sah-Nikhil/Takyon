//! Favicons for the sources `!s` cites (ADR-0022).
//!
//! One extra request per host, and only to hosts the search already contacted.
//! Deliberately not a favicon service: `google.com/s2/favicons` is one call and
//! would hand that service every host you read, on the one feature that already
//! leaves the machine.
//!
//! Cached to disk by host, because hosts repeat across searches where pages do
//! not. The webview never fetches any of this — bytes reach it through
//! `takyon-favicon://`, same seam as application icons.

use std::path::{Path, PathBuf};

use super::{fetch, SearchError};

/// The URI scheme the webview loads these through.
pub const SCHEME: &str = "takyon-favicon";

/// Where the cache lives, under the data directory.
pub const DIR: &str = "favicons";

/// Anything smaller is a tracking pixel or an error page, not an icon.
const MIN_BYTES: usize = 64;

/// The icon a page declares, resolved against the page's own URL.
///
/// Checked before `/favicon.ico` because a site that declares one usually
/// declares a better one, and because the root path 404s on plenty of hosts that
/// still have an icon.
pub fn declared(html: &str, page_url: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut best: Option<String> = None;
    let mut at = 0;
    while let Some(found) = lower[at..].find("<link") {
        let start = at + found;
        let end = lower[start..].find('>').map(|e| start + e)?;
        let tag = &lower[start..end];
        at = end;
        let rel = attribute(tag, "rel")?;
        if !rel.split_whitespace().any(|w| w == "icon") {
            continue;
        }
        // The original-case slice: a href is case-sensitive after the host.
        let Some(href) = attribute(&html[start..end], "href") else {
            continue;
        };
        let resolved = resolve(&href, page_url)?;
        // First declaration wins unless a later one is a PNG, which draws better
        // at 14px than the .ico most sites still declare first.
        let png = resolved.to_ascii_lowercase().ends_with(".png");
        if best.is_none() || png {
            best = Some(resolved);
            if png {
                break;
            }
        }
    }
    best
}

/// `https://<host>/favicon.ico`, the fallback every site is meant to serve.
pub fn root(page_url: &str) -> Option<String> {
    let (host, _, secure) = fetch::parse_url(page_url)?;
    Some(format!(
        "{}://{host}/favicon.ico",
        if secure { "https" } else { "http" }
    ))
}

/// Fetch one host's icon, declared first and root second.
///
/// `None` rather than an error: a missing favicon is cosmetic, and a source row
/// draws its letter tile instead.
pub fn fetch_one(page_url: &str, html: Option<&str>) -> Option<Vec<u8>> {
    let candidates = [
        html.and_then(|h| declared(h, page_url)),
        root(page_url),
    ];
    for url in candidates.into_iter().flatten() {
        if let Ok(response) = fetch::get_icon(&url) {
            if response.status == 200 && response.bytes.len() >= MIN_BYTES {
                return Some(response.bytes);
            }
        }
    }
    None
}

/// Fetch and cache every host in one search, in parallel, best effort.
///
/// Hosts already on disk are skipped, which is the whole reason this caches by
/// host: pages never repeat between searches and hosts constantly do.
pub fn cache_all(dir: &Path, urls: &[String], pages: &[Result<String, SearchError>]) {
    use rayon::prelude::*;
    let wanted: Vec<(String, usize)> = urls
        .iter()
        .enumerate()
        .filter_map(|(i, url)| {
            let (host, _, _) = fetch::parse_url(url)?;
            cached(dir, &host).is_none().then_some((host, i))
        })
        .collect();

    wanted.par_iter().for_each(|(host, i)| {
        let html = pages.get(*i).and_then(|p| p.as_deref().ok());
        if let Some(bytes) = fetch_one(&urls[*i], html) {
            let _ = store(dir, host, &bytes);
        }
    });
}

/// The cache file for a host.
///
/// The host is sanitised rather than hashed, so the directory stays readable and
/// a stale icon can be deleted by hand. Anything outside the unreserved set
/// becomes `_`, which cannot escape the directory.
pub fn cache_file(dir: &Path, host: &str) -> PathBuf {
    let lower = host.to_ascii_lowercase();
    // `www.` is stripped on both sides of the seam or neither. The frontend
    // displays the bare host and asks for the icon under it, so a key written
    // with the prefix is a file nothing ever reads.
    let bare = lower.strip_prefix("www.").unwrap_or(&lower);
    let safe: String = bare
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' | '.' | '-' => c,
            _ => '_',
        })
        .collect();
    dir.join(DIR).join(format!("{safe}.ico"))
}

/// A host's icon from disk, if one was ever fetched.
pub fn cached(dir: &Path, host: &str) -> Option<Vec<u8>> {
    let bytes = std::fs::read(cache_file(dir, host)).ok()?;
    (bytes.len() >= MIN_BYTES).then_some(bytes)
}

/// Write one host's icon. Best effort: a cache that cannot be written costs a
/// request next time and nothing else.
pub fn store(dir: &Path, host: &str, bytes: &[u8]) -> Result<(), SearchError> {
    let path = cache_file(dir, host);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| SearchError::Failed(format!("favicon cache: {e}")))?;
    }
    std::fs::write(&path, bytes).map_err(|e| SearchError::Failed(format!("favicon cache: {e}")))
}

/// The value of `name="..."` in a tag, quoted either way.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let at = tag.find(&format!("{name}="))? + name.len() + 1;
    let rest = tag[at..].trim_start();
    let quote = rest.chars().next()?;
    if quote == '"' || quote == '\'' {
        let body = &rest[1..];
        return Some(body[..body.find(quote)?].to_string());
    }
    Some(rest.split_whitespace().next()?.to_string())
}

/// An href against the page it was declared on. Absolute, protocol-relative and
/// root-relative, which is every form that occurs in practice.
fn resolve(href: &str, page_url: &str) -> Option<String> {
    let href = href.trim();
    if href.starts_with("http://") || href.starts_with("https://") {
        return Some(href.to_string());
    }
    let (host, _, secure) = fetch::parse_url(page_url)?;
    let scheme = if secure { "https" } else { "http" };
    if let Some(rest) = href.strip_prefix("//") {
        return Some(format!("{scheme}://{rest}"));
    }
    if let Some(rest) = href.strip_prefix('/') {
        return Some(format!("{scheme}://{host}/{rest}"));
    }
    Some(format!("{scheme}://{host}/{href}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<html><head>
      <link rel="shortcut icon" href="/favicon.ico">
      <link rel="icon" type="image/png" href="/static/icon-32.png">
    </head><body></body></html>"#;

    /// A PNG beats the `.ico` most sites declare first: at 14px on a dark plate
    /// the multi-resolution `.ico` usually hands back a 16px bitmap that has
    /// already been downscaled once.
    #[test]
    fn v0_10_a_declared_png_wins_over_a_declared_ico() {
        let url = "https://example.com/article";
        assert_eq!(
            declared(PAGE, url).as_deref(),
            Some("https://example.com/static/icon-32.png")
        );
    }

    #[test]
    fn v0_10_a_page_declaring_nothing_falls_back_to_the_root() {
        assert_eq!(declared("<html><head></head></html>", "https://e.x/a"), None);
        assert_eq!(root("https://e.x/a/b").as_deref(), Some("https://e.x/favicon.ico"));
    }

    /// Every href form that occurs: absolute, protocol-relative, root-relative
    /// and bare. Getting one wrong fetches the wrong host, which is the failure
    /// this whole module exists to avoid.
    #[test]
    fn v0_10_hrefs_resolve_against_the_page() {
        let page = "https://news.example.com/2026/story";
        assert_eq!(
            resolve("https://cdn.other.com/i.png", page).as_deref(),
            Some("https://cdn.other.com/i.png")
        );
        assert_eq!(
            resolve("//cdn.other.com/i.png", page).as_deref(),
            Some("https://cdn.other.com/i.png")
        );
        assert_eq!(
            resolve("/i.png", page).as_deref(),
            Some("https://news.example.com/i.png")
        );
        assert_eq!(
            resolve("i.png", page).as_deref(),
            Some("https://news.example.com/i.png")
        );
    }

    /// The cache key is a filename. A host carrying separators must collapse to
    /// one component inside the directory — dots may survive, traversal may not.
    #[test]
    fn v0_10_a_hostile_host_cannot_escape_the_cache_directory() {
        let dir = Path::new(r"C:\data");
        for host in ["../../windows/system32", r"..\..\evil", "a/b/c", "..", "C:evil"] {
            let path = cache_file(dir, host);
            let inside = dir.join(DIR);
            assert!(path.starts_with(&inside), "{}", path.display());
            // Exactly one component past the directory: no separator survived,
            // so there is nothing left to traverse with.
            let rest: Vec<_> = path.strip_prefix(&inside).unwrap().components().collect();
            assert_eq!(rest.len(), 1, "{} -> {}", host, path.display());
            assert!(!matches!(rest[0], std::path::Component::ParentDir));
        }
    }

    /// The seam. Rust writes the key from the URL's host, the frontend asks for
    /// it under the host it displays, and that one strips `www.`. Found by
    /// driving a real search: six icons were on disk and the two `www.` ones
    /// resolved to nothing.
    #[test]
    fn v0_10_a_www_host_and_its_bare_form_are_one_cache_entry() {
        let dir = Path::new(r"C:\data");
        assert_eq!(
            cache_file(dir, "www.allrecipes.com"),
            cache_file(dir, "allrecipes.com")
        );
        assert_eq!(
            cache_file(dir, "WWW.AllRecipes.COM"),
            cache_file(dir, "allrecipes.com")
        );
        // Only a leading `www.`: a host that merely contains it is another site.
        assert_ne!(
            cache_file(dir, "wwwx.example.com"),
            cache_file(dir, "example.com")
        );
    }

    #[test]
    fn v0_10_the_cache_round_trips_and_rejects_a_stub() {
        let dir = std::env::temp_dir().join("takyon-favicon-test");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(cached(&dir, "example.com"), None);

        store(&dir, "example.com", &vec![7u8; 512]).expect("store");
        assert_eq!(cached(&dir, "example.com").map(|b| b.len()), Some(512));

        // A one-pixel response is a tracker or an error page, not an icon.
        store(&dir, "stub.com", &[0u8; 8]).expect("store");
        assert_eq!(cached(&dir, "stub.com"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn v0_10_attributes_parse_with_either_quote() {
        assert_eq!(attribute(r#"<link rel="icon">"#, "rel").as_deref(), Some("icon"));
        assert_eq!(attribute(r#"<link rel='icon'>"#, "rel").as_deref(), Some("icon"));
        assert_eq!(attribute("<link rel=icon >", "rel").as_deref(), Some("icon"));
    }
}
