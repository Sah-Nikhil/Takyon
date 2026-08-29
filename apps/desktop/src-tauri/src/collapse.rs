//! Learning that two Entries are one application (v0.3 task 1b, TBC-0008).
//!
//! Owns the `launched` and `collapsed` tables in `frecency.db`. This is learned
//! usage data and belongs beside the other learned usage data — it is per-machine
//! and must never ship with the binary.
//!
//! Nothing here runs on the query path. Observations are written after an
//! activation; the collapse itself is computed when the application list is
//! rebuilt. TBC-0008 has the evidence rules and what would kill them.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{params, Connection, Result};

use crate::entry::EntryId;

/// The `launched` and `collapsed` tables.
pub struct CollapseStore {
    conn: Mutex<Connection>,
}

/// One Entry suppressed in favour of another, and why.
///
/// The evidence travels with the decision because TBC-0008's second guard needs
/// it: a suppressed Entry is recorded, not forgotten, so "why did my app
/// disappear?" is one file away rather than an archaeology problem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Collapse {
    pub winner: EntryId,
    pub loser: EntryId,
    pub evidence: String,
}

/// How many times one Entry must be seen starting an image before that counts.
///
/// TBC-0008's first guard. One observation can be a Squirrel stub exiting and
/// handing off to its child; a repeat cannot.
pub const OBSERVATIONS_REQUIRED: u32 = 2;

/// Where the "what was hidden and why" list lives, beside `frecency.db`.
///
/// A file rather than a flag, because the question is asked months later by
/// somebody who has forgotten the flag exists. It becomes a Settings surface at
/// v0.6 — `docs/tbd/v0.3.md` §7 owns that.
pub const REPORT_FILE: &str = "collapses.txt";

/// One executable path, canonicalised the way an `EntryId` is.
fn image_key(image: &Path) -> String {
    image.to_string_lossy().to_lowercase()
}

/// Rewrite the report of every suppressed Entry.
///
/// Written whole on every startup, so it can never disagree with the table. An
/// empty list still writes the file: an absent one reads as a broken launcher
/// rather than as "nothing was collapsed".
pub fn write_report(dir: &Path, collapses: &[Collapse]) -> std::io::Result<()> {
    use std::fmt::Write as _;

    std::fs::create_dir_all(dir)?;
    let mut out = String::from(
        "Entries Takyon has learned are duplicates, and hidden.\n\
         Nothing is deleted: the hidden Entry's usage was merged into the one kept.\n\
         Delete the `collapsed` table in frecency.db to undo all of this.\n\n",
    );

    if collapses.is_empty() {
        out.push_str("Nothing has been collapsed.\n");
    }
    for c in collapses {
        let _ = write!(
            out,
            "hidden        {}\n  in favour of  {}\n  because       {}\n\n",
            c.loser.as_str(),
            c.winner.as_str(),
            c.evidence
        );
    }
    std::fs::write(dir.join(REPORT_FILE), out)
}

/// The path half of an id, without the arguments task 0 folded in.
fn path_of(id: &EntryId) -> &str {
    id.as_str().split('|').next().unwrap_or_default()
}

/// Can this Entry's icon say anything about its identity?
///
/// Only an executable's. A document takes its *file type*'s icon, and both
/// halves also start the same viewer, so process observation agrees with the
/// wrong answer. Three of eleven real candidates were this.
fn icon_can_identify(id: &EntryId) -> bool {
    let path = path_of(id);
    if path.starts_with("aumid:") || path.starts_with("steam:") {
        return true;
    }
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("exe"))
        .unwrap_or(false)
}

/// Are these two the same binary, told apart only by their arguments?
///
/// Then they are two applications (task 0, ADR-0014's amendment). Both signals
/// agree here and both are wrong. `wsl.exe` beside `wsl.exe|--cd ~` is live.
fn differ_only_by_arguments(a: &EntryId, b: &EntryId) -> bool {
    a != b && path_of(a) == path_of(b)
}

/// Entries whose extracted icons are byte-identical, in pairs only.
///
/// **Three or more sharing an icon means it is generic** and carries no
/// identity: 41 of 99 icons sat in shared groups, largest eight. A property of
/// the corpus, not a tuning knob. TBC-0008 has the measurement.
pub fn pairs_by_icon(icons: &[(EntryId, Vec<u8>)]) -> Vec<(EntryId, EntryId)> {
    let mut groups: std::collections::HashMap<&[u8], Vec<&EntryId>> =
        std::collections::HashMap::new();
    for (id, bytes) in icons.iter().filter(|(id, _)| icon_can_identify(id)) {
        groups.entry(bytes.as_slice()).or_default().push(id);
    }

    let mut out: Vec<(EntryId, EntryId)> = groups
        .into_values()
        .filter(|g| g.len() == 2)
        .filter(|g| !differ_only_by_arguments(g[0], g[1]))
        .map(|mut g| {
            g.sort();
            (g[0].clone(), g[1].clone())
        })
        .collect();
    out.sort();
    out
}

/// Decide what is a duplicate, hide it, and say so. Called after every walk.
///
/// Off the query path by construction: this runs on the discovery thread, and
/// the icons it compares are the ones already in memory. Returns what was newly
/// decided, so the caller can report a change rather than the whole table.
pub fn learn(
    apps: &crate::sources::apps::AppSource,
    icons: &crate::icons::IconStore,
    store: &CollapseStore,
    frecency: &crate::frecency::Frecency,
    dir: Option<&Path>,
) -> Vec<Collapse> {
    let bytes = icons.extracted();
    let with_icons: Vec<(EntryId, Vec<u8>)> = apps
        .icon_keys()
        .into_iter()
        .filter_map(|(id, key)| bytes.get(&key).map(|b| (id, b.clone())))
        .collect();

    let fresh = store.apply(&store.collapses(&pairs_by_icon(&with_icons)), frecency);

    // Every decision, not only the new ones: a collapse decided last week still
    // has to suppress its row after today's walk rebuilt the list.
    let active = store.active();
    apps.apply_collapses(&active);

    if let Some(dir) = dir {
        if let Err(e) = write_report(dir, &active) {
            eprintln!("[takyon] could not write {REPORT_FILE}: {e}");
        }
    }
    fresh
}

/// Which of a corroborated pair keeps its identity, and which is suppressed.
///
/// ADR-0014 decides: the more durable id wins. Evidence sharpens that rather
/// than replacing it — the id that *is* the image is a real path. A versioned
/// path is disqualified first, because it dies at the next update.
pub fn decide(a: &EntryId, b: &EntryId, image: &str) -> (EntryId, EntryId) {
    let rank = |id: &EntryId| {
        let path = path_of(id);
        if path.starts_with("aumid:") || path.starts_with("steam:") {
            // No path at all: cannot reveal, elevate or copy.
            return 0;
        }
        if crate::sources::apps::lnk::is_versioned_target(Path::new(path)) {
            return 1;
        }
        if path == image {
            // What actually ran, and it survives an update.
            return 3;
        }
        2
    };

    if rank(b) > rank(a) {
        (b.clone(), a.clone())
    } else {
        (a.clone(), b.clone())
    }
}

impl CollapseStore {
    /// Open `frecency.db` in `dir`, or in memory when there is nowhere to write.
    pub fn open(dir: Option<PathBuf>) -> Result<Self> {
        let conn = match dir {
            Some(dir) => {
                std::fs::create_dir_all(&dir).ok();
                Connection::open(dir.join("frecency.db"))?
            }
            None => Connection::open_in_memory()?,
        };
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        conn.execute(
            "CREATE TABLE IF NOT EXISTS launched (
                 entry_id TEXT    NOT NULL,
                 image    TEXT    NOT NULL,
                 count    INTEGER NOT NULL,
                 PRIMARY KEY (entry_id, image)
             )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS collapsed (
                 loser      TEXT PRIMARY KEY,
                 winner     TEXT    NOT NULL,
                 evidence   TEXT    NOT NULL,
                 decided_at INTEGER NOT NULL
             )",
            [],
        )?;
        Ok(CollapseStore {
            conn: Mutex::new(conn),
        })
    }

    /// Record that activating `id` started `image`.
    pub fn observe(&self, id: &EntryId, image: &Path) -> Result<()> {
        let conn = self.conn.lock().expect("collapse mutex");
        conn.execute(
            "INSERT INTO launched (entry_id, image, count) VALUES (?1, ?2, 1)
             ON CONFLICT(entry_id, image) DO UPDATE SET count = count + 1",
            params![id.as_str(), image_key(image)],
        )?;
        Ok(())
    }

    /// What activating `id` has been seen to start, most-observed first.
    pub fn observations(&self, id: &EntryId) -> Vec<(String, u32)> {
        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };
        let Ok(mut stmt) = conn.prepare(
            "SELECT image, count FROM launched WHERE entry_id = ?1 ORDER BY count DESC, image",
        ) else {
            return Vec::new();
        };
        let rows = stmt.query_map([id.as_str()], |r| Ok((r.get(0)?, r.get(1)?)));
        rows.map(|r| r.flatten().collect()).unwrap_or_default()
    }

    /// Every pair of Entries seen to start the same executable, with that image.
    ///
    /// Both halves must clear [`OBSERVATIONS_REQUIRED`]. Ids come back in a fixed
    /// order so callers can compare them; which of the two *wins* is a separate
    /// question, and ADR-0014 answers it.
    pub fn corroborated_pairs(&self) -> Vec<(EntryId, EntryId, String)> {
        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };
        let Ok(mut stmt) = conn.prepare(
            "SELECT a.entry_id, b.entry_id, a.image
               FROM launched a
               JOIN launched b ON a.image = b.image AND a.entry_id < b.entry_id
              WHERE a.count >= ?1 AND b.count >= ?1
              ORDER BY a.entry_id, b.entry_id",
        ) else {
            return Vec::new();
        };
        let rows = stmt.query_map([OBSERVATIONS_REQUIRED], |r| {
            Ok((EntryId(r.get(0)?), EntryId(r.get(1)?), r.get(2)?))
        });
        rows.map(|r| r.flatten().collect()).unwrap_or_default()
    }

    /// Pairs backed by **both** signals, with the loser already decided.
    ///
    /// Icon identity narrows the candidates and process observation corroborates
    /// them; neither is trusted alone. `icon_pairs` comes from [`pairs_by_icon`],
    /// so it already excludes generic icons.
    pub fn collapses(&self, icon_pairs: &[(EntryId, EntryId)]) -> Vec<Collapse> {
        let by_icon: std::collections::HashSet<&(EntryId, EntryId)> =
            icon_pairs.iter().collect();

        self.corroborated_pairs()
            .into_iter()
            .filter(|(a, b, _)| by_icon.contains(&(a.clone(), b.clone())))
            // Again here, not only in `pairs_by_icon`: this is the guard that
            // keeps task 0 intact, and it must hold however a pair arrived.
            .filter(|(a, b, _)| !differ_only_by_arguments(a, b))
            .map(|(a, b, image)| {
                let (winner, loser) = decide(&a, &b, &image);
                Collapse {
                    winner,
                    loser,
                    evidence: format!(
                        "identical icons, and both started {image} at least \
                         {OBSERVATIONS_REQUIRED} times"
                    ),
                }
            })
            .collect()
    }

    /// Decide the collapses that are new, merging each loser's usage once.
    ///
    /// Returns only what was newly decided, so the caller can say what changed.
    /// The `collapsed` table's primary key is what makes this idempotent: a
    /// restart re-derives the same pairs and must not merge their weights again.
    pub fn apply(&self, found: &[Collapse], frecency: &crate::frecency::Frecency) -> Vec<Collapse> {
        let known = self.decided_losers();
        let mut fresh = Vec::new();

        for collapse in found {
            if known.contains(collapse.loser.as_str()) {
                continue;
            }
            // Merged first, recorded second. A merge with no record of it would
            // run again next launch and double the winner's weight.
            if let Err(e) = frecency.merge(&collapse.loser, &collapse.winner) {
                eprintln!("[takyon] could not merge usage for a collapse: {e}");
                continue;
            }
            if let Err(e) = self.record(collapse) {
                eprintln!("[takyon] could not record a collapse: {e}");
                continue;
            }
            fresh.push(collapse.clone());
        }
        fresh
    }

    fn decided_losers(&self) -> std::collections::HashSet<String> {
        self.active().into_iter().map(|c| c.loser.0).collect()
    }

    fn record(&self, collapse: &Collapse) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let conn = self.conn.lock().expect("collapse mutex");
        conn.execute(
            "INSERT OR IGNORE INTO collapsed (loser, winner, evidence, decided_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                collapse.loser.as_str(),
                collapse.winner.as_str(),
                collapse.evidence,
                now
            ],
        )?;
        Ok(())
    }

    /// Every collapse decided so far, oldest first.
    pub fn active(&self) -> Vec<Collapse> {
        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };
        let Ok(mut stmt) =
            conn.prepare("SELECT loser, winner, evidence FROM collapsed ORDER BY decided_at, loser")
        else {
            return Vec::new();
        };
        let rows = stmt.query_map([], |r| {
            Ok(Collapse {
                loser: EntryId(r.get(0)?),
                winner: EntryId(r.get(1)?),
                evidence: r.get(2)?,
            })
        });
        rows.map(|r| r.flatten().collect()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::EntryId;
    use std::path::Path;

    /// One activation is one observation, and the image path is canonicalised the
    /// same way an `EntryId` is — one executable reached two ways is one image.
    #[test]
    fn v0_3_observations_are_counted_per_entry_and_image() {
        let store = CollapseStore::open(None).unwrap();
        let id = EntryId(r"c:\windows\explorer.exe".into());

        store.observe(&id, Path::new(r"C:\Windows\Explorer.exe")).unwrap();
        store.observe(&id, Path::new(r"c:\windows\explorer.exe")).unwrap();

        assert_eq!(
            store.observations(&id),
            vec![(r"c:\windows\explorer.exe".to_string(), 2)]
        );
    }

    /// TBC-0008's first guard: one observation can be a Squirrel stub handing off
    /// to its child. A repeat costs a few days of use and removes that whole class.
    #[test]
    fn v0_3_a_pair_needs_the_same_evidence_twice_from_both_sides() {
        let store = CollapseStore::open(None).unwrap();
        let shim = EntryId(r"c:\windows\explorer.exe".into());
        let packaged = EntryId("aumid:Microsoft.Windows.Explorer".into());
        let image = Path::new(r"c:\windows\explorer.exe");

        store.observe(&shim, image).unwrap();
        store.observe(&shim, image).unwrap();
        store.observe(&packaged, image).unwrap();
        assert!(
            store.corroborated_pairs().is_empty(),
            "one side seen once is a one-off, not evidence"
        );

        store.observe(&packaged, image).unwrap();
        assert_eq!(
            store.corroborated_pairs(),
            vec![(packaged, shim, r"c:\windows\explorer.exe".to_string())]
        );
    }

    /// The measurement that changed the design, as a rule.
    ///
    /// 41 of 99 icons here share bytes with another, the largest group holding 8 —
    /// the console icon every `cmd.exe`-hosted prompt inherits, which are exactly
    /// the applications task 0 separated. A shared icon is generic, not identity.
    #[test]
    fn v0_3_an_icon_shared_by_three_entries_proves_nothing() {
        let console = vec![0x89, b'P', b'N', b'G'];
        let icons = vec![
            (EntryId(r"c:\a\prompt.exe".into()), console.clone()),
            (EntryId(r"c:\b\prompt.exe".into()), console.clone()),
            (EntryId(r"c:\c\prompt.exe".into()), console),
        ];
        assert!(pairs_by_icon(&icons).is_empty());
    }

    #[test]
    fn v0_3_exactly_two_entries_sharing_icon_bytes_are_a_candidate() {
        let shared = vec![1, 2, 3];
        let a = EntryId(r"c:\one\a.exe".into());
        let b = EntryId(r"c:\two\b.exe".into());
        let icons = vec![
            (b.clone(), shared.clone()),
            (a.clone(), shared),
            (EntryId(r"c:\three\c.exe".into()), vec![9, 9]),
        ];
        assert_eq!(pairs_by_icon(&icons), vec![(a, b)]);
    }

    /// Measured on the real machine 2026-08-29: of 11 candidate pairs the icon
    /// rule produced, three were documents sharing a file-type icon. Both halves
    /// of each also start the same editor, so process observation agrees too.
    #[test]
    fn v0_3_documents_are_never_paired_however_they_look() {
        let shared = vec![1, 2, 3];
        let icons = vec![
            (EntryId(r"c:\winrar\rar.txt".into()), shared.clone()),
            (EntryId(r"c:\winrar\whatsnew.txt".into()), shared),
        ];
        assert!(
            pairs_by_icon(&icons).is_empty(),
            "two text files sharing notepad are not one application"
        );
    }

    /// A `.chm` and the viewer that opens it share an icon *and* an image path.
    #[test]
    fn v0_3_a_document_paired_with_its_viewer_is_not_a_duplicate() {
        let shared = vec![4, 5];
        let icons = vec![
            (EntryId(r"c:\winrar\winrar.chm".into()), shared.clone()),
            (EntryId(r"c:\windows\hh.exe".into()), shared),
        ];
        assert!(pairs_by_icon(&icons).is_empty());
    }

    /// The guard that stops this feature undoing task 0.
    ///
    /// Two shortcuts to one binary with different arguments are two applications
    /// (ADR-0014, amended at v0.3 task 0). They share an icon and by definition
    /// start the same image, so both signals agree and both are wrong.
    #[test]
    fn v0_3_two_argument_variants_of_one_binary_are_never_collapsed() {
        let shared = vec![7];
        let plain = EntryId(r"c:\wsl\wsl.exe".into());
        let with_args = EntryId(r"c:\wsl\wsl.exe|--cd ~".into());
        let icons = vec![(plain.clone(), shared.clone()), (with_args.clone(), shared)];
        assert!(
            pairs_by_icon(&icons).is_empty(),
            "arguments are identity, not detail"
        );

        // And again at the decision, in case a pair reaches it another way.
        let store = CollapseStore::open(None).unwrap();
        observed_pair(&store, &plain, &with_args, r"c:\wsl\wsl.exe");
        assert!(store.collapses(&[(plain, with_args)]).is_empty());
    }

    /// ADR-0014's second row, decided by evidence rather than by Source order.
    ///
    /// The id that *is* the image path is the thing that actually ran, and it is a
    /// real path — so it also supports reveal, elevate and copy path.
    #[test]
    fn v0_3_the_entry_that_is_the_image_path_wins() {
        let shim = EntryId(r"c:\windows\explorer.exe".into());
        let packaged = EntryId("aumid:Microsoft.Windows.Explorer".into());
        let (winner, loser) = decide(&packaged, &shim, r"c:\windows\explorer.exe");
        assert_eq!(winner, shim);
        assert_eq!(loser, packaged);
    }

    /// Neither id is the image, so durability decides. A path supports actions an
    /// AUMID cannot.
    #[test]
    fn v0_3_a_real_path_outlives_an_aumid() {
        let path = EntryId(r"c:\tools\thing.exe".into());
        let aumid = EntryId("aumid:Vendor.Thing".into());
        let (winner, _) = decide(&aumid, &path, r"c:\other\real.exe");
        assert_eq!(winner, path);
    }

    /// The Squirrel row: the versioned path dies at the next update, taking the
    /// Frecency with it, so the stub wins even though it is not what ran.
    #[test]
    fn v0_3_a_versioned_path_never_wins() {
        let stub = EntryId(r"c:\users\me\discord\update.exe".into());
        let versioned = EntryId(r"c:\users\me\discord\app-1.0.9253\discord.exe".into());
        let (winner, _) = decide(&stub, &versioned, r"c:\users\me\discord\app-1.0.9253\discord.exe");
        assert_eq!(winner, stub, "the id that ran is the one that will not survive");
    }

    /// Arguments are part of an id but not part of a path, so the comparison has
    /// to look past them (ADR-0014, amended at v0.3 task 0).
    #[test]
    fn v0_3_an_argument_bearing_id_still_matches_its_image() {
        let with_args = EntryId(r"c:\windows\system32\cmd.exe|/k vcvars.bat".into());
        let aumid = EntryId("aumid:Vendor.Prompt".into());
        let (winner, _) = decide(&aumid, &with_args, r"c:\windows\system32\cmd.exe");
        assert_eq!(winner, with_args);
    }

    /// Seed a store where both halves clear [`OBSERVATIONS_REQUIRED`].
    fn observed_pair(store: &CollapseStore, a: &EntryId, b: &EntryId, image: &str) {
        for id in [a, b] {
            for _ in 0..OBSERVATIONS_REQUIRED {
                store.observe(id, Path::new(image)).unwrap();
            }
        }
    }

    /// Neither signal collapses anything alone. TBC-0008 keeps the icon signal
    /// only as a narrowing filter, and process observation is what corroborates.
    #[test]
    fn v0_3_one_signal_alone_collapses_nothing() {
        let store = CollapseStore::open(None).unwrap();
        let shim = EntryId(r"c:\windows\explorer.exe".into());
        let packaged = EntryId("aumid:Microsoft.Windows.Explorer".into());

        observed_pair(&store, &shim, &packaged, r"c:\windows\explorer.exe");
        assert!(
            store.collapses(&[]).is_empty(),
            "the same process is not enough without matching icons"
        );

        let icons_only = CollapseStore::open(None).unwrap();
        assert!(
            icons_only
                .collapses(&[(packaged.clone(), shim.clone())])
                .is_empty(),
            "matching icons are not enough without a launch"
        );
    }

    #[test]
    fn v0_3_both_signals_together_collapse_the_less_durable_entry() {
        let store = CollapseStore::open(None).unwrap();
        let shim = EntryId(r"c:\windows\explorer.exe".into());
        let packaged = EntryId("aumid:Microsoft.Windows.Explorer".into());
        observed_pair(&store, &shim, &packaged, r"c:\windows\explorer.exe");

        let found = store.collapses(&[(packaged.clone(), shim.clone())]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].winner, shim);
        assert_eq!(found[0].loser, packaged);
        assert!(found[0].evidence.contains("explorer.exe"));
    }

    /// The merge must happen once, not once per startup, or every restart would
    /// double the winner's weight.
    #[test]
    fn v0_3_a_collapse_is_decided_and_merged_exactly_once() {
        use crate::entry::EntryKind;
        use crate::frecency::Frecency;

        let store = CollapseStore::open(None).unwrap();
        let frecency = Frecency::open(None).unwrap();
        let shim = EntryId(r"c:\windows\explorer.exe".into());
        let packaged = EntryId("aumid:Microsoft.Windows.Explorer".into());
        observed_pair(&store, &shim, &packaged, r"c:\windows\explorer.exe");
        frecency.record(&packaged, EntryKind::App).unwrap();

        let pairs = [(packaged.clone(), shim.clone())];
        let first = store.apply(&store.collapses(&pairs), &frecency);
        assert_eq!(first.len(), 1, "the first pass decides it");
        let weight = frecency.weight(&shim);
        assert!(weight > 0.0, "the loser's usage moved across");

        let second = store.apply(&store.collapses(&pairs), &frecency);
        assert!(second.is_empty(), "a decided collapse is not decided again");
        assert_eq!(frecency.weight(&shim), weight, "the weight was merged twice");
    }

    /// Every decision, for suppression and for the diagnostics file.
    #[test]
    fn v0_3_decided_collapses_are_readable_afterwards() {
        use crate::frecency::Frecency;

        let store = CollapseStore::open(None).unwrap();
        let frecency = Frecency::open(None).unwrap();
        let shim = EntryId(r"c:\windows\explorer.exe".into());
        let packaged = EntryId("aumid:Microsoft.Windows.Explorer".into());
        observed_pair(&store, &shim, &packaged, r"c:\windows\explorer.exe");
        store.apply(
            &store.collapses(&[(packaged.clone(), shim.clone())]),
            &frecency,
        );

        let active = store.active();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].loser, packaged);
        assert_eq!(active[0].winner, shim);
        assert!(!active[0].evidence.is_empty(), "a decision with no reason");
    }

    /// TBC-0008's second guard: a suppressed Entry is recorded, not forgotten.
    ///
    /// "Why did my app disappear?" has to be answerable without a settings panel,
    /// which does not exist until v0.6.
    #[test]
    fn v0_3_the_report_names_what_was_hidden_and_why() {
        let dir = std::env::temp_dir().join(format!("takyon-report-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        write_report(
            &dir,
            &[Collapse {
                winner: EntryId(r"c:\windows\explorer.exe".into()),
                loser: EntryId("aumid:Microsoft.Windows.Explorer".into()),
                evidence: "identical icons, and both started explorer.exe".into(),
            }],
        )
        .unwrap();

        let text = std::fs::read_to_string(dir.join(REPORT_FILE)).unwrap();
        assert!(text.contains("aumid:Microsoft.Windows.Explorer"), "{text}");
        assert!(text.contains(r"c:\windows\explorer.exe"), "{text}");
        assert!(text.contains("identical icons"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An empty report still gets written. A missing file reads as a bug in the
    /// launcher rather than as "nothing has been collapsed".
    #[test]
    fn v0_3_an_empty_report_says_so_rather_than_being_absent() {
        let dir = std::env::temp_dir().join(format!("takyon-report-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        write_report(&dir, &[]).unwrap();

        let text = std::fs::read_to_string(dir.join(REPORT_FILE)).unwrap();
        assert!(!text.trim().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Two Entries that start different executables are two applications, however
    /// often either is launched.
    #[test]
    fn v0_3_entries_that_start_different_images_are_never_paired() {
        let store = CollapseStore::open(None).unwrap();
        let a = EntryId(r"c:\a\one.exe".into());
        let b = EntryId(r"c:\b\two.exe".into());
        for _ in 0..5 {
            store.observe(&a, Path::new(r"c:\a\one.exe")).unwrap();
            store.observe(&b, Path::new(r"c:\b\two.exe")).unwrap();
        }
        assert!(store.corroborated_pairs().is_empty());
    }
}
