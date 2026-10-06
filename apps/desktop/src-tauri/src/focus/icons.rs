//! Website icons for Focus (ADR-0037): the built-in sites' once, on first
//! launch, and a site's own when the user adds it. Nothing else fetches.
//!
//! Same cache and scheme as `!s` (`search::favicon`), so a host fetched for
//! either is drawn by both. Root `favicon.ico` first; the home page's declared
//! icon only when the root has none.

use std::path::Path;

use crate::search::{favicon, fetch};

/// Domains among Block list keys: `site:x.com` gives `x.com`.
pub fn hosts_of<'a>(keys: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = keys
        .into_iter()
        .filter_map(|k| k.strip_prefix("site:"))
        .map(str::to_string)
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Fetch and cache each host not already on disk, in parallel, best effort.
/// Returns how many were fetched now.
pub fn fetch_missing(dir: &Path, hosts: &[String]) -> usize {
    use rayon::prelude::*;
    hosts
        .par_iter()
        .filter(|host| favicon::cached(dir, host).is_none())
        .filter(|host| {
            let home = format!("https://{host}/");
            let bytes = favicon::fetch_one(&home, None).or_else(|| {
                let page = fetch::get_url(&home).ok()?;
                favicon::fetch_one(&home, Some(&page.body))
            });
            bytes.is_some_and(|b| favicon::store(dir, host, &b).is_ok())
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_16_only_site_keys_name_hosts_once_each() {
        assert_eq!(
            hosts_of([
                "site:youtube.com",
                "exe:discord.exe",
                "site:x.com",
                "site:youtube.com",
                "entry:steam:730",
            ]),
            ["x.com", "youtube.com"]
        );
    }
}
