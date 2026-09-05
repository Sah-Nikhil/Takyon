//! HTML to readable text, Readability-style (ADR-0005).
//!
//! No browser and no DOM. Script, style and the rest of the furniture are cut,
//! then whichever of `<article>`, `<main>` or `<body>` is present is kept and
//! its tags stripped. Crude next to Readability proper, and enough: what the
//! summariser needs is prose, not structure.
//!
//! A page that yields nothing is not silently dropped — `synth` names it, or a
//! thinner answer reads as a complete one (v0.9 Traps).

/// Elements whose contents are never prose.
const DROPPED: [&str; 6] = ["script", "style", "noscript", "svg", "nav", "footer"];

/// Most characters kept per page. Roughly 2,000 words, which is more than a
/// summariser needs and less than a prompt can be flooded by.
pub const MAX_CHARS: usize = 12_000;

/// Readable text from an HTML document, or `None` when there is no prose in it.
pub fn readable(html: &str) -> Option<String> {
    let cleaned = drop_elements(html);
    let body = main_region(&cleaned);
    let text = strip_tags(body);
    let text = collapse(&text);
    if text.chars().count() < 200 {
        // Below this it is navigation, a consent wall, or a page that draws
        // itself in JavaScript. The headless fallback is deferred (post-v1).
        return None;
    }
    Some(text.chars().take(MAX_CHARS).collect())
}

/// The document's title, for naming a source the user has not opened.
pub fn title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title")?;
    let open = lower[start..].find('>')? + start + 1;
    let end = lower[open..].find("</title>")? + open;
    let title = collapse(&decode_entities(&html[open..end]));
    (!title.is_empty()).then_some(title)
}

/// Remove whole elements, contents included.
fn drop_elements(html: &str) -> String {
    let mut out = html.to_string();
    for tag in DROPPED {
        out = drop_one(&out, tag);
    }
    out
}

fn drop_one(html: &str, tag: &str) -> String {
    let lower = html.to_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = String::with_capacity(html.len());
    let mut at = 0;
    while let Some(start) = lower[at..].find(&open).map(|i| i + at) {
        out.push_str(&html[at..start]);
        match lower[start..].find(&close) {
            Some(end) => at = start + end + close.len(),
            // An unclosed `<script>` means the rest of the document is inside
            // it. Cutting to the end is what a parser would do.
            None => return out,
        }
    }
    out.push_str(&html[at..]);
    out
}

/// The region worth reading: `<article>`, then `<main>`, then `<body>`.
fn main_region(html: &str) -> &str {
    for tag in ["article", "main", "body"] {
        if let Some(region) = region(html, tag) {
            return region;
        }
    }
    html
}

fn region<'a>(html: &'a str, tag: &str) -> Option<&'a str> {
    let lower = html.to_lowercase();
    let start = lower.find(&format!("<{tag}"))?;
    let open = lower[start..].find('>')? + start + 1;
    let end = lower[open..]
        .find(&format!("</{tag}>"))
        .map(|i| i + open)
        .unwrap_or(html.len());
    Some(&html[open..end])
}

/// Drop every tag, keeping the text between them.
///
/// Block-level tags become newlines first, or paragraphs run together into one
/// sentence that reads as nonsense.
fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut inside = false;
    let mut tag = String::new();
    for ch in html.chars() {
        match ch {
            '<' => {
                inside = true;
                tag.clear();
            }
            '>' => {
                inside = false;
                let name = tag.trim_start_matches('/').trim().to_lowercase();
                let name = name.split([' ', '\t', '\n']).next().unwrap_or("");
                if matches!(
                    name,
                    "p" | "br" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                ) {
                    out.push('\n');
                }
            }
            _ if inside => tag.push(ch),
            _ => out.push(ch),
        }
    }
    decode_entities(&out)
}

/// The handful of entities that actually appear in prose. Not a full table:
/// anything rarer survives as itself and costs a reader nothing.
fn decode_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

/// Collapse runs of whitespace, keeping paragraph breaks.
fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank = 0usize;
    for line in text.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            blank += 1;
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
            if blank > 0 {
                out.push('\n');
            }
        }
        blank = 0;
        out.push_str(&line);
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn article(body: &str) -> String {
        format!("<html><head><title>A page</title></head><body><article>{body}</article></body></html>")
    }

    /// Enough prose to clear the floor, so a fixture is about what is being
    /// tested rather than about its length.
    fn prose(times: usize) -> String {
        "<p>Scuderia Ferrari has raced in Formula One since 1950 and is the oldest surviving team.</p>".repeat(times)
    }

    #[test]
    fn v0_9_prose_survives_and_tags_do_not() {
        let text = readable(&article(&prose(4))).expect("prose is readable");
        assert!(text.starts_with("Scuderia Ferrari has raced"));
        assert!(!text.contains('<'), "a tag survived into the text");
    }

    /// Script and style are the two that turn an answer into gibberish, because
    /// both are full of words a summariser will happily read.
    #[test]
    fn v0_9_script_and_style_contents_never_reach_the_text() {
        let html = article(&format!(
            "<script>var secret = 'tracking pixel';</script><style>.a{{color:red}}</style>{}",
            prose(4)
        ));
        let text = readable(&html).expect("prose is readable");
        assert!(!text.contains("tracking pixel"));
        assert!(!text.contains("color:red"));
    }

    /// Paragraphs must not run together, or two sentences become one that says
    /// something neither of them did.
    #[test]
    fn v0_9_block_elements_become_line_breaks() {
        let html = article(&format!("<p>First.</p><p>Second.</p>{}", prose(4)));
        let text = readable(&html).expect("prose is readable");
        // A paragraph break, not a line break: `</p><p>` closes one and opens
        // the next, and running them together makes one sentence of two.
        assert!(text.contains("First.\n\nSecond."), "got: {text}");
    }

    /// `<article>` wins over the rest of the page, which is the whole point of
    /// preferring it: navigation and comments are not the article.
    #[test]
    fn v0_9_the_article_region_wins_over_the_page_around_it() {
        let html = format!(
            "<html><body><nav>Home About Contact</nav><article>{}</article><footer>Cookie policy</footer></body></html>",
            prose(4)
        );
        let text = readable(&html).expect("prose is readable");
        assert!(!text.contains("Home About Contact"));
        assert!(!text.contains("Cookie policy"));
    }

    /// A page that draws itself in JavaScript yields nothing, and says so by
    /// returning `None` rather than a line of navigation dressed as an article.
    #[test]
    fn v0_9_a_page_with_no_prose_yields_nothing() {
        assert_eq!(readable("<html><body><div id=root></div></body></html>"), None);
        assert_eq!(readable(""), None);
    }

    /// Entities are decoded, or a quoted answer arrives full of `&quot;`.
    #[test]
    fn v0_9_common_entities_are_decoded() {
        let html = article(&format!("<p>Ferrari &amp; Co. said &quot;no&quot;.</p>{}", prose(4)));
        let text = readable(&html).expect("prose is readable");
        assert!(text.contains(r#"Ferrari & Co. said "no"."#), "got: {text}");
    }

    /// One page cannot flood the prompt: everything past the cap is dropped.
    #[test]
    fn v0_9_one_page_is_capped() {
        let text = readable(&article(&prose(4000))).expect("prose is readable");
        assert_eq!(text.chars().count(), MAX_CHARS);
    }

    /// The title names a source the user has not opened yet.
    #[test]
    fn v0_9_the_document_title_is_read() {
        assert_eq!(title(&article("x")).as_deref(), Some("A page"));
        assert_eq!(title("<html><body>no title</body></html>"), None);
    }

    /// An unclosed `<script>` swallows the rest of the document in a parser, and
    /// must here too — the alternative is emitting minified JavaScript as prose.
    #[test]
    fn v0_9_an_unclosed_script_takes_the_rest_of_the_document_with_it() {
        let html = format!("<article>{}<script>var x = 1;", prose(4));
        let text = readable(&html).expect("prose before the script is readable");
        assert!(!text.contains("var x"));
    }
}
