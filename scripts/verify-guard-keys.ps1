<#
.SYNOPSIS
Prove verify-drive-v0.5.ps1's key guards refuse when the wrong window has focus.

Runs in seconds and needs no build, so it is the cheap check to make before
trusting a driver run - and the regression test for a real incident: a browser
took the foreground mid-run, and paste-back put a real URL into the wrong window.

`Send-Key` and `Send-Chord` are replaced with counters, so nothing reaches the
OS. What is asserted is the count: **zero** whenever the target window is not the
foreground, and exactly one when it is. Takyon is not running, so every
window-name guard below is being asked for a window that does not exist.
#>

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type -Namespace Takyon -Name Drive5 -MemberDefinition @'
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(IntPtr h, out int pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll", SetLastError=true)] public static extern void keybd_event(byte vk, byte sc, uint f, System.UIntPtr x);
  public struct RECT { public int Left, Top, Right, Bottom; }
'@

$UP = 0x0002
$VK = @{ Back = 0x08; Enter = 0x0D; Ctrl = 0x11; A = 0x41; C = 0x43 }

# Counts every key that actually reaches the OS. The guard's whole job is to
# keep this at zero when the wrong window has focus.
$script:sent = 0
function Send-Key([byte]$vk, [int]$rest = 30) { $script:sent++ }
function Send-Chord([byte[]]$Mods, [byte]$Key) { $script:sent++ }

# The helpers under test, copied verbatim from the driver.
function Get-Front {
    $h = [Takyon.Drive5]::GetForegroundWindow()
    $procId = 0
    [void][Takyon.Drive5]::GetWindowThreadProcessId($h, [ref]$procId)
    $p = Get-Process -Id $procId -ErrorAction SilentlyContinue
    @{ Handle = $h; Name = $(if ($p) { $p.ProcessName } else { "?" }); Pid = $procId }
}
function Wait-Front([string]$Pattern, [int]$TimeoutMs = 800) {
    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    while ((Get-Date) -lt $deadline) {
        if ((Get-Front).Name -match $Pattern) { return $true }
        Start-Sleep -Milliseconds 120
    }
    return $false
}
function Send-KeyTo([string]$Pattern, [byte]$vk, [int]$rest = 30) {
    if (-not (Wait-Front $Pattern)) { return $false }
    Send-Key $vk $rest
    return $true
}
function Send-ChordTo([string]$Pattern, [byte[]]$Mods, [byte]$Key) {
    if (-not (Wait-Front $Pattern)) { return $false }
    Send-Chord $Mods $Key
    return $true
}
function Send-ChordToPid([int[]]$Ids, [byte[]]$Mods, [byte]$Key) {
    $deadline = (Get-Date).AddMilliseconds(800)
    while ((Get-Date) -lt $deadline) {
        if ($Ids -contains (Get-Front).Pid) { Send-Chord $Mods $Key; return $true }
        Start-Sleep -Milliseconds 120
    }
    return $false
}

$pass = 0; $fail = 0
function Check([string]$id, [bool]$ok, [string]$detail) {
    if ($ok) { $script:pass++ } else { $script:fail++ }
    "{0,-4} {1,-5} {2}" -f $id, $(if ($ok) { "PASS" } else { "FAIL" }), $detail
}

# A window of our own to hold the foreground. Takyon is not running, so every
# guard below is being asked for a window that does not exist.
$before = @(Get-Process notepad -ErrorAction SilentlyContinue | ForEach-Object Id)
[void](Start-Process notepad)
Start-Sleep -Seconds 3
$ours = @(Get-Process notepad -ErrorAction SilentlyContinue |
    Where-Object { $before -notcontains $_.Id } | ForEach-Object Id)

try {
    $front = (Get-Front).Name
    Check "G0" ($front -notmatch "takyon") "the foreground is '$front', not Takyon"

    $script:sent = 0
    $r = Send-KeyTo "^takyon$" $VK.Enter
    Check "G1" ((-not $r) -and $script:sent -eq 0) "Enter refused, $($script:sent) keys sent"

    $script:sent = 0
    $r = Send-ChordTo "^takyon$" @($VK.Ctrl) $VK.Enter
    Check "G2" ((-not $r) -and $script:sent -eq 0) "Ctrl+Enter refused, $($script:sent) keys sent"

    $script:sent = 0
    $r = Send-ChordTo "^takyon$" @($VK.Ctrl) $VK.Back
    Check "G3" ((-not $r) -and $script:sent -eq 0) "Ctrl+Backspace refused, $($script:sent) keys sent"

    # The positive path: a pid that *is* the foreground must be allowed through,
    # or the guard would refuse everything and prove nothing. Whatever holds focus
    # right now is used, because a freshly started Notepad does not reliably get
    # it - which is exactly why the guard exists.
    $script:sent = 0
    $r = Send-ChordToPid @((Get-Front).Pid) @($VK.Ctrl) $VK.A
    Check "G4" ($r -and $script:sent -eq 1) "a chord to the real foreground fired, $($script:sent) sent"

    # And a pid that is not ours must not.
    $script:sent = 0
    $r = Send-ChordToPid @(999999) @($VK.Ctrl) $VK.C
    Check "G5" ((-not $r) -and $script:sent -eq 0) "Ctrl+C to a foreign pid refused, $($script:sent) sent"
}
finally {
    foreach ($id in $ours) { Stop-Process -Id $id -Force -ErrorAction SilentlyContinue }
}

""
"$pass passed, $fail failed"
