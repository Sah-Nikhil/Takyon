//! `Ctrl+K` action menu (ROADMAP v0.2). Shared primitive: every Source and Mode
//! contributes to it, so it is built with one Source rather than retrofitted
//! across three.
//!
//! Labels and accelerators live here, not in the query response — an Entry
//! carries [`ActionId`]s only. Accelerators are a table, not a `match`, so v0.6
//! rebinding changes data. Storage for that needs `settings.db`.

use crate::entry::{Action, ActionId, Entry, EntryKind};

/// Launch it. The Enter key, and every Entry has it.
pub const OPEN: ActionId = ActionId("open");
/// Launch elevated, via the `runas` shell verb.
pub const RUN_AS_ADMIN: ActionId = ActionId("run_as_admin");
/// Open Explorer with the target selected.
pub const REVEAL: ActionId = ActionId("reveal");
/// Copy the resolved target path to the clipboard.
pub const COPY_PATH: ActionId = ActionId("copy_path");

/// Every action, with label and default accelerator.
///
/// `Enter` is listed for [`OPEN`] though the Palette handles it directly: a blank
/// accelerator on the one action users know reads as broken, not intentional.
const TABLE: &[(ActionId, &str, Option<&str>)] = &[
    (OPEN, "Open", Some("Enter")),
    (RUN_AS_ADMIN, "Run as administrator", Some("Ctrl+Enter")),
    (REVEAL, "Open file location", Some("Ctrl+Shift+Enter")),
    (COPY_PATH, "Copy path", Some("Ctrl+Shift+C")),
];

/// Resolve an id into a menu row.
///
/// `None` for an unknown id, never a placeholder row — that would hide a Source
/// bug behind something that looks deliberate.
pub fn describe(id: &ActionId) -> Option<Action> {
    TABLE.iter().find(|(aid, _, _)| aid == id).map(|(aid, label, accel)| Action {
        id: aid.clone(),
        label: (*label).to_string(),
        accelerator: accel.map(|a| a.to_string()),
    })
}

/// Menu contents for one Entry, in draw order.
///
/// Order comes from the Entry, not [`TABLE`]. The Source knows which action is
/// wanted; the table only knows how to spell them.
pub fn for_entry(entry: &Entry) -> Vec<Action> {
    entry.actions.iter().filter_map(describe).collect()
}

/// Default actions for an application Entry.
///
/// Manual verification step 6 requires Run as administrator and Open file
/// location, so neither is optional.
pub fn for_app(has_path: bool) -> Vec<ActionId> {
    // A UWP app has an AUMID, not a path: nothing to elevate, reveal or copy.
    // A menu item that can only fail teaches users the menu lies.
    if has_path {
        vec![OPEN, RUN_AS_ADMIN, REVEAL, COPY_PATH]
    } else {
        vec![OPEN]
    }
}

/// Default actions for a document or folder (v0.3, Recents).
///
/// No **Run as administrator**: elevating a `.docx` is not a thing, and an
/// action that can only fail teaches users the menu lies — the same rule
/// [`for_app`] applies to a packaged app with no file.
pub fn for_file() -> Vec<ActionId> {
    vec![OPEN, REVEAL, COPY_PATH]
}

/// Which action a modifier combination triggers on Enter.
///
/// The only definition of what `Ctrl+Enter` means. v0.6 rebinding changes this
/// function's data, not its callers.
pub fn for_modifiers(ctrl: bool, shift: bool) -> ActionId {
    match (ctrl, shift) {
        (true, true) => REVEAL,
        (true, false) => RUN_AS_ADMIN,
        _ => OPEN,
    }
}

/// Actions that make no sense for a kind. Near no-op at v0.2, which only produces
/// [`EntryKind::App`].
///
/// Exists for one dangerous case (ADR-0006): a Clip offering "Open file location"
/// would leak clipboard content into Explorer as a path.
pub fn permitted(kind: EntryKind, id: &ActionId) -> bool {
    match kind {
        EntryKind::Clip => id == &COPY_PATH || id == &OPEN,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::EntryId;

    fn app_entry(actions: Vec<ActionId>) -> Entry {
        Entry {
            id: EntryId(r"c:\app\thing.exe".into()),
            title: "Thing".into(),
            subtitle: None,
            kind: EntryKind::App,
            icon: None,
            score: 800.0,
            actions,
        }
    }

    /// Manual verification step 6, as a unit test — the cheaper place to fail.
    #[test]
    fn v0_2_ctrl_k_on_an_app_offers_run_as_admin_and_reveal() {
        let menu = for_entry(&app_entry(for_app(true)));
        let labels: Vec<&str> = menu.iter().map(|a| a.label.as_str()).collect();
        assert!(labels.contains(&"Run as administrator"));
        assert!(labels.contains(&"Open file location"));
    }

    /// A UWP app has no file. Every menu item must be something that can happen.
    #[test]
    fn v0_2_a_uwp_app_is_not_offered_actions_that_need_a_path() {
        let menu = for_entry(&app_entry(for_app(false)));
        assert_eq!(menu.len(), 1);
        assert_eq!(menu[0].id, OPEN);
    }

    /// Task 9: accelerators listed inside the menu. A shortcut documented only in
    /// a changelog is folklore.
    #[test]
    fn v0_2_every_action_in_the_menu_shows_its_accelerator() {
        for action in for_entry(&app_entry(for_app(true))) {
            assert!(
                action.accelerator.is_some(),
                "{} has no accelerator to show",
                action.label
            );
        }
    }

    #[test]
    fn v0_2_no_two_actions_share_an_accelerator() {
        // One chord, two actions: one is unreachable, and which one depends on
        // match order. Invisible in review, maddening in use.
        let mut seen = std::collections::HashSet::new();
        for (id, _, accel) in TABLE {
            if let Some(a) = accel {
                assert!(seen.insert(*a), "{a} is bound twice, once by {}", id.as_str());
            }
        }
    }

    #[test]
    fn v0_2_modifier_accelerators_agree_with_the_table() {
        // The menu's accelerator column and the key handler must not drift. Same
        // data today; this stops someone splitting them later.
        assert_eq!(for_modifiers(false, false), OPEN);
        assert_eq!(for_modifiers(true, false), RUN_AS_ADMIN);
        assert_eq!(for_modifiers(true, true), REVEAL);

        for (id, _, accel) in TABLE {
            let Some(accel) = accel else { continue };
            let ctrl = accel.contains("Ctrl");
            let shift = accel.contains("Shift");
            if accel.ends_with("Enter") {
                assert_eq!(&for_modifiers(ctrl, shift), id, "{accel} disagrees");
            }
        }
    }

    /// Every accelerator the menu advertises is actually bound to something.
    ///
    /// The Enter chords go through [`for_modifiers`]; the rest need their own
    /// branch in the Palette's key handler. Nothing else checks that, and the
    /// failure is silent: `Ctrl+Shift+C` sat in the menu for a whole phase doing
    /// nothing, found only by pressing it against the real binary.
    #[test]
    fn v0_2_every_advertised_accelerator_is_bound_somewhere() {
        let palette = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../src/palette/Palette.tsx"),
        )
        .expect("apps/desktop/src/palette/Palette.tsx");

        for (id, label, accel) in TABLE {
            let Some(accel) = accel else { continue };
            if accel.ends_with("Enter") {
                continue; // covered by `for_modifiers`, asserted above
            }
            assert!(
                palette.contains(&format!("\"{}\"", id.as_str())),
                "{label} advertises {accel} but Palette.tsx never runs {}",
                id.as_str()
            );
        }
    }

    /// An id no table row describes is a Source bug, and must look like one.
    #[test]
    fn v0_2_an_unknown_action_id_is_dropped_rather_than_drawn() {
        assert!(describe(&ActionId("teleport")).is_none());
        let menu = for_entry(&app_entry(vec![OPEN, ActionId("teleport")]));
        assert_eq!(menu.len(), 1);
    }

    /// ADR-0006: a Clip never offers to open a file location.
    #[test]
    fn v0_2_a_clip_may_not_offer_filesystem_actions() {
        assert!(!permitted(EntryKind::Clip, &REVEAL));
        assert!(!permitted(EntryKind::Clip, &RUN_AS_ADMIN));
        assert!(permitted(EntryKind::App, &REVEAL));
    }
}
