//! Matching and ordering (IMPLEMENTATION_PLAN §3).
//!
//! v0.2 is matching only — Frecency and the Stability lock are v0.3, named in
//! comments so the seams are the right shape. No fuzzy subsequence in V1, by
//! decision (`docs/plans/post-v1.md`): the ladder exists so the top Entry is
//! predictable rather than clever.
//!
//! §3's tier table was **amended at v0.2** — its 900 and 800 rungs described the
//! same set, so one was unreachable. The repaired ladder and the reasoning are in
//! §3; the constants below are its implementation.

use crate::entry::{Entry, Query};

/// Scores for each rung of the ladder. Named rather than inlined, because the
/// gaps between them are load-bearing: [`LENGTH_PENALTY_MAX`] must stay smaller
/// than the smallest gap, or a long title in one tier could sink below a short
/// one in the tier beneath it.
pub const TIER_ALIAS_EXACT: f32 = 1000.0;
pub const TIER_EXACT_NAME: f32 = 900.0;
/// A keyword we shipped, not one the user wrote. Below an exact name on purpose:
/// `disk` reaching the Storage page must not beat an app called "Disk Cleanup".
pub const TIER_KEYWORD: f32 = 850.0;
pub const TIER_NAME_PREFIX: f32 = 800.0;
pub const TIER_WORD_PREFIX: f32 = 700.0;
pub const TIER_EXE_PREFIX: f32 = 650.0;
pub const TIER_ACRONYM: f32 = 600.0;

/// The most a title's length can cost it, in points.
///
/// Shorter name wins inside a tier: `photo` should reach "Photos" before "Photo
/// Editor Pro Deluxe". Without a tiebreak the order is whatever the walk
/// produced, which changes between runs and reads as unreliability.
const LENGTH_PENALTY_MAX: f32 = 20.0;

/// The acronym rung needs at least this many characters before it will fire.
///
/// A one-character acronym match means every multi-word app whose first initial
/// is `p` competes on equal footing, which is a list, not an answer.
const MIN_ACRONYM_LEN: usize = 2;

/// The invariant that makes the ladder a ladder, checked at compile time.
///
/// A `const` block, not a test: it is a statement about literals, so a failing
/// build is the right feedback. Widening [`LENGTH_PENALTY_MAX`] or narrowing a
/// gap fails while editing rather than in a test run nobody did.
const _: () = {
    assert!(LENGTH_PENALTY_MAX < TIER_EXE_PREFIX - TIER_ACRONYM);
    assert!(TIER_NAME_PREFIX - LENGTH_PENALTY_MAX > TIER_WORD_PREFIX);
    assert!(TIER_ALIAS_EXACT > TIER_EXACT_NAME);
    assert!(TIER_EXACT_NAME > TIER_KEYWORD);
    assert!(TIER_KEYWORD - LENGTH_PENALTY_MAX > TIER_NAME_PREFIX);
    assert!(TIER_EXACT_NAME > TIER_NAME_PREFIX);
    assert!(TIER_NAME_PREFIX > TIER_WORD_PREFIX);
    assert!(TIER_WORD_PREFIX > TIER_EXE_PREFIX);
    assert!(TIER_EXE_PREFIX > TIER_ACRONYM);
};

/// What a title looks like once it has been prepared for matching.
///
/// Built **once per Entry at discovery time**, not once per keystroke. Splitting
/// and lowercasing 342 titles on every keypress would spend most of the 20 ms
/// Source budget on work whose answer never changes.
#[derive(Clone, Debug)]
pub struct Haystack {
    /// The lowercased display name.
    pub name: String,
    /// Lowercased word tokens of the name, in order.
    pub words: Vec<String>,
    /// First letters of `words`, joined — "visual studio code" gives "vsc".
    pub acronym: String,
    /// The lowercased executable basename without extension, when there is one.
    /// A UWP app has no executable, so this is `None` and that rung never fires.
    pub exe_stem: Option<String>,
    /// User-defined aliases, lowercased. Filled from `settings.db` after the
    /// walk and refreshable in place, so a new alias works without a re-index.
    pub aliases: Vec<String>,
    /// Keywords Takyon ships, lowercased — `wifi` for the Network page. Separate
    /// from `aliases` because the user's own name for a thing outranks ours.
    pub keywords: Vec<String>,
}

impl Haystack {
    pub fn new(name: &str, exe_stem: Option<&str>) -> Self {
        let lower = name.to_lowercase();
        let words = tokenize(&lower);
        let acronym = words
            .iter()
            .filter_map(|w| w.chars().next())
            .collect::<String>();
        Haystack {
            name: lower,
            words,
            acronym,
            exe_stem: exe_stem.map(|s| s.to_lowercase()),
            aliases: Vec::new(),
            keywords: Vec::new(),
        }
    }

    /// A Haystack for something with **no display name** — a bare `PATH` executable.
        ///
        /// `Haystack::new(stem, Some(stem))` is the obvious call and is wrong: it makes
        /// the basename a display name, so `code` matches `code.cmd` at the exact-name
        /// rung and beats Visual Studio Code. Empty name leaves only the 650 rung.
    pub fn for_executable(stem: &str) -> Self {
        Haystack {
            name: String::new(),
            words: Vec::new(),
            acronym: String::new(),
            exe_stem: Some(stem.to_lowercase()),
            aliases: Vec::new(),
            keywords: Vec::new(),
        }
    }
}

/// Split a lowercased title into word tokens.
///
/// Splits on non-alphanumeric, which handles the punctuation real app names
/// carry: "7-Zip File Manager", "Node.js (64-bit)", "Adobe Photoshop 2024".
/// Digits stay attached so `2024` is findable; empty tokens are dropped.
fn tokenize(lower: &str) -> Vec<String> {
    lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect()
}

/// Score one Haystack against one query, or `None` for no match.
///
/// Tier score less a small length penalty, and the single place the ladder is
/// expressed. From v0.3 the *caller* multiplies in Frecency — mixing it here
/// would make the ladder untestable without a usage database.
pub fn score(q: &Query, hay: &Haystack) -> Option<f32> {
    let needle = q.needle.as_str();
    if needle.is_empty() {
        return None;
    }

    let tier = tier_of(needle, hay)?;

    // Shorter names win within a tier. Saturating at 40 characters so that two
    // very long names are separated by their content rather than by a rounding
    // difference in their length.
    let penalty = (hay.name.chars().count().min(40) as f32 / 40.0) * LENGTH_PENALTY_MAX;
    Some(tier - penalty)
}

/// Did this Entry match only through its executable's filename?
///
/// A binary name is an implementation detail: `chrome` reaches Helium through
/// `chrome.exe`. The rung still earns its place where no title answers —
/// `devenv`, `mmc` — so this reports rather than decides.
pub fn matched_only_by_binary(q: &Query, hay: &Haystack) -> bool {
    let needle = q.needle.as_str();
    if needle.is_empty() {
        return false;
    }
    let Some(stem) = &hay.exe_stem else {
        return false;
    };
    if !stem.starts_with(needle) {
        return false;
    }
    // Everything else on the ladder is derived from the title or from an alias
    // the user wrote. If any of those also fire, the row was reachable by name.
    let mut without_exe = hay.clone();
    without_exe.exe_stem = None;
    tier_of(needle, &without_exe).is_none()
}

/// Which rung of the ladder this needle reaches, highest wins.
///
/// Split out from [`score`] so the ladder can be asserted directly, without the
/// length penalty muddying the comparison.
pub fn tier_of(needle: &str, hay: &Haystack) -> Option<f32> {
    if hay.aliases.iter().any(|a| a == needle) {
        return Some(TIER_ALIAS_EXACT);
    }
    if hay.name == needle {
        return Some(TIER_EXACT_NAME);
    }
    if hay.keywords.iter().any(|k| k == needle) {
        return Some(TIER_KEYWORD);
    }
    if hay.name.starts_with(needle) {
        return Some(TIER_NAME_PREFIX);
    }
    // "Later-word" means exactly that: the first word is already covered by the
    // name-prefix rung above, and letting it match here too would mean a name
    // scoring 700 when it should have scored 800.
    if hay.words.iter().skip(1).any(|w| w.starts_with(needle)) {
        return Some(TIER_WORD_PREFIX);
    }
    if let Some(stem) = &hay.exe_stem {
        if stem.starts_with(needle) {
            return Some(TIER_EXE_PREFIX);
        }
    }
    if needle.chars().count() >= MIN_ACRONYM_LEN
        && hay.words.len() >= 2
        && hay.acronym.starts_with(needle)
    {
        return Some(TIER_ACRONYM);
    }
    None
}

/// Order a merged Entry list and truncate to `limit`.
///
/// **Kind before score** (§3): a file with a perfect match still sits below an
/// app that matched on an acronym. Final tiebreak is the id, not the title, so
/// the sort is total and the same query gives the same list every time.
pub fn order(mut entries: Vec<Entry>, limit: usize) -> Vec<Entry> {
    entries.sort_by(|a, b| {
        a.kind
            .tier()
            .cmp(&b.kind.tier())
            .then_with(|| {
                // Descending score. `total_cmp` rather than `partial_cmp` because
                // a NaN score from some future Source must not silently make the
                // comparator inconsistent, which is a panic in Rust's sort.
                b.score.total_cmp(&a.score)
            })
            .then_with(|| a.id.cmp(&b.id))
    });
    entries.truncate(limit);
    entries
}

/// Drop Entries sharing an id, keeping the best-scoring one.
///
/// `code.exe` is on `PATH` *and* has a Start Menu shortcut, and both paths are
/// meant to find it. The Start Menu copy scores higher and carries the real
/// display name, so keeping the higher score keeps the better title too.
pub fn dedupe(entries: Vec<Entry>) -> Vec<Entry> {
    use std::collections::HashMap;

    let mut best: HashMap<crate::entry::EntryId, Entry> = HashMap::new();
    for e in entries {
        match best.get(&e.id) {
            Some(existing) if existing.score >= e.score => {}
            _ => {
                best.insert(e.id.clone(), e);
            }
        }
    }
    best.into_values().collect()
}

/// The most Frecency can multiply a match score by, less one.
///
/// 0.6 lets a well-used Entry climb roughly one rung of the ladder — a much-used
/// acronym match can pass an exact-name match nobody has ever chosen, which is
/// ROADMAP v0.3's "Frecency over raw match quality" stated as a number.
pub const FRECENCY_LIFT: f32 = 0.6;

/// The weight at which half the lift is reached, so one launch is worth a lot
/// and the hundredth is worth almost nothing.
const FRECENCY_HALF: f32 = 1.0;

/// Fold usage into a match score.
///
/// Saturating rather than linear: `w / (w + half)` approaches 1, so the lift is
/// bounded and a decade of launches cannot push an Entry arbitrarily far. Weight
/// zero returns the base score untouched, which keeps a cold install honest.
pub fn with_frecency(base: f32, weight: f64) -> f32 {
    let w = weight.max(0.0) as f32;
    if w == 0.0 {
        return base;
    }
    base * (1.0 + FRECENCY_LIFT * (w / (w + FRECENCY_HALF)))
}

/// Drop every subtitle that is not telling two rows apart.
///
/// Per query, not per selection: a row that changed height when selected would
/// resize the content-sized window on every arrow key. Runs last, on the list
/// the Palette is actually sent. Reasoning in ADR-0016.
pub fn disambiguate_subtitles(mut entries: Vec<Entry>) -> Vec<Entry> {
    use std::collections::HashMap;

    let mut counts: HashMap<String, usize> = HashMap::new();
    for e in &entries {
        *counts.entry(e.title.to_lowercase()).or_default() += 1;
    }
    for e in &mut entries {
        if counts.get(&e.title.to_lowercase()).copied().unwrap_or(0) < 2 {
            e.subtitle = None;
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{EntryId, EntryKind};

    fn exe_hay(name: &str, stem: &str) -> Haystack {
        Haystack::new(name, Some(stem))
    }

    /// Helium is a Chromium fork that kept upstream's binary name, so `chrome`
    /// reaches it through `chrome.exe` and nothing else. A product name typed by
    /// a user must not return a different product because of a shared filename.
    #[test]
    fn v0_3_a_binary_name_match_yields_to_a_real_name_match() {
        let chrome = exe_hay("Google Chrome", "chrome");
        let helium = exe_hay("Helium", "chrome");
        let q = q("chrome");

        assert!(!matched_only_by_binary(&q, &chrome), "Chrome matches by its name");
        assert!(matched_only_by_binary(&q, &helium), "Helium matches only by its exe");
    }

    /// The rung still earns its place where no title answers. `devenv` names no
    /// application on this machine and is how people reach Visual Studio.
    #[test]
    fn v0_3_a_binary_name_is_still_a_way_in_when_no_name_matches() {
        let vs = exe_hay("Visual Studio 2022", "devenv");
        assert!(matched_only_by_binary(&q("devenv"), &vs));
        // Nothing matched by name, so the caller keeps it. That decision is the
        // list's, not this function's.
        assert!(score(&q("devenv"), &vs).is_some());
    }

    /// An acronym comes from the title, so it is a name match despite ranking
    /// below the executable rung.
    #[test]
    fn v0_3_an_acronym_counts_as_matching_by_name() {
        let vsc = exe_hay("Visual Studio Code", "code");
        assert!(!matched_only_by_binary(&q("vsc"), &vsc));
    }

    /// No usage, no opinion. An Entry nobody has chosen must score exactly what
    /// matching gave it, or Frecency silently reweights a cold install.
    #[test]
    fn v0_3_an_unused_entry_keeps_its_match_score() {
        let base = score(&q("code"), &hay("Visual Studio Code")).unwrap();
        assert_eq!(with_frecency(base, 0.0), base);
    }

    /// More use, more lift, and never unbounded — the lift saturates so a decade
    /// of launches cannot push an Entry arbitrarily far up the ladder.
    #[test]
    fn v0_3_the_frecency_lift_rises_and_saturates() {
        let base = 700.0;
        let (one, ten, huge) = (
            with_frecency(base, 1.0),
            with_frecency(base, 10.0),
            with_frecency(base, 10_000.0),
        );
        assert!(one > base && ten > one && huge > ten);
        assert!(huge <= base * (1.0 + FRECENCY_LIFT) + 1e-3);
    }

    /// tbd v0.2 §2, the case that justified this whole phase.
    ///
    /// `code` matched both at the later-word rung, and with no usage data the
    /// only tiebreak was name length — so the shorter "T3 Code (Alpha)" won and
    /// the editor sat second. One launch of the editor must settle it.
    #[test]
    fn v0_3_one_launch_settles_the_code_collision() {
        let t3 = score(&q("code"), &hay("T3 Code (Alpha)")).unwrap();
        let vsc = score(&q("code"), &hay("Visual Studio Code")).unwrap();
        assert!(t3 > vsc, "without usage the shorter name still wins");
        assert!(
            with_frecency(vsc, 1.0) > with_frecency(t3, 0.0),
            "one launch of the editor must put it on top"
        );
    }

    /// Kind ordering is a product rule, not a score. No amount of use may lift a
    /// document above an application — §3, and task 4's whole point.
    #[test]
    fn v0_3_frecency_never_lifts_a_file_above_an_app() {
        let mut file = titled("Notes", "C:\\notes.txt");
        file.kind = EntryKind::File;
        file.score = with_frecency(900.0, 10_000.0);
        let mut app = titled("Nothing", "C:\\n.exe");
        app.score = 1.0;
        let ordered = order(vec![file, app], 8);
        assert_eq!(ordered[0].kind, EntryKind::App);
    }

    fn titled(title: &str, subtitle: &str) -> Entry {
        Entry {
            id: EntryId(format!("{title}:{subtitle}").to_lowercase()),
            title: title.into(),
            subtitle: Some(subtitle.into()),
            kind: EntryKind::App,
            icon: None,
            score: 800.0,
            actions: vec![],
            version: None,
        }
    }

    /// A row whose title is unique needs nothing under it. The second line is
    /// disambiguation, and there is nothing to disambiguate.
    #[test]
    fn v0_3_a_unique_title_carries_no_second_line() {
        let out = disambiguate_subtitles(vec![
            titled("Visual Studio Code", r"C:\vsc\Code.exe"),
            titled("T3 Code (Alpha)", r"C:\t3\t3code.exe"),
        ]);
        assert!(out.iter().all(|e| e.subtitle.is_none()));
    }

    /// Two rows, one title: without the paths the user cannot tell which is
    /// which. This is the case the second line exists for.
    #[test]
    fn v0_3_a_repeated_title_keeps_its_second_line() {
        let out = disambiguate_subtitles(vec![
            titled("Code", r"C:\vsc\Code.exe"),
            titled("Code", r"C:\other\Code.exe"),
            titled("Notepad", r"C:\Windows\notepad.exe"),
        ]);
        let kept: Vec<&str> = out
            .iter()
            .filter_map(|e| e.subtitle.as_deref())
            .collect();
        assert_eq!(kept.len(), 2, "both Code rows keep theirs");
        assert!(kept.iter().all(|s| s.ends_with("Code.exe")));
        assert!(out.iter().find(|e| e.title == "Notepad").unwrap().subtitle.is_none());
    }

    /// Titles collide as the user reads them, not as bytes. `mspaint` beside
    /// "Paint" survives on purpose (ADR-0014), but a casing difference is not a
    /// difference anyone can see.
    #[test]
    fn v0_3_titles_collide_case_insensitively() {
        let out = disambiguate_subtitles(vec![
            titled("Discord", r"C:\a\Discord.exe"),
            titled("discord", r"C:\b\Discord.exe"),
        ]);
        assert!(out.iter().all(|e| e.subtitle.is_some()));
    }

    fn hay(name: &str) -> Haystack {
        Haystack::new(name, None)
    }

    fn hay_exe(name: &str, exe: &str) -> Haystack {
        Haystack::new(name, Some(exe))
    }

    fn q(s: &str) -> Query {
        Query::new(s)
    }

    // ---- the ladder, rung by rung ------------------------------------------

    #[test]
    fn v0_2_an_exact_name_outranks_a_prefix_of_a_longer_name() {
        // The reason the deviation documented at the top of this file is an
        // improvement rather than a workaround: typing `code` must not surface
        // "Code Composer Studio" above "Code".
        let exact = score(&q("code"), &hay("Code")).unwrap();
        let prefix = score(&q("code"), &hay("Code Composer Studio")).unwrap();
        assert!(exact > prefix, "exact {exact} should beat prefix {prefix}");
    }

    #[test]
    fn v0_2_a_name_prefix_outranks_a_later_word_prefix() {
        // §3's two worked examples, in the order §3 puts them.
        let adobe = score(&q("adobe"), &hay("Adobe Photoshop")).unwrap();
        let photo = score(&q("photo"), &hay("Adobe Photoshop")).unwrap();
        assert!(adobe > photo);
        assert_eq!(tier_of("adobe", &hay("Adobe Photoshop")), Some(TIER_NAME_PREFIX));
        assert_eq!(tier_of("photo", &hay("Adobe Photoshop")), Some(TIER_WORD_PREFIX));
    }

    /// Manual verification step 1: `phot` finds Photoshop.
    #[test]
    fn v0_2_phot_reaches_photoshop_through_the_later_word_rung() {
        assert!(score(&q("phot"), &hay("Adobe Photoshop")).is_some());
    }

    /// Manual verification step 3 as the plan words it: **`code` finds Visual Studio
    /// Code**, not the `code.cmd` shim beside it on `PATH`.
    ///
    /// Failed on first run against the real binary — a `PATH` entry was given its
    /// basename as a display name, matching at 900 and burying the editor.
    #[test]
    fn v0_2_a_path_shim_does_not_outrank_the_application_it_shims() {
        let shim = Haystack::for_executable("code");
        let editor = hay_exe("Visual Studio Code", "Code");

        let shim_score = score(&q("code"), &shim).unwrap();
        let editor_score = score(&q("code"), &editor).unwrap();
        assert!(
            editor_score > shim_score,
            "Visual Studio Code ({editor_score}) must outrank code.cmd ({shim_score})"
        );
        assert_eq!(tier_of("code", &shim), Some(TIER_EXE_PREFIX));
    }

    /// The other half: a tool that genuinely only exists on `PATH` is still found.
    /// Fixing the rung must not make `node` or `ffmpeg` unreachable.
    #[test]
    fn v0_2_a_path_only_tool_is_still_findable_by_its_basename() {
        let node = Haystack::for_executable("node");
        assert_eq!(tier_of("node", &node), Some(TIER_EXE_PREFIX));
        assert_eq!(tier_of("nod", &node), Some(TIER_EXE_PREFIX));
        // And nothing else about it matches, because it has no name to match.
        assert!(tier_of("javascript", &node).is_none());
    }

    /// Manual verification step 3: `code` finds Visual Studio Code.
    ///
    /// Asserted as "found", not "found by rung X" — it arrives on the later-word
    /// rung, since "Code" is a word of the name. Pinning the rung would fail the day
    /// someone renames the binary, for no user-visible reason.
    #[test]
    fn v0_2_code_reaches_vs_code() {
        let h = hay_exe("Visual Studio Code", "Code");
        assert!(tier_of("code", &h).is_some());
    }

    /// The executable rung earning its place: a binary named nothing like its app.
    /// `devenv` for Visual Studio, `wt` for Windows Terminal, `subl` for Sublime.
    /// Without it, none is reachable by the name a developer types.
    #[test]
    fn v0_2_an_executable_named_nothing_like_its_app_is_still_findable() {
        let vs = hay_exe("Visual Studio", "devenv");
        assert_eq!(tier_of("devenv", &vs), Some(TIER_EXE_PREFIX));
        assert_eq!(tier_of("deve", &vs), Some(TIER_EXE_PREFIX));

        // `wt` reaches Windows Terminal by both the executable rung and the
        // acronym rung. The executable wins, which is §3's ordering.
        let terminal = hay_exe("Windows Terminal", "wt");
        assert_eq!(terminal.acronym, "wt");
        assert_eq!(tier_of("wt", &terminal), Some(TIER_EXE_PREFIX));
    }

    /// Manual verification step 2: `vsc` finds VS Code by acronym.
    #[test]
    fn v0_2_vsc_reaches_vs_code_through_the_acronym_rung() {
        let h = hay_exe("Visual Studio Code", "Code");
        assert_eq!(h.acronym, "vsc");
        assert_eq!(tier_of("vsc", &h), Some(TIER_ACRONYM));
        // A prefix of the acronym counts too — nobody finishes typing.
        assert_eq!(tier_of("vs", &h), Some(TIER_ACRONYM));
    }

    /// Both rungs can fire for the same needle on different apps; §3 puts the
    /// executable first because it is something the user deliberately knows. The
    /// ordering of the constants themselves is asserted at compile time above.
    #[test]
    fn v0_2_the_executable_basename_rung_outranks_the_acronym_rung() {
        let by_exe = score(&q("devenv"), &hay_exe("Visual Studio", "devenv")).unwrap();
        let by_acronym = score(&q("cod"), &hay("Chrome Optimised Debugger")).unwrap();
        assert!(by_exe > by_acronym);
    }

    /// A keyword we ship must not outrank an application named for the same word.
    ///
    /// `disk` reached the Storage settings page above "Disk Cleanup", because
    /// task 8 put its curated keywords on the *user alias* rung. The user's own
    /// naming outranks ours; ours sits below an exact name.
    #[test]
    fn v0_3_a_shipped_keyword_ranks_below_a_users_own_alias_and_an_exact_name() {
        let mut storage = hay("Storage");
        storage.keywords = vec!["storage".into(), "disk".into()];
        assert_eq!(tier_of("disk", &storage), Some(TIER_KEYWORD));

        // An app whose name merely *starts* with the word still loses to the
        // keyword rung on tier — the 0.8 kind weight is what settles that pair,
        // and `query.rs` owns the test for it.
        assert_eq!(tier_of("disk", &hay("Disk Cleanup")), Some(TIER_NAME_PREFIX));

        // A user alias still wins outright, and an exact name still beats ours.
        let mut both = hay("Storage");
        both.keywords = vec!["disk".into()];
        both.aliases = vec!["disk".into()];
        assert_eq!(tier_of("disk", &both), Some(TIER_ALIAS_EXACT));
        let mut named = hay("disk");
        named.keywords = vec!["disk".into()];
        assert_eq!(tier_of("disk", &named), Some(TIER_EXACT_NAME));
    }

    #[test]
    fn v0_2_an_alias_beats_every_other_rung() {
        let mut h = hay("Adobe Photoshop");
        h.aliases.push("ps".into());
        assert_eq!(tier_of("ps", &h), Some(TIER_ALIAS_EXACT));
        // v0.2 discovers no aliases, so the rung is unreachable in practice until
        // v0.3 populates the field. It is tested now so that v0.3 is data, not
        // a change to the ladder.
        assert!(hay("Adobe Photoshop").aliases.is_empty());
    }

    // ---- what must NOT match ------------------------------------------------

    #[test]
    fn v0_2_a_subsequence_is_not_a_match() {
        // No fuzzy matching in V1 (post-v1.md). `sc` must not reach "Sublime
        // Text Code Editor" by picking letters out of the middle of words.
        assert!(score(&q("sc"), &hay("Sublime Text Code Editor")).is_none());
    }

    #[test]
    fn v0_2_a_mid_word_substring_is_not_a_match() {
        // "hop" appears inside "Photoshop" but starts no word. Allowing this would
        // put an app under every three-letter fragment of its own name.
        assert!(score(&q("hop"), &hay("Adobe Photoshop")).is_none());
    }

    /// A one-character needle never lands on the acronym rung.
    ///
    /// [`MIN_ACRONYM_LEN`] is not what enforces it: matching a one-char acronym means
    /// matching `words[0][0]`, so the name-prefix rung already fired one rung higher.
    /// The guard makes that explicit rather than a coincidence two rungs must keep.
    #[test]
    fn v0_2_a_single_character_never_lands_on_the_acronym_rung() {
        for name in ["Visual Studio Code", "Vendor Software Console", "Quick Zip"] {
            for needle in ["a", "q", "v", "z", "x"] {
                assert_ne!(
                    tier_of(needle, &hay(name)),
                    Some(TIER_ACRONYM),
                    "{needle:?} against {name:?} must not resolve by acronym"
                );
            }
        }
        // The matches that do happen for a single character come from a real
        // position in the name, not from its initials.
        assert_eq!(tier_of("v", &hay("Visual Studio Code")), Some(TIER_NAME_PREFIX));
        assert_eq!(tier_of("z", &hay("Quick Zip")), Some(TIER_WORD_PREFIX));
        assert!(tier_of("x", &hay("Quick Zip")).is_none());
    }

    #[test]
    fn v0_2_a_single_word_name_has_no_acronym_rung() {
        // "Photoshop" is one word; its "acronym" is "p", and matching that would
        // be indistinguishable from matching nothing.
        let h = hay("Photoshop");
        assert_eq!(h.acronym, "p");
        assert!(tier_of("px", &h).is_none());
    }

    #[test]
    fn v0_2_an_empty_query_matches_nothing() {
        // ADR-0001 again, from the ranker's side: no query, no Entries. Every
        // installed app on an empty Palette is a list, not an answer.
        assert!(score(&q(""), &hay("Adobe Photoshop")).is_none());
        assert!(score(&q("   "), &hay("Adobe Photoshop")).is_none());
    }

    // ---- tokenising real app names -----------------------------------------

    #[test]
    fn v0_2_punctuation_in_app_names_splits_into_words() {
        assert_eq!(tokenize("7-zip file manager"), ["7", "zip", "file", "manager"]);
        assert_eq!(tokenize("node.js (64-bit)"), ["node", "js", "64", "bit"]);
        assert_eq!(tokenize("adobe photoshop 2024"), ["adobe", "photoshop", "2024"]);
    }

    #[test]
    fn v0_2_a_version_number_in_the_name_is_findable() {
        assert_eq!(
            tier_of("2024", &hay("Adobe Photoshop 2024")),
            Some(TIER_WORD_PREFIX)
        );
    }

    // ---- within-tier ordering ----------------------------------------------

    #[test]
    fn v0_2_the_shorter_name_wins_inside_one_tier() {
        let short = score(&q("photo"), &hay("Photos")).unwrap();
        let long = score(&q("photo"), &hay("Photo Editor Pro Deluxe Ultimate")).unwrap();
        assert!(short > long, "short {short} should beat long {long}");
    }

    /// The compile-time invariant, demonstrated on real scores.
    ///
    /// The `const` block above proves the arithmetic; this proves the arithmetic
    /// is the arithmetic [`score`] actually does. A forty-character name on the
    /// name-prefix rung still has to beat a two-character name one rung down.
    #[test]
    fn v0_2_the_length_penalty_never_crosses_a_tier_boundary() {
        let long_name = "a".repeat(60);
        let worst_prefix = score(&q("a"), &hay(&long_name)).unwrap();
        let best_word = score(&q("zip"), &hay("Q Zip")).unwrap();
        assert!(
            worst_prefix > best_word,
            "the longest possible name-prefix match ({worst_prefix}) must still beat \
             the shortest later-word match ({best_word})"
        );
    }

    // ---- ordering and dedupe -----------------------------------------------

    fn entry(id: &str, title: &str, kind: EntryKind, score: f32) -> Entry {
        Entry {
            id: EntryId(id.into()),
            title: title.into(),
            subtitle: None,
            kind,
            icon: None,
            score,
            actions: vec![],
            version: None,
        }
    }

    #[test]
    fn v0_2_an_app_outranks_a_better_scoring_file() {
        let ordered = order(
            vec![
                entry("f", "notes.txt", EntryKind::File, 900.0),
                entry("a", "Notepad", EntryKind::App, 600.0),
            ],
            12,
        );
        assert_eq!(ordered[0].title, "Notepad");
    }

    #[test]
    fn v0_2_ordering_is_total_so_the_same_query_gives_the_same_list() {
        // Two Entries identical in kind and score. Without the id tiebreak their
        // order would depend on the order the rayon fan-out happened to merge
        // them, which differs between runs and reads as flakiness.
        let a = order(
            vec![
                entry("zzz", "Same", EntryKind::App, 700.0),
                entry("aaa", "Same", EntryKind::App, 700.0),
            ],
            12,
        );
        let b = order(
            vec![
                entry("aaa", "Same", EntryKind::App, 700.0),
                entry("zzz", "Same", EntryKind::App, 700.0),
            ],
            12,
        );
        assert_eq!(a[0].id, b[0].id);
        assert_eq!(a[0].id.as_str(), "aaa");
    }

    #[test]
    fn v0_2_the_list_is_truncated_to_the_limit() {
        let many: Vec<Entry> = (0..40)
            .map(|i| entry(&format!("id{i}"), "App", EntryKind::App, 700.0))
            .collect();
        assert_eq!(order(many, crate::entry::MAX_ENTRIES).len(), 12);
    }

    #[test]
    fn v0_2_one_exe_found_twice_collapses_to_the_better_entry() {
        // `code.exe` is on PATH and has a Start Menu shortcut. Both paths find it;
        // the user must see one row, and it must be the one with the real name.
        let merged = dedupe(vec![
            entry("c:\\vsc\\code.exe", "code", EntryKind::App, 650.0),
            entry("c:\\vsc\\code.exe", "Visual Studio Code", EntryKind::App, 800.0),
        ]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].title, "Visual Studio Code");
    }

    #[test]
    fn v0_2_dedupe_keeps_genuinely_different_apps() {
        let merged = dedupe(vec![
            entry("c:\\a\\code.exe", "Code", EntryKind::App, 800.0),
            entry("c:\\b\\code.exe", "Code Insiders", EntryKind::App, 800.0),
        ]);
        assert_eq!(merged.len(), 2);
    }
}
