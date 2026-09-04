//! File search (IMPLEMENTATION_PLAN §5, ADR-0007).
//!
//! An unelevated scoped directory walk into a memory-mapped inverted index. No
//! service, no elevation, no raw volume access — MFT/USN was measured and
//! rejected in ADR-0007, surviving only as a post-V1 accelerator behind
//! [`FileIndex`].
//!
//! Names only, never content. `roots.rs` decides where the walk goes, `walker.rs`
//! performs it, `store.rs` is the on-disk format, `live.rs` maps and queries it.
//!
//! **Never serve a known-stale index silently.** [`IndexStatus::Stale`] reaches
//! the UI: an index that quietly misses files teaches the user not to trust it.

pub mod live;
pub mod roots;
pub mod store;
pub mod walker;

use std::path::PathBuf;

/// One file or folder the index matched.
///
/// Carries the full path because that is both the Entry's identity (§2) and what
/// every `!e` action needs — open, reveal, copy path.
#[derive(Clone, Debug, PartialEq)]
pub struct FileHit {
    pub path: PathBuf,
    pub is_dir: bool,
    /// Match quality from `rank::score`, before Frecency and before Kind tiers.
    pub score: f32,
}

/// What the index can currently promise, surfaced in the UI (§5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexStatus {
    Ready,
    /// First walk in progress. `pct` is for a progress row, not a guarantee.
    Building { pct: u8 },
    /// Events were dropped and a rescan is pending. Results may be missing, and
    /// the user is told so rather than left to discover it.
    Stale,
}

/// The seam every acquisition strategy sits behind (§2).
///
/// Three implementors are foreseen: the walk, the Windows Search fallback for
/// locations outside the roots, and a post-V1 MFT accelerator. Keeping the trait
/// this narrow is what makes the third one a module rather than a rewrite.
pub trait FileIndex: Send + Sync {
    fn search(&self, q: &str, limit: usize) -> Vec<FileHit>;
    /// Bumped on any rescan, so a caller can tell results from two different
    /// index states apart.
    fn generation(&self) -> u64;
    fn status(&self) -> IndexStatus;
}
