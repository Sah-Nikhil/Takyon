//! Clipboard history (ROADMAP v0.5, ADR-0006 and ADR-0008).
//!
//! Four parts, split so the safety rules are each in one place: `key` wraps the
//! AES key with DPAPI, `store` owns `clips.db` and every path that destroys rows,
//! `watch` decides what is ever captured, and `paste` puts a clip back.
//!
//! **Clips are not a Source.** They never appear in a Bangless list (ADR-0006),
//! so nothing here implements [`crate::entry::Source`] and nothing here is in
//! `query.rs`'s registry. `!v` reaches the store directly.

pub mod blocklist;
pub mod key;
pub mod paste;
pub mod store;
pub mod watch;

pub use blocklist::Blocklist;
pub use store::{Clip, ClipKind, ClipStore, Retention};
