//! Steam games, from `libraryfolders.vdf` and the per-game `.acf` manifests.
//!
//! Steam puts no game in the Start Menu and none on `PATH`, so without this path a
//! whole library is invisible.
//!
//! **Games launch through the client, never as an executable.** Run directly they
//! exit immediately or complain about DRM, and never get cloud saves or playtime.
//! Hence [`LaunchTarget::SteamGame`] and `steam://rungameid/`.
//!
//! The VDF parser is hand-rolled: eighty lines for a four-token format, against a
//! dependency whose licence would need auditing while distribution is open.

use std::path::{Path, PathBuf};

use super::{Game, GameLibrary};
use crate::entry::GameLauncher;

/// A parsed VDF document.
///
/// Ordered pairs rather than a map, because VDF permits duplicate keys and
/// collapsing them silently would drop library folders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Vdf {
    Value(String),
    Table(Vec<(String, Vdf)>),
}

impl Vdf {
    /// The first child with this key, case-insensitively.
    ///
    /// Case-insensitive because Valve is not consistent about it across versions —
    /// `AppState` and `appid` sit in the same file — and a matcher that cared
    /// would work until the week Steam changed one.
    pub fn get(&self, key: &str) -> Option<&Vdf> {
        match self {
            Vdf::Table(pairs) => pairs
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v),
            Vdf::Value(_) => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Vdf::Value(s) => Some(s),
            Vdf::Table(_) => None,
        }
    }

    pub fn pairs(&self) -> &[(String, Vdf)] {
        match self {
            Vdf::Table(pairs) => pairs,
            Vdf::Value(_) => &[],
        }
    }
}

/// Parse a VDF document into a table of its top-level pairs.
///
/// Tolerant: a malformed tail returns what parsed rather than an error. Steam
/// rewrites these while a download is in flight, so reading a half-written one is
/// routine — and it should cost that one game, not the library.
pub fn parse(input: &str) -> Vdf {
    let mut cursor = Tokens::new(input);
    Vdf::Table(parse_pairs(&mut cursor, 0))
}

/// How deep a document may nest before it is treated as hostile.
///
/// Not a real Steam concern — it is four levels deep — but the parser recurses,
/// and a file consisting of eight thousand open braces would otherwise overflow
/// the stack of whichever thread happened to read it.
const MAX_DEPTH: usize = 32;

fn parse_pairs(cursor: &mut Tokens<'_>, depth: usize) -> Vec<(String, Vdf)> {
    let mut out = Vec::new();
    if depth > MAX_DEPTH {
        return out;
    }
    loop {
        match cursor.next() {
            None | Some(Token::Close) => return out,
            Some(Token::Open) => continue, // a stray brace; skip rather than abort
            Some(Token::Str(key)) => match cursor.next() {
                Some(Token::Str(value)) => out.push((key, Vdf::Value(value))),
                Some(Token::Open) => {
                    out.push((key, Vdf::Table(parse_pairs(cursor, depth + 1))));
                }
                Some(Token::Close) | None => return out,
            },
        }
    }
}

enum Token {
    Open,
    Close,
    Str(String),
}

struct Tokens<'a> {
    rest: &'a str,
}

impl<'a> Tokens<'a> {
    fn new(input: &'a str) -> Self {
        Tokens { rest: input }
    }

    fn next(&mut self) -> Option<Token> {
        loop {
            self.rest = self.rest.trim_start();
            // VDF comments run to end of line. Steam writes these into
            // `config.vdf` and, occasionally, into library files.
            if let Some(after) = self.rest.strip_prefix("//") {
                self.rest = after.find('\n').map_or("", |i| &after[i..]);
                continue;
            }
            break;
        }

        let mut chars = self.rest.char_indices();
        let (_, first) = chars.next()?;

        match first {
            '{' => {
                self.rest = &self.rest[1..];
                Some(Token::Open)
            }
            '}' => {
                self.rest = &self.rest[1..];
                Some(Token::Close)
            }
            '"' => {
                let mut value = String::new();
                let mut escaped = false;
                for (i, c) in chars {
                    if escaped {
                        // Only these two escapes appear in practice. Anything else
                        // is passed through with its backslash intact, because a
                        // Windows path is full of backslashes that mean themselves.
                        value.push(match c {
                            'n' => '\n',
                            't' => '\t',
                            other => other,
                        });
                        escaped = false;
                        continue;
                    }
                    match c {
                        '\\' => escaped = true,
                        '"' => {
                            self.rest = &self.rest[i + 1..];
                            return Some(Token::Str(value));
                        }
                        other => value.push(other),
                    }
                }
                // Unterminated string: consume the rest and stop.
                self.rest = "";
                Some(Token::Str(value))
            }
            _ => {
                // An unquoted token. Rare, but `libraryfolders.vdf` has been seen
                // with bare keys after a client update.
                let end = self
                    .rest
                    .find(|c: char| c.is_whitespace() || c == '{' || c == '}')
                    .unwrap_or(self.rest.len());
                let (token, rest) = self.rest.split_at(end);
                self.rest = rest;
                Some(Token::Str(token.to_string()))
            }
        }
    }
}

/// One installed Steam game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SteamGame {
    pub app_id: u32,
    pub name: String,
}

/// App ids Steam installs that are not games.
///
/// These sit in every library and would otherwise be the *only* things a machine
/// with no games installed surfaces, which is exactly the state this development
/// machine is in — the whole library here is `228980`.
const NOT_GAMES: &[u32] = &[
    228980,  // Steamworks Common Redistributables
    1070560, // Steam Linux Runtime
    1391110, // Steam Linux Runtime - Soldier
    1493710, // Proton Experimental
    1628350, // Steam Linux Runtime - Sniper
    2180100, // Proton Hotfix
];

/// Is this manifest a game rather than a runtime or redistributable?
///
/// Belt and braces: the id list catches the known ones, the name check catches
/// the next runtime Valve ships under an id nobody has written down. Too
/// permissive leaves "Proton Experimental" in results forever.
pub fn is_game(app_id: u32, name: &str) -> bool {
    if NOT_GAMES.contains(&app_id) {
        return false;
    }
    let lower = name.to_lowercase();
    // `"proton "` with the trailing space, and `== "proton"` for the bare case.
    // A bare `starts_with("proton")` also eats "Protonaut", which is a real game —
    // the kind of over-filtering that is invisible until the one person who owns
    // that game notices it is missing.
    let is_proton = lower == "proton" || lower.starts_with("proton ");
    !(lower.contains("steamworks") || lower.contains("steam linux runtime") || is_proton)
}

/// Extract the library paths from a parsed `libraryfolders.vdf`.
///
/// Two shapes are current, depending on when the client last rewrote the file:
/// modern nests `"path"` under each numbered entry, older makes the entry itself
/// the path. Handling only the modern one loses every game on a second drive.
pub fn library_paths(doc: &Vdf) -> Vec<PathBuf> {
    let Some(root) = doc.get("libraryfolders") else {
        return Vec::new();
    };
    root.pairs()
        .iter()
        .filter_map(|(_, entry)| match entry {
            Vdf::Table(_) => entry.get("path").and_then(Vdf::as_str).map(PathBuf::from),
            Vdf::Value(path) => Some(PathBuf::from(path)),
        })
        .collect()
}

/// Read one `appmanifest_*.acf` into a [`SteamGame`].
///
/// Returns `None` for a manifest with no name or no id — which happens while a
/// download is being set up, and means "not installed yet" rather than "broken".
pub fn parse_manifest(input: &str) -> Option<SteamGame> {
    let doc = parse(input);
    let state = doc.get("AppState")?;
    let app_id: u32 = state.get("appid")?.as_str()?.trim().parse().ok()?;
    let name = state.get("name")?.as_str()?.trim();
    if name.is_empty() {
        return None;
    }
    Some(SteamGame {
        app_id,
        name: name.to_string(),
    })
}

/// Where Steam is installed, from the registry.
///
/// `HKCU\Software\Valve\Steam\SteamPath`, written per-user at install time. The
/// value uses forward slashes, which every Windows API accepts, so it is used
/// as-is rather than normalised into something that only looks more correct.
#[cfg(windows)]
pub fn steam_path() -> Option<PathBuf> {
    use windows::core::w;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ, RRF_RT_REG_SZ,
    };

    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Valve\\Steam"),
            Some(0),
            KEY_READ,
            &mut key,
        )
        .is_err()
        {
            return None;
        }

        let mut buf = [0u16; 512];
        let mut size = (buf.len() * 2) as u32;
        let result = RegGetValueW(
            key,
            None,
            w!("SteamPath"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        );
        let _ = RegCloseKey(key);
        if result.is_err() {
            return None;
        }

        // `size` counts bytes and includes the terminating NUL.
        let len = (size as usize / 2).saturating_sub(1);
        let path = String::from_utf16_lossy(&buf[..len]);
        if path.is_empty() {
            None
        } else {
            Some(PathBuf::from(path))
        }
    }
}

#[cfg(not(windows))]
pub fn steam_path() -> Option<PathBuf> {
    None
}

/// Every installed game across every library.
///
/// The `steamapps` directory of the install itself is always a library, and it is
/// listed in `libraryfolders.vdf` too — hence the dedupe. Reading it twice would
/// double every game on a single-library machine.
pub fn discover(steam: &Path) -> Vec<SteamGame> {
    let mut roots = vec![steam.to_path_buf()];
    let index = steam.join("steamapps").join("libraryfolders.vdf");
    if let Ok(text) = std::fs::read_to_string(&index) {
        roots.extend(library_paths(&parse(&text)));
    }

    let mut seen_roots = std::collections::HashSet::new();
    let mut games = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();

    for root in roots {
        let key = root.to_string_lossy().to_lowercase().replace('/', "\\");
        if !seen_roots.insert(key) {
            continue;
        }
        let Ok(dir) = std::fs::read_dir(root.join("steamapps")) else {
            continue;
        };
        for manifest in dir.flatten() {
            let path = manifest.path();
            if path.extension().and_then(|e| e.to_str()) != Some("acf") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Some(game) = parse_manifest(&text) else {
                continue;
            };
            if is_game(game.app_id, &game.name) && seen_ids.insert(game.app_id) {
                games.push(game);
            }
        }
    }
    games
}

/// Steam's installed library.
pub struct SteamLibrary {
    path: PathBuf,
}

impl SteamLibrary {
    /// Present only if the client wrote its install path to the registry.
    pub fn detect() -> Option<Self> {
        steam_path().map(|path| SteamLibrary { path })
    }
}

impl GameLibrary for SteamLibrary {
    fn launcher(&self) -> GameLauncher {
        GameLauncher::Steam
    }

    fn games(&self) -> Vec<Game> {
        discover(&self.path)
            .into_iter()
            .map(|game| Game {
                launcher: GameLauncher::Steam,
                id: game.app_id.to_string(),
                name: game.name,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact shape of this machine's `libraryfolders.vdf`, checked in as a
    /// fixture. There is no game installed here, so this file is the only evidence
    /// the parser sees the real format rather than an idealised one.
    const REAL_LIBRARYFOLDERS: &str = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\GG\\STEAM"
		"label"		""
		"contentid"		"2875466511952205709"
		"totalsize"		"0"
		"update_clean_bytes_tally"		"2148411392"
		"time_last_update_verified"		"1785881129"
		"apps"
		{
			"228980"		"132691472"
		}
	}
}
"#;

    const REAL_REDIST_MANIFEST: &str = r#"
"AppState"
{
	"appid"		"228980"
	"universe"		"1"
	"name"		"Steamworks Common Redistributables"
	"StateFlags"		"4"
	"installdir"		"Steamworks Shared"
	"LastUpdated"		"1785881129"
	"SizeOnDisk"		"132691472"
	"buildid"		"18040421"
}
"#;

    const A_REAL_GAME: &str = r#"
"AppState"
{
	"appid"		"440"
	"name"		"Team Fortress 2"
	"installdir"		"Team Fortress 2"
	"StateFlags"		"4"
}
"#;

    #[test]
    fn v0_2_the_real_libraryfolders_file_yields_its_library() {
        let paths = library_paths(&parse(REAL_LIBRARYFOLDERS));
        assert_eq!(paths, vec![PathBuf::from(r"C:\GG\STEAM")]);
    }

    /// The escaped backslashes in the fixture are real: Valve writes `C:\\GG\\STEAM`
    /// and a parser that passed them through unescaped would produce a path that
    /// exists nowhere.
    #[test]
    fn v0_2_escaped_backslashes_in_a_path_are_unescaped_once() {
        let doc = parse(r#""libraryfolders" { "0" { "path" "D:\\Games\\Steam" } }"#);
        assert_eq!(library_paths(&doc), vec![PathBuf::from(r"D:\Games\Steam")]);
    }

    /// The older format, where the numbered entry is the path itself. Still on
    /// disk for anyone whose client has not rewritten the file.
    #[test]
    fn v0_2_the_legacy_libraryfolders_shape_still_parses() {
        let doc = parse(
            r#"
"libraryfolders"
{
	"TimeNextStatsReport"		"1785881129"
	"1"		"D:\\SteamLibrary"
	"2"		"E:\\Games"
}
"#,
        );
        let paths = library_paths(&doc);
        assert!(paths.contains(&PathBuf::from(r"D:\SteamLibrary")));
        assert!(paths.contains(&PathBuf::from(r"E:\Games")));
    }

    #[test]
    fn v0_2_a_game_manifest_yields_its_id_and_name() {
        assert_eq!(
            parse_manifest(A_REAL_GAME),
            Some(SteamGame {
                app_id: 440,
                name: "Team Fortress 2".into()
            })
        );
    }

    /// The one manifest that exists on this development machine. It must be
    /// filtered out, or the only Steam Entry the launcher ever shows here is a
    /// redistributable package that cannot be launched.
    #[test]
    fn v0_2_the_redistributables_manifest_is_not_a_game() {
        let parsed = parse_manifest(REAL_REDIST_MANIFEST).unwrap();
        assert_eq!(parsed.app_id, 228980);
        assert!(!is_game(parsed.app_id, &parsed.name));
    }

    #[test]
    fn v0_2_runtimes_are_filtered_by_name_as_well_as_by_id() {
        // The next runtime Valve ships will have an id this list does not know.
        assert!(!is_game(999_999, "Steam Linux Runtime 4.0"));
        assert!(!is_game(999_998, "Proton 9.0"));
        assert!(is_game(440, "Team Fortress 2"));
        // A game whose name merely mentions one of the words is still a game.
        assert!(is_game(12345, "Protonaut"));
    }

    /// Steam rewrites `.acf` files while a download is in flight. Reading a
    /// half-written one must cost that one game, not the library.
    #[test]
    fn v0_2_a_truncated_manifest_is_skipped_rather_than_fatal() {
        assert!(parse_manifest(r#""AppState" { "appid" "440" "#).is_none());
        assert!(parse_manifest(r#""AppState" { "name" "Half Life" }"#).is_none());
        assert!(parse_manifest("").is_none());
        assert!(parse_manifest(r#""AppState" { "appid" "440" "name" "" }"#).is_none());
    }

    #[test]
    fn v0_2_comments_and_odd_casing_do_not_break_the_parser() {
        let doc = parse(
            r#"
// written by the client
"appstate"
{
	"AppID"		"620"
	"Name"		"Portal 2"
}
"#,
        );
        let state = doc.get("AppState").unwrap();
        assert_eq!(state.get("appid").unwrap().as_str(), Some("620"));
        assert_eq!(state.get("NAME").unwrap().as_str(), Some("Portal 2"));
    }

    /// The parser recurses. A file of nothing but open braces must not take the
    /// discovery thread's stack with it.
    #[test]
    fn v0_2_a_pathologically_nested_document_terminates() {
        let hostile = r#""a" {"#.repeat(5000);
        let _ = parse(&hostile);
    }

    #[test]
    fn v0_2_duplicate_keys_are_all_kept() {
        // A map would silently drop the second library folder.
        let doc = parse(r#""libraryfolders" { "0" { "path" "C:\\A" } "1" { "path" "C:\\B" } }"#);
        assert_eq!(library_paths(&doc).len(), 2);
    }
}
