//! Sources: producers of Entries for Bangless queries (CONTEXT.md).
//!
//! One Source in v0.2. The others arrive on their own phases — clipboard at v0.5,
//! files at v0.7, calculator at v0.4 — and each is a new module here plus one line
//! in `query.rs`'s registry. Nothing else should have to change, which is the test
//! of whether the [`crate::entry::Source`] trait was drawn in the right place.

pub mod apps;
pub mod calc;
pub mod commands;
pub mod files;
pub mod recents;
pub mod system;
