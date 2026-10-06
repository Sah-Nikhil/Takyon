//! A session's Block list resolved against this machine, and the per-tick
//! verdicts the enforcer acts on. Pure: processes and address bars come in.

use std::path::{Path, PathBuf};

use super::categories::Category;
use super::identity::{resolve, Process, Target};
use super::procs::Running;
use super::protected::{is_protected, refuses};
use super::session::Snoozes;
use super::site::{domain_of, host_of, matches};
use super::store::BlockList;

/// What one session blocks, ready to match.
#[derive(Clone, Debug, Default)]
pub struct Rules {
    apps: Vec<(String, Target)>,
    sites: Vec<(String, String)>,
}

/// Inputs to [`Rules::build`] that depend on the machine.
pub struct Machine<'a> {
    pub builtins: &'a [Category],
    pub custom: &'a [Category],
    /// `EntryId`s of installed games, for the `games` Focus category.
    pub games: &'a [String],
    pub game_dir: &'a dyn Fn(&str, &str) -> Option<PathBuf>,
    pub takyon_dir: &'a Path,
}

impl Rules {
    /// Expand Focus categories, dedupe, and split off keys that would hit
    /// something protected. Second value: refused keys, for the start form.
    pub fn build(list: &BlockList, machine: &Machine) -> (Rules, Vec<String>) {
        let mut wanted: Vec<String> = Vec::new();
        for id in &list.categories {
            let found = machine
                .builtins
                .iter()
                .chain(machine.custom)
                .find(|c| &c.id == id);
            let Some(category) = found else { continue };
            wanted.extend(category.items.iter().map(|i| i.key.clone()));
            if category.dynamic.as_deref() == Some("games") {
                wanted.extend(machine.games.iter().map(|g| format!("entry:{g}")));
            }
        }
        wanted.extend(list.items.iter().cloned());

        let mut rules = Rules::default();
        let mut refused = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for key in wanted {
            if !seen.insert(key.clone()) {
                continue;
            }
            if let Some(domain) = key.strip_prefix("site:").and_then(domain_of) {
                rules.sites.push((key, domain));
            } else if let Some(target) = resolve(&key, machine.game_dir) {
                if refuses(&target, machine.takyon_dir) {
                    refused.push(key);
                } else {
                    rules.apps.push((key, target));
                }
            }
        }
        (rules, refused)
    }

    /// Processes to quit now, with the key that blocked each. Snoozed and
    /// protected ones spared; `foreground` holds an expired snooze open.
    pub fn to_quit(
        &self,
        procs: &[Running],
        snoozes: &mut Snoozes,
        foreground: Option<u32>,
        now: u64,
        takyon_dir: &Path,
    ) -> Vec<(u32, String)> {
        procs
            .iter()
            .filter(|r| !is_protected(&r.process, takyon_dir))
            .filter_map(|r| {
                let key = self.app_key(&r.process)?;
                let spared = snoozes.allows(key, now, foreground == Some(r.pid));
                (!spared).then(|| (r.pid, key.to_string()))
            })
            .collect()
    }

    fn app_key(&self, process: &Process) -> Option<&str> {
        self.apps
            .iter()
            .find(|(_, t)| t.matches(process))
            .map(|(k, _)| k.as_str())
    }

    /// Key of the blocked site the address bar shows, unless snoozed. The
    /// block page has no host, so it is never blocked again.
    pub fn site_block(&self, display: &str, snoozes: &mut Snoozes, now: u64) -> Option<String> {
        let host = host_of(display)?;
        let (key, _) = self
            .sites
            .iter()
            .find(|(_, domain)| matches(&host, domain))?;
        (!snoozes.allows(key, now, true)).then(|| key.clone())
    }

    pub fn is_empty(&self) -> bool {
        self.apps.is_empty() && self.sites.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::focus::categories::Item;

    fn cat(id: &str, keys: &[&str]) -> Category {
        Category {
            id: id.into(),
            name: id.into(),
            icon: "x".into(),
            dynamic: (id == "games").then(|| "games".into()),
            copied_from: None,
            items: keys
                .iter()
                .map(|k| Item {
                    key: (*k).into(),
                    label: (*k).into(),
                })
                .collect(),
        }
    }

    fn steam(launcher: &str, id: &str) -> Option<PathBuf> {
        (launcher == "steam" && id == "730").then(|| PathBuf::from(r"D:\Steam\common\CS"))
    }

    fn build(list: BlockList) -> (Rules, Vec<String>) {
        let builtins = [
            cat("social", &["site:x.com", "exe:tiktok.exe"]),
            cat("games", &["exe:steam.exe"]),
        ];
        let custom = [cat("custom:1", &["site:reddit.com", "site:x.com"])];
        let games = ["steam:730".to_string(), "steam:999".to_string()];
        let machine = Machine {
            builtins: &builtins,
            custom: &custom,
            games: &games,
            game_dir: &steam,
            takyon_dir: Path::new(r"D:\Takyon"),
        };
        Rules::build(&list, &machine)
    }

    fn keys(rules: &Rules) -> Vec<&str> {
        let mut out: Vec<&str> = rules
            .apps
            .iter()
            .map(|(k, _)| k.as_str())
            .chain(rules.sites.iter().map(|(k, _)| k.as_str()))
            .collect();
        out.sort();
        out
    }

    #[test]
    fn v0_16_categories_expand_and_keys_dedupe() {
        let (rules, refused) = build(BlockList {
            categories: vec!["social".into(), "custom:1".into(), "nonexistent".into()],
            items: vec!["site:x.com".into(), "exe:discord.exe".into()],
        });
        assert_eq!(
            keys(&rules),
            [
                "exe:discord.exe",
                "exe:tiktok.exe",
                "site:reddit.com",
                "site:x.com"
            ]
        );
        assert!(refused.is_empty());
    }

    /// Games = the category's launchers plus every installed game that still
    /// resolves to an install directory.
    #[test]
    fn v0_16_the_games_category_adds_installed_games() {
        let (rules, _) = build(BlockList {
            categories: vec!["games".into()],
            items: vec![],
        });
        assert_eq!(keys(&rules), ["entry:steam:730", "exe:steam.exe"]);
    }

    #[test]
    fn v0_16_protected_keys_are_refused_not_blocked() {
        let (rules, refused) = build(BlockList {
            categories: vec![],
            items: vec![
                "exe:explorer.exe".into(),
                r"entry:d:\takyon\takyon.exe".into(),
                "exe:slack.exe".into(),
            ],
        });
        assert_eq!(keys(&rules), ["exe:slack.exe"]);
        assert_eq!(refused, ["exe:explorer.exe", r"entry:d:\takyon\takyon.exe"]);
    }

    const T0: u64 = 1_760_000_000_000;

    fn running(pid: u32, image: &str, session_id: u32) -> Running {
        Running {
            pid,
            process: Process {
                image: image.into(),
                session_id,
                ..Default::default()
            },
        }
    }

    fn discord_rules() -> Rules {
        build(BlockList {
            categories: vec![],
            items: vec!["exe:discord.exe".into(), "exe:explorer.exe".into()],
        })
        .0
    }

    #[test]
    fn v0_16_blocked_processes_are_quit_and_everything_else_is_left_alone() {
        let procs = [
            running(10, r"D:\Discord\Discord.exe", 1),
            running(11, r"D:\Discord\Discord.exe", 1),
            running(12, r"D:\Slack\slack.exe", 1),
            running(13, r"C:\Windows\explorer.exe", 1),
        ];
        let mut snoozes = Snoozes::default();
        let quit = discord_rules().to_quit(&procs, &mut snoozes, None, T0, Path::new(r"D:\Takyon"));
        assert_eq!(
            quit,
            [
                (10, "exe:discord.exe".to_string()),
                (11, "exe:discord.exe".to_string())
            ]
        );
    }

    /// Snoozed apps stay; an expired snooze holds only while its app is in front.
    #[test]
    fn v0_16_snoozed_apps_are_spared() {
        let procs = [running(10, r"D:\Discord\Discord.exe", 1)];
        let rules = discord_rules();
        let mut snoozes = Snoozes::default();
        snoozes.snooze("exe:discord.exe", T0, 60_000);
        let takyon = Path::new(r"D:\Takyon");
        assert!(rules
            .to_quit(&procs, &mut snoozes, None, T0 + 1, takyon)
            .is_empty());
        assert!(rules
            .to_quit(&procs, &mut snoozes, Some(10), T0 + 120_000, takyon)
            .is_empty());
        assert_eq!(
            rules
                .to_quit(&procs, &mut snoozes, None, T0 + 121_000, takyon)
                .len(),
            1
        );
    }

    #[test]
    fn v0_16_a_blocked_site_is_named_by_its_key() {
        let (rules, _) = build(BlockList {
            categories: vec![],
            items: vec!["site:youtube.com".into()],
        });
        let mut snoozes = Snoozes::default();
        assert_eq!(
            rules.site_block("m.youtube.com/watch?v=1", &mut snoozes, T0),
            Some("site:youtube.com".into())
        );
        assert_eq!(
            rules.site_block("https://notyoutube.com", &mut snoozes, T0),
            None
        );
        assert_eq!(
            rules.site_block(
                "C:/x/focus/blocked.html?u=https%3A%2F%2Fyoutube.com",
                &mut snoozes,
                T0
            ),
            None
        );
        snoozes.snooze("site:youtube.com", T0, 60_000);
        assert_eq!(rules.site_block("youtube.com", &mut snoozes, T0 + 1), None);
    }

    #[test]
    fn v0_16_an_empty_list_blocks_nothing() {
        assert!(build(BlockList::default()).0.is_empty());
    }
}
