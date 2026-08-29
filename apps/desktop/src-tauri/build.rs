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
    tauri_build::build()
}
