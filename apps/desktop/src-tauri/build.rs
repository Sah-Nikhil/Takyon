fn main() {
    // SCRATCH PROBE: a cargo test binary gets no application manifest, so the
    // loader binds comctl32 v5, which has no `TaskDialogIndirect` -> the process
    // dies with STATUS_ENTRYPOINT_NOT_FOUND before main. Only test targets.
    #[cfg(windows)]
    println!(
        "cargo:rustc-link-arg-tests=/MANIFESTDEPENDENCY:type='win32' \
         name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
         processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
    );

    // Profile dir binaries land in, for `firstrun::built_into`. OUT_DIR is
    // `<profile>\build\<pkg>-<hash>\out`; walking to `build` survives deeper layouts.
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default());
    let built_into = out
        .ancestors()
        .find(|a| a.file_name().is_some_and(|n| n == "build"))
        .and_then(|b| b.parent())
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=TAKYON_BUILT_INTO={built_into}");

    tauri_build::build()
}
