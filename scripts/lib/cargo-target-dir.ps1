# Print where cargo writes build output for the desktop crate.
#
# Not always src-tauri\target: CARGO_TARGET_DIR, --target-dir or a cargo config
# can move it (D:\cargo-target on the main dev machine). cargo metadata resolves
# all three the way the build does. TypeScript twin: cargo-target-dir.ts.
#
# Usage:  $target = & "$PSScriptRoot\lib\cargo-target-dir.ps1"

$ErrorActionPreference = 'Stop'
$manifest = Join-Path (Split-Path -Parent (Split-Path -Parent $PSScriptRoot)) 'apps\desktop\src-tauri\Cargo.toml'
$meta = & cargo metadata --format-version 1 --no-deps --manifest-path $manifest
if ($LASTEXITCODE -ne 0) { throw "cargo metadata failed for $manifest" }
($meta | Out-String | ConvertFrom-Json).target_directory
