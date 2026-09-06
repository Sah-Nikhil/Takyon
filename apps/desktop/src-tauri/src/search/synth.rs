//! Synthesis: read pages become one prompt for an Agent (v0.9 task 4).
//!
//! Takyon holds no LLM account (ADR-0017), so the summariser is whichever Agent
//! `!c` would have asked — the ranked, switched-on list, tools off, in the
//! Scratch directory. A machine with only Codex installed still gets `!s`.
//!
//! Citations are numbered and the Agent is told to cite those numbers, so the
//! Palette can turn `[2]` into the second Hit's URL without parsing prose.

use super::{extract, Hit, SearchError};

/// One Hit, with whatever its page yielded.
///
/// **Citation, not Source**: `CONTEXT.md` gives Source to the producers of
/// Entries. The answer's own list is user-facing copy and still says "Sources",
/// which is what a reader of an answer expects to see above links.
#[derive(Clone, Debug)]
pub struct Citation {
    pub hit: Hit,
    /// Extracted prose, or `None` where the page could not be read. Kept rather
    /// than dropped: a page that failed is named in the answer (v0.9 Traps).
    pub text: Option<String>,
}

/// Most characters of page text in one prompt, across every citation.
///
/// A budget rather than per-citation arithmetic: ten pages at `extract::MAX_CHARS`
/// each would be 120,000 characters, which is a bill and a slow first token.
pub const PROMPT_BUDGET: usize = 24_000;

/// Turn Hits and their fetched bodies into Citations.
pub fn citations(hits: Vec<Hit>, bodies: Vec<Result<String, SearchError>>) -> Vec<Citation> {
    hits.into_iter()
        .zip(bodies)
        .map(|(hit, body)| Citation {
            hit,
            text: body.ok().and_then(|html| extract::readable(&html)),
        })
        .collect()
}

/// The prompt one Turn answers.
///
/// Every citation is listed whether or not its page could be read: an unread one
/// still carries the provider's own description, which is often enough to cite.
pub fn prompt(question: &str, citations: &[Citation]) -> String {
    let mut out = String::with_capacity(PROMPT_BUDGET);
    out.push_str(
        "You have read several web pages. Answer the question from them, reading \
         across all of them rather than summarising each in turn.\n\n\
         Reply in exactly this shape and nothing else:\n\
         HEADLINE: six to ten words naming the answer, not the topic.\n\
         Then three to six lines, each one:\n\
         - {icon} **Label** — one sentence of detail. [n]\n\n\
         The label is two or three words naming what the line is about (Final \
         score, Who directed it, Cost). The detail is one sentence, specific, \
         with the number or name in it. Every line ends with the source numbers \
         it came from, like [2] or [1][4]. The numbers go at the end of the line \
         and nowhere else in it.\n\n\
         {icon} is one word from this list, in braces, naming what the line is \
         about. Pick the closest; use {unknown} if none fit:\n\
         score money time date place person group car chart up down warning \
         check cross question quote list star globe book tool fire shield \
         disagree unknown egg food health code music\n\n\
         Where a name in the detail is what one of the sources is about, write \
         it as [Name](n) so it opens that source. At most two per line, and \
         never around the whole sentence. n is a source number, never a URL.\n\n\
         When the answer has more than one part, group the lines under headings \
         written as `## Two or three words`. Use at most three headings, and \
         none at all when the answer is one group.\n\n\
         Where the sources disagree, say so in a line of its own with {disagree} \
         and name each reading with the source behind it. Where they do not \
         answer the question, say that in a line of its own with {unknown} \
         rather than filling it in from memory. No preamble, no closing line.\n\n",
    );
    out.push_str("Question: ");
    out.push_str(question.trim());
    out.push_str("\n\n");

    let share = PROMPT_BUDGET / citations.len().max(1);
    for (i, cited) in citations.iter().enumerate() {
        out.push_str(&format!("[{}] {}\n{}\n", i + 1, cited.hit.title, cited.hit.url));
        match &cited.text {
            Some(text) => {
                out.push_str(&text.chars().take(share).collect::<String>());
                out.push('\n');
            }
            None => {
                // Named rather than skipped, so the Agent can cite the snippet
                // and the user can see which pages were not readable.
                out.push_str("(page could not be read; provider summary only)\n");
                out.push_str(&cited.hit.description);
                out.push('\n');
            }
        }
        out.push('\n');
    }
    out
}

/// Which Agent answers: the first switched-on one, exactly as `!c` picks.
pub fn agent(prefs: &crate::prefs::Prefs) -> Option<crate::agents::AgentKind> {
    crate::agents::route(prefs).first().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(n: usize) -> Hit {
        Hit {
            title: format!("Source {n}"),
            url: format!("https://example.com/{n}"),
            description: format!("Snippet {n}."),
        }
    }

    #[test]
    fn v0_9_every_citation_is_numbered_from_one() {
        let cited = citations(
            vec![hit(1), hit(2)],
            vec![Ok(String::new()), Ok(String::new())],
        );
        let prompt = prompt("why", &cited);
        assert!(prompt.contains("[1] Source 1"));
        assert!(prompt.contains("[2] Source 2"));
        assert!(prompt.contains("https://example.com/2"));
    }

    /// A page that would not load is named in the prompt rather than dropped, or
    /// a thinner answer reads as a complete one (v0.9 Traps).
    #[test]
    fn v0_9_an_unreadable_page_stays_in_the_prompt_as_its_snippet() {
        let cited = citations(vec![hit(1)], vec![Err(SearchError::Failed("timeout".into()))]);
        assert!(cited[0].text.is_none());
        let prompt = prompt("why", &cited);
        assert!(prompt.contains("could not be read"));
        assert!(prompt.contains("Snippet 1."));
    }

    /// The question is in the prompt, or the Agent summarises the pages instead
    /// of answering anything.
    #[test]
    fn v0_9_the_question_is_carried_into_the_prompt() {
        let prompt = prompt("  ferrari in f1  ", &[]);
        assert!(prompt.contains("Question: ferrari in f1"));
    }

    /// Ten long pages must not become a six-figure prompt: that is a bill and a
    /// slow first token, and the budget is shared between citations.
    #[test]
    fn v0_9_the_prompt_is_bounded_however_long_the_pages_are() {
        let long = "word ".repeat(40_000);
        let hits: Vec<Hit> = (1..=10).map(hit).collect();
        let bodies: Vec<Result<String, SearchError>> = (0..10)
            .map(|_| Ok(format!("<article><p>{long}</p></article>")))
            .collect();
        let prompt = prompt("why", &citations(hits, bodies));
        assert!(
            prompt.chars().count() < PROMPT_BUDGET + 4_000,
            "prompt was {} chars",
            prompt.chars().count()
        );
    }

    /// The instruction that makes citations parseable. Without it the Agent
    /// writes prose with no way back to a URL.
    #[test]
    fn v0_9_the_prompt_asks_for_numbered_citations() {
        let prompt = prompt("why", &[]);
        assert!(prompt.contains("[1]"));
    }

    /// The answer shape is Arc Search's, not an essay: a headline, then labelled
    /// one-line findings. Prose paragraphs are what this replaced.
    #[test]
    fn v0_9_the_prompt_asks_for_a_headline_and_labelled_findings() {
        let prompt = prompt("what happened in the chiefs game", &[]);
        assert!(prompt.contains("HEADLINE:"), "no headline instruction");
        assert!(prompt.contains("**Label**"), "no labelled-bullet instruction");
        assert!(prompt.contains("- "), "no bullet instruction");
    }

    /// Several sources are read to be compared, not concatenated. Where they
    /// disagree the answer has to say so rather than picking one silently.
    #[test]
    fn v0_9_the_prompt_asks_the_sources_to_be_compared() {
        let prompt = prompt("who won", &[]);
        let lower = prompt.to_lowercase();
        assert!(lower.contains("disagree"), "nothing about conflicting sources");
        assert!(lower.contains("across"), "nothing about reading across sources");
    }

    /// `!s` asks whoever `!c` would ask, so a Codex-only machine still answers.
    #[test]
    fn v0_9_the_summariser_is_the_agent_c_would_have_asked() {
        let prefs = crate::prefs::Prefs::open(None).unwrap();
        prefs
            .set(crate::prefs::ASK_ORDER, r#"["codex","claude","opencode"]"#)
            .unwrap();
        assert_eq!(agent(&prefs), Some(crate::agents::AgentKind::Codex));
    }

    /// Every Agent switched off is the one state `!s` cannot synthesise in, and
    /// it says so rather than starting a Turn that cannot run.
    #[test]
    fn v0_9_no_switched_on_agent_means_no_summariser() {
        let prefs = crate::prefs::Prefs::open(None).unwrap();
        for kind in crate::agents::AgentKind::ALL {
            prefs.set(&crate::prefs::ask_enabled_key(kind), "0").unwrap();
        }
        assert_eq!(agent(&prefs), None);
    }
}
