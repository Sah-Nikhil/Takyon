# Refuse to run a manual verification pass against the wrong binary.
#
# tauri-plugin-single-instance hands a second launch to the copy already running.
# An installed Takyon therefore swallows every summon and the build under test
# never runs - silently, with a Palette that looks correct. See docs/verify/.
#
# Usage:  .\scripts\verify-guard.ps1            # check, and stop imposters
#         .\scripts\verify-guard.ps1 -WhatIf    # report only

[CmdletBinding(SupportsShouldProcess)]
param(
    [string]$Exe = "apps\desktop\src-tauri\target\release\takyon.exe"
)

$ErrorActionPreference = "Stop"

$underTest = $null
if (Test-Path $Exe) {
    $underTest = (Resolve-Path $Exe).Path
} else {
    Write-Warning "No build at $Exe - run 'bun run build' first."
}

$running = @(Get-Process takyon -ErrorAction SilentlyContinue)
if (-not $running) {
    Write-Output "No takyon.exe running. Clear."
    exit 0
}

$imposters = @($running | Where-Object { $_.Path -ne $underTest })

foreach ($p in $running) {
    $tag = if ($p.Path -eq $underTest) { "under test" } else { "IMPOSTER" }
    Write-Output ("  pid {0,-7} {1,-11} {2}" -f $p.Id, $tag, $p.Path)
}

if (-not $imposters) {
    Write-Output "Only the build under test is running. Clear."
    exit 0
}

foreach ($p in $imposters) {
    if ($PSCmdlet.ShouldProcess($p.Path, "Stop-Process")) {
        Stop-Process -Id $p.Id -Force
        Write-Output "Stopped pid $($p.Id)."
    }
}
