<#
.SYNOPSIS
  Build, self-sign and install the UIAccess helper so the Palette can appear over
  elevated windows on THIS machine.

.DESCRIPTION
  Windows honours a `uiAccess="true"` manifest only when the binary is
  Authenticode-signed against a trusted root AND lives somewhere a standard user
  cannot write to. Fail either and the process refuses to start with
  ERROR_ELEVATION_REQUIRED (740).

  A commercial certificate satisfies condition 1 for everyone. A self-signed
  certificate satisfies it only for machines that trust it — which is fine for
  development and useless for distribution. A real certificate stays a v1.0-ship
  prerequisite; this script exists so the feature can be built and verified now
  instead of being written blind.

.NOTES
  READ THIS BEFORE RUNNING.

  This script installs a root certificate into LocalMachine\Root. Until you remove
  it, your machine will trust ANY binary signed with that key. The private key
  lives in your certificate store and is gitignored wherever it is exported.

  Remove it with scripts/remove-dev-cert.ps1 when you are done.

  Requires an elevated PowerShell: writing to LocalMachine\Root and to
  %ProgramFiles% both need administrator rights.

.EXAMPLE
  # From an ELEVATED PowerShell, at the repo root:
  .\scripts\dev-sign-uiaccess.ps1
#>

[CmdletBinding()]
param(
    # Where the signed helper is installed. Must be a directory a standard user
    # cannot write to, or Windows ignores the manifest no matter how it is signed.
    [string]$InstallDir = (Join-Path $env:ProgramFiles 'Takyon'),

    [string]$CertSubject = 'CN=Takyon Dev Signing'
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot

function Assert-Elevated {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw "This script needs an elevated PowerShell. It writes to LocalMachine\Root and to $InstallDir."
    }
}

function Find-SignTool {
    # signtool ships with the Windows SDK, in a versioned directory. Newest wins.
    $roots = @(
        "${env:ProgramFiles(x86)}\Windows Kits\10\bin",
        "$env:ProgramFiles\Windows Kits\10\bin"
    ) | Where-Object { Test-Path $_ }

    $found = $roots |
        ForEach-Object { Get-ChildItem $_ -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue } |
        Where-Object { $_.FullName -match '\\x64\\' } |
        Sort-Object FullName -Descending |
        Select-Object -First 1

    if (-not $found) {
        throw @'
signtool.exe was not found. It ships with the Windows SDK.

Install "Windows 10/11 SDK" from the Visual Studio Installer (the "Windows SDK
Signing Tools" component alone is enough), then run this again.
'@
    }
    $found.FullName
}

Assert-Elevated
$signtool = Find-SignTool
Write-Host "signtool: $signtool"

# --- 1. Build the helper in release ------------------------------------------
# Debug is pointless here: the whole exercise is verifying the shipping artifact,
# and a debug helper would be signed and installed under a name the release build
# does not produce.
Write-Host "`nBuilding the helper (release)..."
& cargo build --release -p takyon-uiaccess-helper --manifest-path (Join-Path $repo 'apps\desktop\src-tauri\Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

$helper = Join-Path $repo 'apps\desktop\src-tauri\target\release\takyon-uiaccess-helper.exe'
if (-not (Test-Path $helper)) { throw "the helper was not produced at $helper" }

# --- 2. Certificate -----------------------------------------------------------
$cert = Get-ChildItem Cert:\LocalMachine\My |
    Where-Object { $_.Subject -eq $CertSubject } |
    Sort-Object NotAfter -Descending |
    Select-Object -First 1

if ($cert) {
    Write-Host "`nReusing the existing certificate ($($cert.Thumbprint))."
} else {
    Write-Host "`nCreating a self-signed code-signing certificate..."
    # `CodeSigningCert` gives it the Code Signing EKU, which is what Authenticode
    # verification checks for. A generic self-signed cert will sign but not verify.
    $cert = New-SelfSignedCertificate `
        -Type CodeSigningCert `
        -Subject $CertSubject `
        -CertStoreLocation Cert:\LocalMachine\My `
        -KeyUsage DigitalSignature `
        -KeyLength 2048 `
        -NotAfter (Get-Date).AddYears(2)

    Write-Host "  thumbprint: $($cert.Thumbprint)"

    # Trusting it. This is the security-relevant step, and it is why the script
    # says so at the top rather than doing it quietly.
    Write-Host '  installing into LocalMachine\Root (your machine will now trust this key)'
    $store = [Security.Cryptography.X509Certificates.X509Store]::new('Root', 'LocalMachine')
    $store.Open('ReadWrite'); $store.Add($cert); $store.Close()
}

# --- 3. Sign ------------------------------------------------------------------
Write-Host "`nSigning..."
& $signtool sign /fd SHA256 /sha1 $cert.Thumbprint /t http://timestamp.digicert.com $helper
if ($LASTEXITCODE -ne 0) {
    Write-Warning 'Timestamping failed (no network?). Signing without a timestamp; the signature dies with the certificate.'
    & $signtool sign /fd SHA256 /sha1 $cert.Thumbprint $helper
    if ($LASTEXITCODE -ne 0) { throw 'signtool failed' }
}

& $signtool verify /pa /v $helper
if ($LASTEXITCODE -ne 0) { throw 'the signature did not verify; Windows will refuse the uiAccess manifest' }

# --- 4. Install into a trusted location ---------------------------------------
# Condition 2. %ProgramFiles% qualifies because a standard user cannot write to
# it; anywhere under %LOCALAPPDATA% or the repo does not, however it is signed.
Write-Host "`nInstalling to $InstallDir..."
if (-not (Test-Path $InstallDir)) { New-Item -ItemType Directory -Path $InstallDir | Out-Null }
Copy-Item $helper (Join-Path $InstallDir 'takyon-uiaccess-helper.exe') -Force

$installed = Join-Path $InstallDir 'takyon-uiaccess-helper.exe'

Write-Host @"

Done.

  helper:  $installed
  cert:    $CertSubject  ($($cert.Thumbprint))

To use it from a development build, point Takyon at it:

  `$env:TAKYON_UIACCESS_HELPER = '$installed'
  bun run dev

Then verify the thing this exists for:

  1. Open an ELEVATED terminal and click it so it has focus.
  2. Press Alt+Space.
  3. The Palette should appear IN FRONT of it and accept typing.

Without the helper, step 3 fails silently -- that is UIPI, not a bug.

When you are finished, remove the trusted root:

  .\scripts\remove-dev-cert.ps1
"@
