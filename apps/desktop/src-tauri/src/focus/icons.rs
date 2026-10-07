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
    fetch_missing_with(dir, hosts, |host| {
        let home = format!("https://{host}/");
        favicon::fetch_one(&home, None).or_else(|| {
            let page = fetch::get_url(&home).ok()?;
            favicon::fetch_one(&home, Some(&page.body))
        })
    })
}

/// [`fetch_missing`] with the fetch injected, so a test needs no network.
pub fn fetch_missing_with(
    dir: &Path,
    hosts: &[String],
    fetch: impl Fn(&str) -> Option<Vec<u8>> + Sync,
) -> usize {
    use rayon::prelude::*;
    fetch::pool().install(|| {
        hosts
            .par_iter()
            .filter(|host| favicon::cached(dir, host).is_none())
            .filter(|host| fetch(host).is_some_and(|b| favicon::store(dir, host, &b).is_ok()))
            .count()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CI's bench hung at show 19/30: sixty slow fetches had every thread of
    /// rayon's global pool, which the keystroke fan-out shares. A fetch must
    /// never cost a keystroke anything.
    #[test]
    fn v0_17_slow_fetches_leave_the_keystroke_pool_free() {
        let dir = std::env::temp_dir().join("takyon-icons-pool-test");
        let hosts: Vec<String> = (0..64).map(|i| format!("h{i}.test")).collect();
        let slow = std::thread::spawn(move || {
            fetch_missing_with(&dir, &hosts, |_| {
                std::thread::sleep(std::time::Duration::from_millis(400));
                None
            })
        });
        std::thread::sleep(std::time::Duration::from_millis(100));

        use rayon::prelude::*;
        let started = std::time::Instant::now();
        let sum: u64 = (0..64u64).into_par_iter().sum();
        let waited = started.elapsed();
        assert_eq!(sum, 2016);
        assert!(
            waited < std::time::Duration::from_millis(150),
            "keystroke pool waited {waited:?}"
        );
        assert_eq!(slow.join().unwrap(), 0);
    }

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
