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
/// Copy a calculated answer to the clipboard (v0.4).
///
/// Separate from [`COPY_PATH`] because they are different promises: one hands
/// over a location, the other a value. Sharing an id would also mean a Clip could
/// reach it, which [`permitted`] deliberately allows for paths.
pub const COPY_ANSWER: ActionId = ActionId("copy_answer");

/// Every action, with label and default accelerator.
///
/// `Enter` appears twice — [`OPEN`] and [`COPY_ANSWER`] — because what it does
/// depends on what is selected, which is [`for_modifiers`]'s job. Uniqueness is
/// per-menu, and a test asserts it there.
const TABLE: &[(ActionId, &str, Option<&str>)] = &[
    (OPEN, "Open", Some("Enter")),
    (RUN_AS_ADMIN, "Run as administrator", Some("Ctrl+Enter")),
    (REVEAL, "Open file location", Some("Ctrl+Shift+Enter")),
    (COPY_PATH, "Copy path", Some("Ctrl+Shift+C")),
    (COPY_ANSWER, "Copy answer", Some("Enter")),
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

/// Every action and its label, for the footer (v0.4.5 task 4).
///
/// Fetched once on mount rather than per selection: labels live here (ADR-0009),
/// and the alternative is an `invoke` on every arrow key or the same strings
/// re-sent with every keystroke.
pub fn all() -> Vec<Action> {
    TABLE.iter().filter_map(|(id, _, _)| describe(id)).collect()
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

/// Default actions for a system entry (task 8).
///
/// Open only: a settings page or control-panel task has no file to reveal or
/// copy and nothing to elevate. Same "no action that can only fail" rule as a
/// packaged app.
pub fn for_system() -> Vec<ActionId> {
    vec![OPEN]
}

/// Default actions for a calculation (v0.4).
///
/// One action, and it is not [`OPEN`]: there is nothing to launch. The answer
/// travels inside the EntryId, so copying needs no lookup — `sources/calc`.
pub fn for_calc() -> Vec<ActionId> {
    vec![COPY_ANSWER]
}

/// Which action a modifier combination triggers on Enter, for one Kind.
///
/// The only definition of what `Enter` and `Ctrl+Enter` mean. Kind-aware since
/// v0.4: a calculation has nothing to open, so every chord copies its answer
/// instead of reaching an action it does not have.
pub fn for_modifiers(kind: EntryKind, ctrl: bool, shift: bool) -> ActionId {
    if kind == EntryKind::Calc {
        return COPY_ANSWER;
    }
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
        // A calculation has no file and no target. Everything but copying its
        // answer would hit a launch arm that can only fail.
        EntryKind::Calc => id == &COPY_ANSWER,
        // A system entry can only be opened. Elevating or revealing one hits a
        // launch arm that errors, so a Ctrl+Enter accelerator would raise a
        // useless dialog; refuse it here instead.
        EntryKind::System | EntryKind::SystemTask => id == &OPEN,
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
            version: None,
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

    /// Amended at v0.4: uniqueness is **per menu**, not across the table.
    ///
    /// `Enter` is on both `Open` and `Copy answer` by design, and no Entry offers
    /// both. Sharing a chord *within one menu* is still the bug it always was:
    /// one action becomes unreachable, and which one depends on match order.
    #[test]
    fn v0_2_no_two_actions_in_one_menu_share_an_accelerator() {
        for menu in [for_app(true), for_app(false), for_file(), for_system(), for_calc()] {
            let mut seen = std::collections::HashSet::new();
            for action in menu.iter().filter_map(describe) {
                let Some(accel) = action.accelerator else { continue };
                assert!(
                    seen.insert(accel.clone()),
                    "{accel} is bound twice in one menu, once by {}",
                    action.id.as_str()
                );
            }
        }
    }

    #[test]
    fn v0_2_modifier_accelerators_agree_with_the_table() {
        // The menu's accelerator column and the key handler must not drift. Same
        // data today; this stops someone splitting them later.
        assert_eq!(for_modifiers(EntryKind::App, false, false), OPEN);
        assert_eq!(for_modifiers(EntryKind::App, true, false), RUN_AS_ADMIN);
        assert_eq!(for_modifiers(EntryKind::App, true, true), REVEAL);

        for (id, _, accel) in TABLE {
            let Some(accel) = accel else { continue };
            // `Enter` on a calculation is the kind-aware branch, asserted on its
            // own below rather than against the launchable mapping.
            if id == &COPY_ANSWER {
                continue;
            }
            let ctrl = accel.contains("Ctrl");
            let shift = accel.contains("Shift");
            if accel.ends_with("Enter") {
                assert_eq!(&for_modifiers(EntryKind::App, ctrl, shift), id, "{accel} disagrees");
            }
        }
    }

    /// v0.4.5: the footer reads the Entry's **first** action to name what Enter
    /// will do.
    ///
    /// Only honest while the first action *is* the plain-Enter one for every
    /// Kind. Reorder a Source's actions and the footer lies, silently.
    #[test]
    fn v0_4_5_the_first_action_is_always_what_plain_enter_does() {
        for (kind, actions) in [
            (EntryKind::App, for_app(true)),
            (EntryKind::App, for_app(false)),
            (EntryKind::File, for_file()),
            (EntryKind::System, for_system()),
            (EntryKind::Calc, for_calc()),
        ] {
            assert_eq!(
                actions.first(),
                Some(&for_modifiers(kind, false, false)),
                "the footer would name the wrong action for {kind:?}"
            );
        }
    }

    /// Every id in the table describes itself, or the footer draws a blank where
    /// a verb should be.
    #[test]
    fn v0_4_5_every_action_ships_a_label_for_the_footer() {
        let all = all();
        assert_eq!(all.len(), TABLE.len());
        for action in &all {
            assert!(!action.label.is_empty(), "{} has no label", action.id.as_str());
        }
    }

    /// v0.4: Enter against a calculation copies rather than launching, and the
    /// modifiers do not reach actions a calculation does not have. Raycast says
    /// the same thing in its footer — "Copy Answer", not "Open Application".
    #[test]
    fn v0_4_enter_on_a_calculation_copies_its_answer_whatever_the_modifiers() {
        for (ctrl, shift) in [(false, false), (true, false), (true, true)] {
            assert_eq!(for_modifiers(EntryKind::Calc, ctrl, shift), COPY_ANSWER);
        }
        assert!(permitted(EntryKind::Calc, &COPY_ANSWER));
        assert!(!permitted(EntryKind::Calc, &OPEN));
        assert!(!permitted(EntryKind::Calc, &REVEAL));
    }

    /// The menu a calculation gets: one row, labelled for what it does, with the
    /// key that does it shown beside it.
    #[test]
    fn v0_4_a_calculation_offers_exactly_one_action() {
        let menu = for_entry(&Entry {
            id: EntryId("calc:14.16".into()),
            title: "14.16".into(),
            subtitle: Some("12*1.18".into()),
            kind: EntryKind::Calc,
            icon: None,
            score: 1000.0,
            actions: for_calc(),
            version: None,
        });
        assert_eq!(menu.len(), 1);
        assert_eq!(menu[0].label, "Copy answer");
        assert_eq!(menu[0].accelerator.as_deref(), Some("Enter"));
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
