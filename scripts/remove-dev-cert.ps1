<#
.SYNOPSIS
  Remove the self-signed development certificate installed by
  scripts/dev-sign-uiaccess.ps1, and the helper it signed.

.DESCRIPTION
  While that certificate sits in LocalMachine\Root, this machine trusts anything
  signed with its private key. That is an acceptable trade on a development
  machine and a bad thing to leave behind afterwards, so removing it is a script
  rather than a paragraph in a document nobody reopens.

  Requires an elevated PowerShell.

.EXAMPLE
  .\scripts\remove-dev-cert.ps1
#>

[CmdletBinding(SupportsShouldProcess)]
param(
    [string]$CertSubject = 'CN=Takyon Dev Signing',
    [string]$InstallDir = (Join-Path $env:ProgramFiles 'Takyon')
)

$ErrorActionPreference = 'Stop'

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'This script needs an elevated PowerShell.'
}

$removed = 0
foreach ($storeName in 'Root', 'My', 'TrustedPublisher') {
    $store = [Security.Cryptography.X509Certificates.X509Store]::new($storeName, 'LocalMachine')
    $store.Open('ReadWrite')
    foreach ($c in @($store.Certificates | Where-Object { $_.Subject -eq $CertSubject })) {
        if ($PSCmdlet.ShouldProcess("$storeName\$($c.Thumbprint)", 'Remove certificate')) {
            $store.Remove($c)
            Write-Host "removed from $storeName : $($c.Thumbprint)"
            $removed++
        }
    }
    $store.Close()
}

if ($removed -eq 0) {
    Write-Host "No certificate with subject '$CertSubject' was found. Nothing to do."
}

$helper = Join-Path $InstallDir 'takyon-uiaccess-helper.exe'
if (Test-Path $helper) {
    # Left in place unless asked. It is now signed by a key this machine no longer
    # trusts, so Windows will refuse to start it -- inert rather than dangerous --
    # and deleting files under %ProgramFiles% without being asked is not this
    # script's business.
    Write-Host @"

The signed helper is still at:
  $helper

It will no longer start, because the certificate that vouched for it is gone.
Delete it yourself if you want it gone:
  Remove-Item '$helper'
"@
}
