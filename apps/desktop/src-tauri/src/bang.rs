//! Bang parsing (IMPLEMENTATION_PLAN §9).
//!
//! ```text
//! line := bang? rest
//! bang := '!' ident (WS | EOL)
//! ```
//!
//! Position 0 only, and a Bang consumes the whole line: the rest is that Mode's
//! raw query, never a ranked search. That is what makes ADR-0002 checkable by
//! reading — a line with no Bang cannot reach the network.
//!
//! One Bang at v0.5. Registry, `!` picker and user-defined Bangs are
//! `docs/plans/bang-registry.md`, which resumes at v0.8.

/// Where a line of input goes, and what that Mode sees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route<'a> {
    /// No Bang, an unknown one, or a bare `!`. The whole line, matched by Sources.
    Bangless(&'a str),
    /// `!v` — clipboard history. The rest is a substring search, or empty for the
    /// full list.
    Clips(&'a str),
}

/// The clipboard Bang. One letter, and `v` because `Ctrl+V` is what it replaces.
pub const CLIPS: &str = "v";

/// Route one line of input.
pub fn parse(line: &str) -> Route<'_> {
    let Some(after_sigil) = line.strip_prefix('!') else {
        return Route::Bangless(line);
    };

    // The ident ends at the first whitespace. A bare `!` yields an empty one,
    // which matches nothing and falls through — the picker is v0.8.
    let end = after_sigil
        .find(char::is_whitespace)
        .unwrap_or(after_sigil.len());
    let (ident, rest) = after_sigil.split_at(end);

    match ident.to_lowercase().as_str() {
        CLIPS => Route::Clips(rest.trim()),
        // Unknown Bang is treated literally rather than rejected (§9). A hint row
        // saying so is part of the registry work, not of this parser.
        _ => Route::Bangless(line),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_5_a_bang_alone_selects_its_mode_with_an_empty_query() {
        assert_eq!(parse("!v"), Route::Clips(""));
        assert_eq!(parse("!v "), Route::Clips(""));
    }

    #[test]
    fn v0_5_the_rest_of_the_line_is_the_modes_raw_query() {
        assert_eq!(parse("!v api key"), Route::Clips("api key"));
        assert_eq!(parse("!v   spaced  "), Route::Clips("spaced"));
    }

    /// §9: position 0 only. Anything else is text that happens to contain a `!`.
    #[test]
    fn v0_5_a_bang_anywhere_but_the_start_is_not_a_bang() {
        assert_eq!(parse(" !v thing"), Route::Bangless(" !v thing"));
        assert_eq!(parse("git commit !v"), Route::Bangless("git commit !v"));
    }

    /// The ident has to end at whitespace, so `!video` is its own (unknown) Bang
    /// rather than `!v` with a query of `ideo`.
    #[test]
    fn v0_5_the_ident_ends_at_whitespace_not_at_a_known_prefix() {
        assert_eq!(parse("!video"), Route::Bangless("!video"));
    }

    /// Unknown Bangs fall through with the line intact (§9), so nothing is lost
    /// while the registry is still parked.
    #[test]
    fn v0_5_an_unknown_bang_falls_through_to_bangless() {
        assert_eq!(parse("!s ferrari"), Route::Bangless("!s ferrari"));
        assert_eq!(parse("!"), Route::Bangless("!"));
        assert_eq!(parse("!!"), Route::Bangless("!!"));
    }

    #[test]
    fn v0_5_a_bang_is_case_insensitive() {
        assert_eq!(parse("!V token"), Route::Clips("token"));
    }

    #[test]
    fn v0_5_an_ordinary_query_is_untouched() {
        assert_eq!(parse("code"), Route::Bangless("code"));
        assert_eq!(parse(""), Route::Bangless(""));
    }
}
