//! Clipboard history (ROADMAP v0.5, ADR-0006 and ADR-0008).
//!
//! Split so the safety rules are each in one place: `key` wraps the AES key with
//! DPAPI, `store` owns `clips.db` and every path that destroys rows, `blobs` the
//! image files those paths shred, `image` the pixel formats, `watch` decides what
//! is ever captured, `paste` puts a clip back, and `os` holds the system
//! clipboard behind [`os::ClipboardStore`] (ADR-0025).
//!
//! **Clips are not a Source.** They never appear in a Bangless list (ADR-0006),
//! so nothing here implements [`crate::entry::Source`] and nothing here is in
//! `query.rs`'s registry. `!v` reaches the store directly.

pub mod blobs;
pub mod blocklist;
pub mod image;
pub mod key;
pub mod os;
pub mod paste;
pub mod store;
pub mod watch;

pub use blocklist::Blocklist;
pub use os::ClipboardStore;
pub use store::{Clip, ClipKind, ClipStore, Content, Retention};

/// URI scheme the webview draws image clips through (v0.14). Bytes are
/// decrypted per request and served `no-store`, so WebView2 never writes a
/// screenshot into its disk cache.
pub const SCHEME: &str = "takyon-clip";

/// Which half of an image clip a `takyon-clip` key asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum ImagePart {
    Thumb(i64),
    Full(i64),
}

/// `thumb-12` or `full-12`. Anything else is `None`, and a 404.
pub fn parse_image_key(key: &str) -> Option<ImagePart> {
    if let Some(id) = key.strip_prefix("thumb-") {
        return id.parse().ok().map(ImagePart::Thumb);
    }
    key.strip_prefix("full-")?.parse().ok().map(ImagePart::Full)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_14_image_keys_parse_and_nothing_else_does() {
        assert_eq!(parse_image_key("thumb-12"), Some(ImagePart::Thumb(12)));
        assert_eq!(parse_image_key("full-7"), Some(ImagePart::Full(7)));
        assert_eq!(parse_image_key("full-"), None);
        assert_eq!(parse_image_key("12"), None);
        assert_eq!(parse_image_key("thumb-../x"), None);
    }
}
