//! What a Focus category holds, as Settings lists it: each item marked
//! installed or not against the app list, plus Games' detected games.
//!
//! Pure: [`Known`] apps come in from the apps Source. Matching mirrors
//! `identity::resolve`, against discovery's launch targets rather than a
//! running process.

use serde::Serialize;

use super::categories::Category;

/// One app the apps Source found, reduced to what matching needs.
#[derive(Clone, Debug, Default)]
pub struct Known {
    /// `EntryId`: lowercased path, `aumid:…`, or `steam:730`.
    pub id: String,
    pub title: String,
    /// Lowercased image name of an `Exe` target.
    pub exe: Option<String>,
    pub aumid: Option<String>,
    /// Found through a Game launcher.
    pub game: bool,
    pub icon: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub key: String,
    pub label: String,
    /// `site` or `app`.
    pub kind: &'static str,
    /// Apps only: found on this machine.
    pub installed: bool,
    pub icon: Option<String>,
    /// Built-ins: the user put it there.
    pub added: bool,
    /// Games: found through a launcher rather than listed.
    pub detected: bool,
}

/// Items in order, then (Games only) installed games not taken out.
pub fn members(category: &Category, known: &[Known]) -> Vec<Member> {
    let mut out: Vec<Member> = category
        .items
        .iter()
        .map(|item| {
            let found = if item.key.starts_with("site:") {
                None
            } else {
                known.iter().find(|k| is(&item.key, k))
            };
            Member {
                key: item.key.clone(),
                label: item.label.clone(),
                kind: if item.key.starts_with("site:") {
                    "site"
                } else {
                    "app"
                },
                installed: found.is_some(),
                icon: found.and_then(|k| k.icon.clone()),
                added: category.added.contains(&item.key),
                detected: false,
            }
        })
        .collect();
    if category.dynamic.as_deref() == Some("games") {
        for game in known.iter().filter(|k| k.game) {
            let key = format!("entry:{}", game.id);
            if category.removed.iter().any(|r| r.key == key) || out.iter().any(|m| m.key == key) {
                continue;
            }
            out.push(Member {
                key,
                label: game.title.clone(),
                kind: "app",
                installed: true,
                icon: game.icon.clone(),
                added: false,
                detected: true,
            });
        }
    }
    out
}

/// Does Block list `key` name this installed app?
fn is(key: &str, app: &Known) -> bool {
    match key.split_once(':') {
        Some(("exe", name)) => app.exe.as_deref() == Some(name),
        Some(("package", family)) => app.aumid.as_deref().is_some_and(|a| {
            a.to_lowercase()
                .strip_prefix(family)
                .is_some_and(|rest| rest.starts_with('!'))
        }),
        Some(("entry", id)) => app.id == id,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::focus::categories::Item;

    fn item(key: &str, label: &str) -> Item {
        Item {
            key: key.into(),
            label: label.into(),
        }
    }

    fn known() -> Vec<Known> {
        vec![
            Known {
                id: r"c:\discord\discord.exe".into(),
                title: "Discord".into(),
                exe: Some("discord.exe".into()),
                icon: Some("i1".into()),
                ..Default::default()
            },
            Known {
                id: "aumid:5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App".into(),
                title: "WhatsApp".into(),
                aumid: Some("5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App".into()),
                icon: Some("i2".into()),
                ..Default::default()
            },
            Known {
                id: "steam:730".into(),
                title: "Counter-Strike 2".into(),
                game: true,
                icon: Some("i3".into()),
                ..Default::default()
            },
            Known {
                id: "steam:570".into(),
                title: "Dota 2".into(),
                game: true,
                ..Default::default()
            },
        ]
    }

    #[test]
    fn v0_16_each_item_is_marked_installed_or_not_with_its_icon() {
        let category = Category {
            id: "messaging".into(),
            items: vec![
                item("exe:discord.exe", "Discord"),
                item("site:discord.com", "Discord"),
                item("package:5319275a.whatsappdesktop_cv1g1gvanyjgm", "WhatsApp"),
                item("exe:signal.exe", "Signal"),
            ],
            added: vec!["exe:signal.exe".into()],
            ..Default::default()
        };
        let got: Vec<_> = members(&category, &known())
            .into_iter()
            .map(|m| (m.label, m.kind, m.installed, m.icon, m.added))
            .collect();
        let row = |label: &str, kind, installed, icon: Option<&str>, added| {
            (
                label.to_string(),
                kind,
                installed,
                icon.map(String::from),
                added,
            )
        };
        assert_eq!(
            got,
            [
                row("Discord", "app", true, Some("i1"), false),
                row("Discord", "site", false, None, false),
                row("WhatsApp", "app", true, Some("i2"), false),
                row("Signal", "app", false, None, true),
            ]
        );
    }

    /// Games lists what launchers found, minus what the user took out.
    #[test]
    fn v0_16_games_lists_detected_games_except_removed_ones() {
        let category = Category {
            id: "games".into(),
            dynamic: Some("games".into()),
            items: vec![item("exe:steam.exe", "Steam")],
            removed: vec![item("entry:steam:570", "Dota 2")],
            ..Default::default()
        };
        let got: Vec<(String, bool)> = members(&category, &known())
            .into_iter()
            .map(|m| (m.label, m.detected))
            .collect();
        assert_eq!(
            got,
            [
                ("Steam".to_string(), false),
                ("Counter-Strike 2".to_string(), true)
            ]
        );
    }

    #[test]
    fn v0_16_an_aumid_from_another_package_is_not_a_match() {
        let other = Known {
            aumid: Some("5319275A.WhatsAppDesktopBeta_cv1g1gvanyjgm!App".into()),
            ..Default::default()
        };
        assert!(!is(
            "package:5319275a.whatsappdesktop_cv1g1gvanyjgm",
            &other
        ));
    }
}
