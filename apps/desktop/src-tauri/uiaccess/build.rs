//! Embed the manifest that asks for `uiAccess="true"`.
//!
//! Done through the MSVC linker rather than a manifest crate, because the one
//! thing that matters is that the attribute reaches the binary verbatim, and most
//! manifest crates build the XML from a typed API that may not model it.
//!
//! Two options, and the split between them is not cosmetic:
//!
//! - `/MANIFESTUAC` owns `requestedExecutionLevel`. The linker authors its own
//!   manifest fragment when embedding, and `mt.exe` refuses to merge two fragments
//!   that disagree about `uiAccess` ("manifest authoring error c1010001"). So the
//!   attribute is set here and nowhere else.
//! - `/MANIFESTINPUT` merges `app.manifest`, which carries everything the linker
//!   does not own — the assembly identity and the supported-OS list.
//!
//! Neither of these makes the binary *trusted*. Windows honours the attribute only
//! for a signed binary in a location a standard user cannot write to; see
//! `docs/plans/uiaccess-signing.md`.
fn main() {
    println!("cargo:rerun-if-changed=app.manifest");

    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        // The GNU toolchain takes manifests through a resource file instead. This
        // is a loud skip rather than a silent one: an unmanifested helper starts
        // fine and then fails to do the single thing it exists for.
        println!("cargo:warning=not an MSVC target; the uiAccess manifest was NOT embedded");
        return;
    }

    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("app.manifest");
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:level='asInvoker' uiAccess='true'");
    println!(
        "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
        manifest.display()
    );
}
