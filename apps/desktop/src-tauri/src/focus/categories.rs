//! Built-in Focus categories, from `categories.json`, plus the user's edits.
//!
//! Adding one is a JSON edit; the schema test below turns a typo into a
//! failed build rather than a category that silently blocks nothing. A user
//! adds to or removes from a built-in through [`Edit`]s in `focus.db`, laid
//! over the shipped list by [`with_edits`], so a later `categories.json` still
//! reaches everything the user did not touch.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::site::domain_of;

/// One Focus category.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    pub name: String,
    /// Phosphor icon name, drawn by the frontend.
    pub icon: String,
    /// `games`: items plus every installed game, resolved at session start.
    #[serde(default)]
    pub dynamic: Option<String>,
    /// Built-in id this custom category was duplicated from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copied_from: Option<String>,
    pub items: Vec<Item>,
    /// Built-ins: keys the user added, also in `items`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<String>,
    /// Built-ins: shipped items (or detected games) the user took out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<Item>,
}

/// One user change to a built-in Focus category, as `focus.db` stores it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub category: String,
    pub key: String,
    pub label: String,
    /// True: take `key` out. False: add it.
    pub removed: bool,
}

/// The built-ins with the user's edits laid over them, shipped order kept.
pub fn with_edits(builtins: &[Category], edits: &[Edit]) -> Vec<Category> {
    builtins
        .iter()
        .map(|c| {
            let mut c = c.clone();
            for e in edits.iter().filter(|e| e.category == c.id) {
                let item = Item {
                    key: e.key.clone(),
                    label: e.label.clone(),
                };
                if e.removed {
                    c.items.retain(|i| i.key != e.key);
                    c.removed.push(item);
                } else if !c.items.iter().any(|i| i.key == e.key) {
                    c.added.push(e.key.clone());
                    c.items.push(item);
                }
            }
            c
        })
        .collect()
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Item {
    /// `site:`, `exe:` or `package:`; never `entry:`, whose paths are per machine.
    pub key: String,
    pub label: String,
}

/// The built-ins. Parsed once.
pub fn builtins() -> &'static [Category] {
    static PARSED: OnceLock<Vec<Category>> = OnceLock::new();
    PARSED.get_or_init(|| {
        let file: File = serde_json::from_str(include_str!("categories.json"))
            .expect("categories.json is checked by its schema test");
        file.categories
    })
}

#[derive(Deserialize)]
struct File {
    categories: Vec<Category>,
}

/// Why a built-in or imported item key is malformed, or None if it is fine.
///
/// Keys are stored the way matching compares them: lowercase, `site:` already
/// a [`domain_of`] result, `package:` a family name (`<name>_<13-char id>`).
pub fn schema_error(key: &str) -> Option<String> {
    let lower = key == key.to_lowercase();
    let problem = match key.split_once(':') {
        Some(("site", domain)) => (domain_of(domain).as_deref() != Some(domain))
            .then(|| format!("not a bare lowercase domain; want {:?}", domain_of(domain))),
        Some(("exe", name)) => {
            (!lower || !name.ends_with(".exe") || name.len() <= 4 || name.contains(['\\', '/']))
                .then(|| "want a lowercase image name ending .exe".to_string())
        }
        Some(("package", family)) => {
            let id = family.rsplit_once('_').map(|(_, id)| id).unwrap_or("");
            let id_ok = id.len() == 13 && id.bytes().all(|b| b.is_ascii_alphanumeric());
            (!lower || !id_ok).then(|| "want a lowercase package family name".to_string())
        }
        _ => Some("want site:, exe: or package:".to_string()),
    };
    problem.map(|p| format!("{key}: {p}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_16_the_six_built_ins_ship() {
        let ids: Vec<&str> = builtins().iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            ids,
            ["social", "messaging", "video", "news", "shopping", "games"]
        );
    }

    /// Every item must be something `identity::resolve` or `site::matches`
    /// can act on, written the way they compare it.
    #[test]
    fn v0_16_every_built_in_item_is_well_formed() {
        for category in builtins() {
            assert!(
                !category.name.is_empty() && !category.icon.is_empty(),
                "{}",
                category.id
            );
            assert!(!category.items.is_empty(), "{} is empty", category.id);
            let mut keys = std::collections::HashSet::new();
            for item in &category.items {
                let at = format!("{} / {}", category.id, item.key);
                assert!(keys.insert(&item.key), "duplicate {at}");
                assert!(!item.label.trim().is_empty(), "no label: {at}");
                assert_eq!(schema_error(&item.key), None, "{at}");
            }
        }
    }

    #[test]
    fn v0_16_the_schema_check_catches_each_kind_of_typo() {
        assert_eq!(schema_error("site:youtube.com"), None);
        assert_eq!(schema_error("exe:discord.exe"), None);
        assert_eq!(
            schema_error("package:5319275a.whatsappdesktop_cv1g1gvanyjgm"),
            None
        );
        assert!(schema_error("site:www.youtube.com").is_some());
        assert!(schema_error("site:https://youtube.com").is_some());
        assert!(schema_error("site:YouTube.com").is_some());
        assert!(schema_error("exe:Discord.exe").is_some());
        assert!(schema_error("exe:discord").is_some());
        assert!(schema_error("package:Foo_bar").is_some());
        assert!(schema_error("package:foo").is_some());
        assert!(schema_error(r"entry:c:\x.exe").is_some());
        assert!(schema_error("youtube.com").is_some());
    }

    fn edit(category: &str, key: &str, label: &str, removed: bool) -> Edit {
        Edit {
            category: category.into(),
            key: key.into(),
            label: label.into(),
            removed,
        }
    }

    /// A user's additions land after the defaults; removals leave the list
    /// and are kept aside, labelled, so Settings can offer them back.
    #[test]
    fn v0_16_edits_add_to_and_remove_from_a_built_in() {
        let edits = [
            edit("social", "exe:mastodon.exe", "Mastodon", false),
            edit("social", "site:linkedin.com", "LinkedIn", true),
            edit("games", "entry:steam:730", "Counter-Strike 2", true),
            edit("nonexistent", "site:x.com", "X", false),
        ];
        let merged = with_edits(builtins(), &edits);
        let social = merged.iter().find(|c| c.id == "social").unwrap();
        assert_eq!(social.items.last().unwrap().key, "exe:mastodon.exe");
        assert!(!social.items.iter().any(|i| i.key == "site:linkedin.com"));
        assert_eq!(social.added, ["exe:mastodon.exe"]);
        assert_eq!(
            social.removed,
            [Item {
                key: "site:linkedin.com".into(),
                label: "LinkedIn".into()
            }]
        );
        let games = merged.iter().find(|c| c.id == "games").unwrap();
        assert_eq!(games.removed[0].key, "entry:steam:730");
        let video = merged.iter().find(|c| c.id == "video").unwrap();
        assert!(video.added.is_empty() && video.removed.is_empty());
        assert_eq!(merged.len(), builtins().len());
    }

    /// Adding a default back is a no-op, not a duplicate.
    #[test]
    fn v0_16_adding_an_item_a_built_in_already_has_changes_nothing() {
        let merged = with_edits(builtins(), &[edit("social", "site:x.com", "X", false)]);
        let social = merged.iter().find(|c| c.id == "social").unwrap();
        assert_eq!(
            social
                .items
                .iter()
                .filter(|i| i.key == "site:x.com")
                .count(),
            1
        );
        assert!(social.added.is_empty());
    }

    #[test]
    fn v0_16_only_games_is_dynamic() {
        for category in builtins() {
            let want = (category.id == "games").then(|| "games".to_string());
            assert_eq!(category.dynamic, want, "{}", category.id);
        }
    }
}
