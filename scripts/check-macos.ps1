# Type-check the crate for aarch64-apple-darwin from Windows.
#
# Rust cross-compiles on its own; the C in the dependency tree does not.
# `libsqlite3-sys` and `objc2-exception-helper` both build native code, so cargo
# needs a C compiler that can target macOS. zig is one, and it ships the macOS
# libc and Objective-C headers, so no Apple SDK is involved.
#
# This checks and lints only. Linking a real .app still needs a Mac (or the
# Apple SDK), and no test runs here — see docs/plans/macos.md.
#
#   bun run check:macos
#
# Set TAKYON_ZIG to a zig directory to override discovery.

$ErrorActionPreference = "Stop"

$repo = Split-Path -Parent $PSScriptRoot
$target = "aarch64-apple-darwin"

# 1. zig, from TAKYON_ZIG, from PATH, or from the usual unpacked location.
$zigDir = $null
if ($env:TAKYON_ZIG -and (Test-Path (Join-Path $env:TAKYON_ZIG "zig.exe"))) {
    $zigDir = $env:TAKYON_ZIG
} elseif (Get-Command zig -ErrorAction SilentlyContinue) {
    $zigDir = Split-Path -Parent (Get-Command zig).Source
} else {
    $found = Get-ChildItem -Path "$env:LOCALAPPDATA\zig" -Filter "zig.exe" -Recurse -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($found) { $zigDir = $found.DirectoryName }
}

if (-not $zigDir) {
    Write-Host "zig not found." -ForegroundColor Red
    Write-Host "Download a build from https://ziglang.org/download/ and unpack it into"
    Write-Host "  $env:LOCALAPPDATA\zig\"
    Write-Host "or point TAKYON_ZIG at the directory holding zig.exe."
    exit 1
}

# 2. The Rust side of the target, which rustup owns.
$installed = & rustup target list --installed
if ($installed -notcontains $target) {
    Write-Host "rust target $target is missing. Adding it." -ForegroundColor Yellow
    & rustup target add $target
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

$env:PATH = "$zigDir;$env:PATH"
$env:CC_aarch64_apple_darwin = Join-Path $PSScriptRoot "zig\zig-cc-macos.cmd"
$env:AR_aarch64_apple_darwin = Join-Path $PSScriptRoot "zig\zig-ar.cmd"

Write-Host "zig:    $zigDir"
Write-Host "target: $target"
Write-Host ""

# `-p takyon`, never `--workspace`: the uiaccess helper is Windows-only by
# definition and has no macOS half to check.
& cargo clippy `
    --manifest-path (Join-Path $repo "apps\desktop\src-tauri\Cargo.toml") `
    --target $target -p takyon --all-targets -- -D warnings

exit $LASTEXITCODE
