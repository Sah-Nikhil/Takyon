//! Opening a URL, and handing a query to the browser (v0.9 task 6).
//!
//! Enter on `!s` opens the query in the **default browser with its own default
//! search engine**. There is no API for the second half — an engine lives inside
//! a browser profile — but every mainstream browser treats a non-URL argument as
//! a search with whatever engine the user chose, so the query is passed as an
//! argument to the browser `AssocQueryStringW` names.
//!
//! Falls back to the provider's results page when no browser can be resolved,
//! which is a portable answer rather than nothing happening.

use crate::entry::LaunchTarget;
use crate::launch;

/// Where a fallback query goes: the provider we already search through.
const FALLBACK_SEARCH: &str = "https://search.brave.com/search?q=";

/// Open one URL in the default browser.
///
/// http(s) only, checked here rather than trusted: URLs on this path come from
/// a remote provider, and `ShellExecuteW` on a `file:` or a custom scheme would
/// let a search result start a program.
pub fn open_url(url: &str) -> Result<(), String> {
    if super::fetch::parse_url(url).is_none() {
        return Err(format!("{url} is not a web address Takyon will open."));
    }
    launch::open(&LaunchTarget::Uri(url.to_string())).map(|_| ())
}

/// Open a query in the default browser, using that browser's own engine.
pub fn open_query(query: &str) -> Result<(), String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("Nothing to search for.".into());
    }
    match default_browser().filter(|_| is_plain_argument(query)) {
        Some(exe) => launch::open(&LaunchTarget::Exe {
            path: exe,
            args: Some(query.to_string()),
            working_dir: None,
        })
        .map(|_| ()),
        // No browser association is rare and recoverable: the provider's own
        // results page answers the same question through the shell's handler.
        None => open_url(&format!(
            "{FALLBACK_SEARCH}{}",
            super::fetch::percent_encode(query)
        )),
    }
}

/// Whether a query is safe to hand a browser as a bare argument.
///
/// A leading `-` is a flag, and Chromium's `--gpu-launcher` starts a process.
/// Quotes and control characters go the same way. Anything refused here is
/// still searched, through the fallback URL, where nothing reads as an option.
fn is_plain_argument(query: &str) -> bool {
    !query.starts_with('-')
        && !query.contains('"')
        && !query.contains('\'')
        && !query.chars().any(char::is_control)
}

/// The executable registered for `http`, or `None` when nothing is.
#[cfg(windows)]
fn default_browser() -> Option<std::path::PathBuf> {
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::UI::Shell::{AssocQueryStringW, ASSOCF_NONE, ASSOCSTR_EXECUTABLE};

    let scheme: Vec<u16> = "http\0".encode_utf16().collect();
    let mut len: u32 = 0;
    unsafe {
        // First call sizes the buffer; it reports failure by HRESULT and still
        // writes the length, so the result is deliberately not checked here.
        let _ = AssocQueryStringW(
            ASSOCF_NONE,
            ASSOCSTR_EXECUTABLE,
            PCWSTR(scheme.as_ptr()),
            PCWSTR::null(),
            None,
            &mut len,
        );
        if len == 0 {
            return None;
        }
        let mut buffer = vec![0u16; len as usize];
        AssocQueryStringW(
            ASSOCF_NONE,
            ASSOCSTR_EXECUTABLE,
            PCWSTR(scheme.as_ptr()),
            PCWSTR::null(),
            Some(PWSTR(buffer.as_mut_ptr())),
            &mut len,
        )
        .ok()
        .ok()?;
        let path = String::from_utf16_lossy(&buffer[..buffer.iter().position(|c| *c == 0)?]);
        let path = std::path::PathBuf::from(path);
        // Windows answers with a stub for "no association". A path that is not
        // there would open a dialog rather than a browser.
        path.is_file().then_some(path)
    }
}

#[cfg(not(windows))]
fn default_browser() -> Option<std::path::PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// URLs here arrive from a remote provider. Anything but http(s) is refused
    /// rather than handed to the shell, which would run a program.
    #[test]
    fn v0_9_only_http_urls_are_opened() {
        for url in [
            "file:///C:/Windows/System32/cmd.exe",
            "javascript:alert(1)",
            "steam://run/570",
            "ms-settings:bluetooth",
            "",
        ] {
            assert!(open_url(url).is_err(), "{url} was not refused");
        }
    }

    /// A query that could be read as a command-line flag never becomes one.
    /// Chromium's `--gpu-launcher` starts an arbitrary process; the fallback URL
    /// is inert, so a refused query is still searched.
    #[test]
    fn v0_9_a_query_that_looks_like_a_flag_is_never_passed_as_an_argument() {
        for hostile in [
            "--gpu-launcher=calc.exe",
            "-remote-debugging-port=9222",
            "ferrari\" --gpu-launcher=calc",
            "ferrari' --load-extension=x",
            "ferrari\nmore",
        ] {
            assert!(!is_plain_argument(hostile), "{hostile:?} was accepted");
        }
        // An ordinary question still reaches the browser's own engine.
        assert!(is_plain_argument("ferrari in f1"));
        assert!(is_plain_argument("what is 2 + 2 & why"));
    }

    /// An empty query must not open a browser at all: Enter on a bare `!s` is a
    /// keystroke, not a request.
    #[test]
    fn v0_9_an_empty_query_opens_nothing() {
        assert!(open_query("   ").is_err());
    }

    /// The fallback is a real URL that the URL guard itself accepts, or the two
    /// halves of this file disagree the day no browser is registered.
    #[test]
    fn v0_9_the_fallback_search_url_is_one_takyon_would_open() {
        let url = format!("{FALLBACK_SEARCH}{}", super::super::fetch::percent_encode("a b"));
        assert_eq!(url, "https://search.brave.com/search?q=a%20b");
        assert!(super::super::fetch::parse_url(&url).is_some());
    }
}
