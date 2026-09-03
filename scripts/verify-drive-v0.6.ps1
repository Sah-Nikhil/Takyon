<#
.SYNOPSIS
Drive the automatable half of docs/verify/v0.6.md section S, slice 1.

The native window is what is left after the Playwright suite: whether Ctrl+,
reaches Settings, whether a second request focuses the window rather than
building another, and whether closing it ends the process. That last one is the
step this script exists for - it is the failure ADR-0003 prevents, it looks like
a crash rather than a lifecycle bug, and it is the only place in the app with a
close button to regress it.

Same two rules as scripts/verify-drive.ps1. It refuses to type unless the
foreground window belongs to takyon, because injected keys go wherever focus is.
And it launches the binary itself, because a GUI process started from a tool call
is reaped when that call returns.

Section A (the real registry) and section M (the localStorage migration) stay
manual. Both need state this script must not create on the operator's machine.
#>
[CmdletBinding()]
param(
    [string]$Exe = "apps\desktop\src-tauri\target\release\takyon.exe",
    [string]$OutDir = "$env:TEMP\takyon-verify-v0.6",
    # Free chord. Alt+Space is contested by Raycast and PowerToys Run on most
    # machines this is developed on.
    [string]$Hotkey = "Ctrl+Alt+Shift+F9"
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type -Namespace Takyon -Name Drive6 -MemberDefinition @'
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(IntPtr h, out int pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll", SetLastError=true)] public static extern void keybd_event(byte vk, byte sc, uint f, System.UIntPtr x);
  public struct RECT { public int Left, Top, Right, Bottom; }
'@

# Without this the capture photographs the top-left fraction of a scaled display
# and every measured size is wrong by the scale factor. Silently.
[void][Takyon.Drive6]::SetProcessDPIAware()

$UP = 0x0002
$VK = @{ Esc = 0x1B; Ctrl = 0x11; Alt = 0x12; Shift = 0x10; F4 = 0x73; F9 = 0x78; Comma = 0xBC }

function Send-Chord([byte[]]$Mods, [byte]$Key) {
    foreach ($m in $Mods) { [Takyon.Drive6]::keybd_event($m, 0, 0, [UIntPtr]::Zero) }
    [Takyon.Drive6]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 40
    [Takyon.Drive6]::keybd_event($Key, 0, $UP, [UIntPtr]::Zero)
    $rev = $Mods.Clone(); [array]::Reverse($rev)
    foreach ($m in $rev) { [Takyon.Drive6]::keybd_event($m, 0, $UP, [UIntPtr]::Zero) }
    Start-Sleep -Milliseconds 600
}

function Send-Key([byte]$vk, [int]$rest = 300) {
    [Takyon.Drive6]::keybd_event($vk, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 20
    [Takyon.Drive6]::keybd_event($vk, 0, $UP, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds $rest
}

function Get-Front {
    $h = [Takyon.Drive6]::GetForegroundWindow()
    $procId = 0
    [void][Takyon.Drive6]::GetWindowThreadProcessId($h, [ref]$procId)
    $p = Get-Process -Id $procId -ErrorAction SilentlyContinue
    $r = New-Object Takyon.Drive6+RECT
    [void][Takyon.Drive6]::GetWindowRect($h, [ref]$r)
    @{
        Handle = $h
        Name   = $(if ($p) { $p.ProcessName } else { "?" })
        Width  = $r.Right - $r.Left
        Height = $r.Bottom - $r.Top
    }
}

function Show-Palette {
    Send-Chord @($VK.Ctrl, $VK.Alt, $VK.Shift) $VK.F9
    Start-Sleep -Milliseconds 500
}

# Does the foreground window actually have a page in it?
#
# The bug this exists for: a window whose webview never loaded is a title bar over
# an opaque white rectangle. Every other check passes - the window exists, it has
# the right label, it is the right size - and only the pixels say it is empty.
function Test-Painted([string]$Tag) {
    $f = Get-Front
    $r = New-Object Takyon.Drive6+RECT
    [void][Takyon.Drive6]::GetWindowRect($f.Handle, [ref]$r)
    # Inset past the title bar and borders, which are painted by Windows either way.
    $x = $r.Left + 40; $y = $r.Top + 90
    $w = ($r.Right - $r.Left) - 80; $h = ($r.Bottom - $r.Top) - 130
    if ($w -le 0 -or $h -le 0) { Write-Warning "${Tag}: window too small to sample"; return }

    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($x, $y, 0, 0, $bmp.Size)
    $blank = 0; $total = 0
    for ($px = 5; $px -lt $w; $px += 37) {
        for ($py = 5; $py -lt $h; $py += 37) {
            $c = $bmp.GetPixel($px, $py)
            $total++
            if ($c.R -gt 240 -and $c.G -gt 240 -and $c.B -gt 240) { $blank++ }
        }
    }
    $g.Dispose(); $bmp.Dispose()
    $pct = if ($total) { [math]::Round(100 * $blank / $total) } else { 0 }
    if ($pct -gt 90) {
        Write-Warning "$Tag FAILED: $pct% of the window is blank white - the webview did not load"
    }
    else {
        Write-Output "$Tag ok: window is painted ($pct% white)"
    }
}

function Save-Shot([string]$Tag) {
    $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($b.X, $b.Y, 0, 0, $bmp.Size)
    $bmp.Save((Join-Path $OutDir "$Tag.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    $f = Get-Front
    Write-Output ("{0,-24} front={1,-9} {2}x{3}" -f $Tag, $f.Name, $f.Width, $f.Height)
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
Get-ChildItem $OutDir -Filter *.png -ErrorAction SilentlyContinue | Remove-Item -Force

if (-not (Test-Path $Exe)) { throw "no build at $Exe - run 'bun run build' first" }
$underTest = (Resolve-Path $Exe).Path

# An installed Takyon swallows every summon through single-instance and the build
# under test never runs, silently. Same guard as verify-guard.ps1.
Get-Process takyon -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -ne $underTest } |
    ForEach-Object { Write-Output "stopping imposter pid $($_.Id) $($_.Path)"; Stop-Process -Id $_.Id -Force }

$env:TAKYON_HOTKEY = $Hotkey
$log = Join-Path $OutDir "stderr.txt"
$app = Start-Process -FilePath $Exe -RedirectStandardError $log `
    -RedirectStandardOutput (Join-Path $OutDir "stdout.txt") -PassThru
Write-Output "under test: pid $($app.Id)"
Start-Sleep -Seconds 6
Get-Content $log -ErrorAction SilentlyContinue

# S1: Ctrl+, from the Palette opens Settings.
Show-Palette
Save-Shot "S1a-palette"
Send-Chord @($VK.Ctrl) $VK.Comma
Start-Sleep -Milliseconds 900
Save-Shot "S1b-settings-open"

$settings = Get-Front
if ($settings.Name -ne "takyon") {
    Write-Warning "S1 FAILED: Ctrl+, left the foreground on '$($settings.Name)'"
}
elseif ($settings.Width -lt 700) {
    Write-Warning "S1 suspicious: window is $($settings.Width)px wide; settings.rs asks for 880"
}
else {
    Write-Output "S1 ok: settings window $($settings.Width)x$($settings.Height)"
}
Test-Painted "S1-painted"

# S3: a second request focuses the existing window rather than building another.
Show-Palette
Send-Chord @($VK.Ctrl) $VK.Comma
Start-Sleep -Milliseconds 900
Save-Shot "S3-second-request"

# S5: the one that looks like a crash. Close Settings; Takyon keeps running.
Send-Chord @($VK.Alt) $VK.F4
Start-Sleep -Seconds 2
Save-Shot "S5a-after-close"

$alive = Get-Process -Id $app.Id -ErrorAction SilentlyContinue
if (-not $alive) {
    Write-Warning "S5 FAILED: closing Settings ended the process (ADR-0003)"
}
else {
    Write-Output "S5 ok: pid $($app.Id) still running after Settings closed"
    # And the hotkey still answers, which is the half a live process does not prove.
    Show-Palette
    $after = Get-Front
    Save-Shot "S5b-hotkey-after-close"
    if ($after.Name -eq "takyon") {
        Write-Output "S5 ok: the hotkey still opens the Palette"
    }
    else {
        Write-Warning "S5 FAILED: process alive but the hotkey no longer answers"
    }
    Send-Key $VK.Esc
}

Get-Content $log -ErrorAction SilentlyContinue
Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
Write-Output ""
Write-Output "shots in $OutDir"
Write-Output "still needs a person: S2 and S4 (tray item, resize), section A (the"
Write-Output "real registry, including forcing a refused write) and section M (the"
Write-Output "localStorage migration, which needs a pre-v0.6 profile)."
