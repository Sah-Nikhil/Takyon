//! The on-disk index: written once, then read by offset arithmetic (§5).
//!
//! Five sections: header, arena, entries, trigrams, postings. The layout is
//! IMPLEMENTATION_PLAN §5, and `Builder::finish` writes them in that order.
//!
//! **Never parsed.** Opening is an `mmap` and reading a row is an offset, so
//! startup costs a page fault. No Rust struct is laid over the bytes either:
//! every field goes through `from_le_bytes`, which removes alignment and padding
//! from the format's correctness.
//!
//! An entry stores its **name and parent**, not its path — paths share prefixes
//! massively, and storing each in full multiplies the arena by average depth.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// Bump to force a rebuild. Any layout change is a bump, including one that only
/// adds a field: an old file read by new offsets is silent nonsense.
pub const FORMAT_VERSION: u32 = 1;

const MAGIC: &[u8; 4] = b"TKX1";
const HEADER_LEN: usize = 80;
const ENTRY_LEN: usize = 12;
const TRIGRAM_LEN: usize = 12;

/// Parent of a root entry. Roots store their whole path as their name.
pub const NO_PARENT: u32 = u32::MAX;

const FLAG_DIR: u32 = 1;

/// Byte trigrams of a lowercased name, deduplicated, in first-seen order.
///
/// Bytes rather than chars: a UTF-8 name and a UTF-8 query produce the same byte
/// windows, so non-ASCII costs nothing and needs no separate path.
pub fn trigrams(name: &str) -> Vec<u32> {
    let lower = name.to_lowercase();
    let bytes = lower.as_bytes();
    if bytes.len() < 3 {
        return Vec::new();
    }
    let mut keys: Vec<u32> = Vec::with_capacity(bytes.len() - 2);
    for w in bytes.windows(3) {
        let key = (w[0] as u32) << 16 | (w[1] as u32) << 8 | w[2] as u32;
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

/// Accumulates entries in memory, then lays out the file in one pass.
///
/// Built by the walk and thrown away. A partial write is never mapped: the caller
/// writes to a temp path and renames, so a crash mid-write leaves the previous
/// index intact rather than a truncated one that opens and lies.
#[derive(Default)]
pub struct Builder {
    arena: Vec<u8>,
    interned: HashMap<String, u32>,
    entries: Vec<[u32; 3]>,
    postings: BTreeMap<u32, Vec<u32>>,
    roots: u32,
}

impl Builder {
    pub fn new() -> Self {
        Builder::default()
    }

    /// Add one entry, returning its id. Ids are handed out in insertion order, so
    /// a parent must be pushed before its children.
    pub fn push(&mut self, name: &str, parent: Option<u32>, is_dir: bool) -> u32 {
        let id = self.entries.len() as u32;
        let name_off = self.intern(name);
        let flags = if is_dir { FLAG_DIR } else { 0 };
        self.entries.push([name_off, parent.unwrap_or(NO_PARENT), flags]);

        if parent.is_none() {
            self.roots += 1;
        }
        for key in trigrams(name) {
            self.postings.entry(key).or_default().push(id);
        }
        id
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Identical names are stored once. `main.rs` and `mod.rs` repeat thousands of
    /// times across a code tree, and the arena is the largest part of the file.
    fn intern(&mut self, name: &str) -> u32 {
        if let Some(off) = self.interned.get(name) {
            return *off;
        }
        let off = self.arena.len() as u32;
        self.arena.extend_from_slice(name.as_bytes());
        self.arena.push(0);
        self.interned.insert(name.to_string(), off);
        off
    }

    /// Serialise. Postings are already sorted: ids are appended in increasing
    /// order because `push` hands them out that way.
    pub fn finish(self, generation: u64) -> Vec<u8> {
        let arena_off = HEADER_LEN;
        let entries_off = arena_off + self.arena.len();
        let trigrams_off = entries_off + self.entries.len() * ENTRY_LEN;
        let postings_off = trigrams_off + self.postings.len() * TRIGRAM_LEN;
        let postings_len: usize = self.postings.values().map(Vec::len).sum();

        let mut out = Vec::with_capacity(postings_off + postings_len * 4);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&generation.to_le_bytes());
        out.extend_from_slice(&self.roots.to_le_bytes());
        out.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        out.extend_from_slice(&(arena_off as u64).to_le_bytes());
        out.extend_from_slice(&(self.arena.len() as u64).to_le_bytes());
        out.extend_from_slice(&(entries_off as u64).to_le_bytes());
        out.extend_from_slice(&(trigrams_off as u64).to_le_bytes());
        out.extend_from_slice(&(self.postings.len() as u32).to_le_bytes());
        out.extend_from_slice(&(postings_len as u32).to_le_bytes());
        out.extend_from_slice(&(postings_off as u64).to_le_bytes());
        debug_assert!(out.len() <= HEADER_LEN, "header outgrew its reserved space");
        out.resize(HEADER_LEN, 0);

        out.extend_from_slice(&self.arena);
        for e in &self.entries {
            for field in e {
                out.extend_from_slice(&field.to_le_bytes());
            }
        }

        let mut cursor = 0u32;
        for (key, ids) in &self.postings {
            out.extend_from_slice(&key.to_le_bytes());
            out.extend_from_slice(&cursor.to_le_bytes());
            out.extend_from_slice(&(ids.len() as u32).to_le_bytes());
            cursor += ids.len() as u32;
        }
        for ids in self.postings.values() {
            for id in ids {
                out.extend_from_slice(&id.to_le_bytes());
            }
        }
        out
    }
}

/// Where the bytes came from. Mapped in the product, owned in tests — the reader
/// above it cannot tell, which is what keeps the format testable without a disk.
enum Backing {
    Mapped(memmap2::Mmap),
    Owned(Vec<u8>),
}

impl Backing {
    fn bytes(&self) -> &[u8] {
        match self {
            Backing::Mapped(m) => m,
            Backing::Owned(v) => v,
        }
    }
}

/// A mapped index, read by offset.
pub struct Index {
    backing: Backing,
    generation: u64,
    entry_count: u32,
    root_count: u32,
    arena_off: usize,
    arena_len: usize,
    entries_off: usize,
    trigrams_off: usize,
    trigram_count: usize,
    postings_off: usize,
    postings_len: usize,
}

impl Index {
    /// Map an index file, or refuse it.
    ///
    /// Refusal is normal, not exceptional: a `format_version` bump, a truncated
    /// write, a file from a future build. Every one of those means rebuild, and
    /// none of them may produce a half-usable index.
    pub fn open(path: &Path) -> Option<Index> {
        let file = std::fs::File::open(path).ok()?;
        // SAFETY: the index file is written once and replaced by rename, never
        // mutated in place, so no mapping outlives the bytes it describes.
        let map = unsafe { memmap2::Mmap::map(&file) }.ok()?;
        Index::new(Backing::Mapped(map))
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Option<Index> {
        Index::new(Backing::Owned(bytes))
    }

    fn new(backing: Backing) -> Option<Index> {
        let b = backing.bytes();
        if b.len() < HEADER_LEN || &b[0..4] != MAGIC {
            return None;
        }
        if u32(b, 4) != FORMAT_VERSION {
            return None;
        }
        let index = Index {
            generation: u64(b, 8),
            root_count: u32(b, 16),
            entry_count: u32(b, 20),
            arena_off: u64(b, 24) as usize,
            arena_len: u64(b, 32) as usize,
            entries_off: u64(b, 40) as usize,
            trigrams_off: u64(b, 48) as usize,
            trigram_count: u32(b, 56) as usize,
            postings_len: u32(b, 60) as usize,
            postings_off: u64(b, 64) as usize,
            backing,
        };
        index.plausible().then_some(index)
    }

    /// Every section has to fit inside the file before a single offset is
    /// trusted. A truncated index that opens is the one failure the format cannot
    /// detect later — it just returns fewer entries, silently.
    fn plausible(&self) -> bool {
        let len = self.backing.bytes().len();
        let entries_end = self.entries_off + self.entry_count as usize * ENTRY_LEN;
        let trigrams_end = self.trigrams_off + self.trigram_count * TRIGRAM_LEN;
        self.arena_off + self.arena_len <= len
            && entries_end <= len
            && trigrams_end <= len
            && self.postings_off + self.postings_len * 4 <= len
            && self.root_count <= self.entry_count
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn entry_count(&self) -> u32 {
        self.entry_count
    }

    pub fn root_count(&self) -> u32 {
        self.root_count
    }

    /// The entry's own name — one path segment, or the whole path for a root.
    pub fn name(&self, id: u32) -> &str {
        let Some(off) = self.field(id, 0) else {
            return "";
        };
        let b = self.backing.bytes();
        let start = self.arena_off + off as usize;
        let arena_end = self.arena_off + self.arena_len;
        if start >= arena_end {
            return "";
        }
        let end = b[start..arena_end]
            .iter()
            .position(|&c| c == 0)
            .map(|n| start + n)
            .unwrap_or(start);
        std::str::from_utf8(&b[start..end]).unwrap_or("")
    }

    pub fn parent(&self, id: u32) -> Option<u32> {
        match self.field(id, 1)? {
            NO_PARENT => None,
            parent => Some(parent),
        }
    }

    pub fn is_dir(&self, id: u32) -> bool {
        self.field(id, 2).is_some_and(|f| f & FLAG_DIR != 0)
    }

    /// Rebuild the full path by walking parents.
    ///
    /// The depth bound is a cycle guard, not a limit on real trees: a corrupt
    /// parent field pointing at a descendant would otherwise loop forever inside
    /// a query.
    pub fn path(&self, id: u32) -> PathBuf {
        let mut segments: Vec<&str> = Vec::new();
        let mut cursor = Some(id);
        for _ in 0..256 {
            let Some(current) = cursor else { break };
            segments.push(self.name(current));
            cursor = self.parent(current);
        }
        let mut path = PathBuf::new();
        for segment in segments.iter().rev() {
            path.push(segment);
        }
        path
    }

    fn field(&self, id: u32, n: usize) -> Option<u32> {
        if id >= self.entry_count {
            return None;
        }
        let off = self.entries_off + id as usize * ENTRY_LEN + n * 4;
        Some(u32(self.backing.bytes(), off))
    }

    /// Entry ids carrying one trigram, ascending. Binary search over the sorted
    /// key table, so a lookup is a handful of page reads.
    pub fn posting(&self, key: u32) -> Vec<u32> {
        let b = self.backing.bytes();
        let mut lo = 0usize;
        let mut hi = self.trigram_count;
        while lo < hi {
            let mid = (lo + hi) / 2;
            let at = self.trigrams_off + mid * TRIGRAM_LEN;
            match u32(b, at).cmp(&key) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => {
                    let off = u32(b, at + 4) as usize;
                    let len = u32(b, at + 8) as usize;
                    let start = self.postings_off + off * 4;
                    if start + len * 4 > b.len() {
                        return Vec::new();
                    }
                    return (0..len).map(|i| u32(b, start + i * 4)).collect();
                }
            }
        }
        Vec::new()
    }

    /// Candidate ids for a needle: every entry carrying **all** its trigrams.
    ///
    /// An over-approximation by design: `abc` also matches `abxxbc`, where both
    /// trigrams coexist. The real matcher verifies each candidate afterwards
    /// (§5), so this only has to be a cheap, correct superset.
    pub fn candidates(&self, needle: &str) -> Vec<u32> {
        let keys = trigrams(needle);
        if keys.is_empty() {
            return Vec::new();
        }
        let mut lists: Vec<Vec<u32>> = keys.iter().map(|k| self.posting(*k)).collect();
        // Intersect from the shortest: the first empty list ends it, and the work
        // is bounded by the rarest trigram rather than the commonest.
        lists.sort_by_key(Vec::len);
        let mut acc = lists.remove(0);
        for list in lists {
            if acc.is_empty() {
                break;
            }
            acc = intersect(&acc, &list);
        }
        acc
    }
}

/// Intersect two ascending id lists in one pass.
fn intersect(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                out.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out
}

fn u32(b: &[u8], at: usize) -> u32 {
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&b[at..at + 4]);
    u32::from_le_bytes(buf)
}

fn u64(b: &[u8], at: usize) -> u64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&b[at..at + 8]);
    u64::from_le_bytes(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Index {
        let mut b = Builder::new();
        let root = b.push(r"C:\Programming", None, true);
        let takyon = b.push("takyon", Some(root), true);
        let src = b.push("src", Some(takyon), true);
        b.push("main.rs", Some(src), false);
        b.push("bang.rs", Some(src), false);
        let other = b.push("pitchr", Some(root), true);
        b.push("main.rs", Some(other), false);
        Index::from_bytes(b.finish(7)).expect("index opens")
    }

    /// The round trip the whole format exists for: written, mapped, read back
    /// with every field intact.
    #[test]
    fn v0_7_an_index_survives_a_write_and_a_read() {
        let index = sample();
        assert_eq!(index.generation(), 7);
        assert_eq!(index.entry_count(), 7);
        assert_eq!(index.root_count(), 1);
        assert_eq!(index.name(0), r"C:\Programming");
        assert!(index.is_dir(0));
        assert!(!index.is_dir(3));
    }

    /// Entries store a name and a parent; the path is rebuilt. Getting this wrong
    /// gives every action the wrong file with no other symptom.
    #[test]
    fn v0_7_a_path_is_rebuilt_from_its_parents() {
        let index = sample();
        assert_eq!(
            index.path(3),
            PathBuf::from(r"C:\Programming\takyon\src\main.rs")
        );
        assert_eq!(index.path(6), PathBuf::from(r"C:\Programming\pitchr\main.rs"));
        assert_eq!(index.path(0), PathBuf::from(r"C:\Programming"));
    }

    /// Two files genuinely named `main.rs` are two entries sharing one arena
    /// string. Interning must not merge the entries themselves.
    #[test]
    fn v0_7_a_repeated_name_is_stored_once_but_stays_two_entries() {
        let index = sample();
        assert_eq!(index.name(3), index.name(6));
        assert_ne!(index.path(3), index.path(6));
    }

    /// Postings are the query path. A trigram present in one name must not return
    /// the other, or the verification step does all the work.
    #[test]
    fn v0_7_candidates_narrow_to_entries_carrying_every_trigram() {
        let index = sample();
        let hits: Vec<PathBuf> = index.candidates("bang").iter().map(|id| index.path(*id)).collect();
        assert_eq!(
            hits,
            vec![PathBuf::from(r"C:\Programming\takyon\src\bang.rs")]
        );

        let mains = index.candidates("main");
        assert_eq!(mains.len(), 2);
    }

    /// Candidates are a superset, verified later — but they must never *miss*.
    /// A false negative here is a file that exists and cannot be found.
    #[test]
    fn v0_7_a_substring_of_a_name_finds_it() {
        let index = sample();
        for needle in ["takyon", "kyo", "gramming", "pitchr"] {
            assert!(
                !index.candidates(needle).is_empty(),
                "{needle} should have candidates"
            );
        }
    }

    /// Under three bytes there is no trigram, so this path returns nothing and
    /// the caller takes the short-query route (§5) instead of scanning everything.
    #[test]
    fn v0_7_a_query_under_three_characters_yields_no_candidates() {
        let index = sample();
        assert!(index.candidates("ma").is_empty());
        assert!(index.candidates("").is_empty());
        assert!(trigrams("ab").is_empty());
    }

    /// A `format_version` bump forces a rebuild rather than a reinterpretation of
    /// bytes whose meaning has changed.
    #[test]
    fn v0_7_a_foreign_format_version_is_refused() {
        let mut bytes = sample_bytes();
        bytes[4] = FORMAT_VERSION as u8 + 1;
        assert!(Index::from_bytes(bytes).is_none());
    }

    /// The failure the format cannot detect later: a short file whose offsets
    /// point past its own end. It has to be refused at open.
    #[test]
    fn v0_7_a_truncated_index_is_refused_rather_than_half_read() {
        let bytes = sample_bytes();
        for cut in [HEADER_LEN, HEADER_LEN + 8, bytes.len() - 4] {
            assert!(
                Index::from_bytes(bytes[..cut].to_vec()).is_none(),
                "a file cut to {cut} bytes must not open"
            );
        }
        assert!(Index::from_bytes(Vec::new()).is_none());
        assert!(Index::from_bytes(b"not an index at all".to_vec()).is_none());
    }

    fn sample_bytes() -> Vec<u8> {
        let mut b = Builder::new();
        let root = b.push(r"C:\Programming", None, true);
        let src = b.push("src", Some(root), true);
        b.push("main.rs", Some(src), false);
        b.finish(1)
    }
}
