# Upstream licence files

These published crate archives declare a licence in `Cargo.toml` but omit the
licence file stored at their repository root. The notice generator uses the
upstream file from each archive's `.cargo_vcs_info.json` commit when the archive
has no licence file. Keep each copy pinned to its package version; recheck the
source before changing a version.

| Packages | Upstream source at published commit |
| --- | --- |
| `alloc-stdlib@0.2.4` | [Dropbox LICENSE](https://github.com/dropbox/rust-alloc-no-stdlib/blob/ae42d22078b98549e987d2f03d12df7b984fde47/LICENSE) |
| `defmt-parser@1.0.0` | [Apache](https://github.com/knurling-rs/defmt/blob/4a8cdb44891ed57b8ff5a023b6bec7137c48708f/LICENSE-APACHE), [MIT](https://github.com/knurling-rs/defmt/blob/4a8cdb44891ed57b8ff5a023b6bec7137c48708f/LICENSE-MIT) |
| Five `unic-*` crates at `0.9.0` | [Apache](https://github.com/open-i18n/rust-unic/blob/5878605364af97a3358368a6eaef02104af2e016/LICENSE-APACHE), [MIT](https://github.com/open-i18n/rust-unic/blob/5878605364af97a3358368a6eaef02104af2e016/LICENSE-MIT) |
| `webview2-com@0.38.2`, `webview2-com-sys@0.38.2` | [MIT](https://github.com/wravery/webview2-rs/blob/b74dc5e2b394044bea5191052868ce7a106c202c/LICENSE) |
| `webview2-com-macros@0.8.1` | [MIT](https://github.com/wravery/webview2-rs/blob/dffa41a8a46d3f5565eefbff2de57d38d399f158/LICENSE) |

`selectors@0.36.1` has no full licence file at its published commit. Its
[source header](https://github.com/servo/stylo/blob/635e1a19d02960588a00e189bd4bd5bdb150ec3d/selectors/lib.rs)
links to Mozilla's MPL-2.0 text; the generator includes canonical MPL-2.0 text.
That copy comes from [SPDX License List v3.29.0](https://github.com/spdx/license-list-data/blob/v3.29.0/json/details/MPL-2.0.json).
`react-remove-scroll-bar@2.3.8` declares MIT in its
[published npm manifest](https://registry.npmjs.org/react-remove-scroll-bar/2.3.8)
but omits a licence file from the published package. No source revision for
that version was confirmed, so the generator retains its canonical MIT fallback.
Neither omission changes the licence declaration.
