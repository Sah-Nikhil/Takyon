<#
.SYNOPSIS
Drive the automatable half of docs/verify/v0.4.md against a release build.

Sections C, D, E and F are queries and one clipboard read, which is most of the
phase - the calculator has no icons, no window-shape surprises and nothing that
launches, so nearly all of it drives.

Same two rules as verify-drive.ps1: it refuses to type unless the foreground
window is takyon, because injected keys go wherever focus is, and it launches the
binary itself, because a GUI process started from a tool call is reaped when that
call returns.

Unlike the v0.2 driver this one types symbols, and it uses SendKeys rather than
keybd_event to do it. Holding VK_SHIFT around an injected keypress silently did
not take here: `12*1.18` arrived as `1281.18`, which is a valid expression, so
the Palette answered 1,281.18 and every screenshot looked like a pass. Read the
clipboard, not the pixels.
#>
[CmdletBinding()]
param(
    [string]$Exe = "apps\desktop\src-tauri\target\release\takyon.exe",
    [string]$OutDir = "$env:TEMP\takyon-verify-v0.4",
    # Free chord. Alt+Space is contested by Raycast and PowerToys Run on the
    # machines this is developed on, and a failed registration adds the amber
    # banner, which changes every window size measured below.
    [string]$Hotkey = "Ctrl+Alt+Shift+F9"
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type -Namespace Takyon -Name Drive4 -MemberDefinition @'
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(IntPtr h, out int pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll", SetLastError=true)] public static extern void keybd_event(byte vk, byte sc, uint f, System.UIntPtr x);
  public struct RECT { public int Left, Top, Right, Bottom; }
'@

# Without this the capture photographs the top-left fraction of a scaled display
# and every measured size is wrong by the scale factor. Silently.
[void][Takyon.Drive4]::SetProcessDPIAware()

$UP = 0x0002
$VK = @{ Back = 0x08; Enter = 0x0D; Esc = 0x1B; Space = 0x20; Ctrl = 0x11; Alt = 0x12; Shift = 0x10; F9 = 0x78; K = 0x4B }

# SendKeys reads `+ ^ % ~ ( ) { } [ ]` as control characters, and four of those
# are operators this phase is entirely about. Braced, they are literals.
function ConvertTo-SendKeys([string]$Text) {
    $out = ""
    foreach ($c in $Text.ToCharArray()) {
        if ('+^%~(){}[]'.Contains($c)) { $out += "{$c}" } else { $out += $c }
    }
    return $out
}

function Send-Key([byte]$vk, [int]$rest = 30) {
    [Takyon.Drive4]::keybd_event($vk, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 20
    [Takyon.Drive4]::keybd_event($vk, 0, $UP, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds $rest
}

function Send-Chord([byte[]]$Mods, [byte]$Key) {
    foreach ($m in $Mods) { [Takyon.Drive4]::keybd_event($m, 0, 0, [UIntPtr]::Zero) }
    [Takyon.Drive4]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 40
    [Takyon.Drive4]::keybd_event($Key, 0, $UP, [UIntPtr]::Zero)
    $rev = $Mods.Clone(); [array]::Reverse($rev)
    foreach ($m in $rev) { [Takyon.Drive4]::keybd_event($m, 0, $UP, [UIntPtr]::Zero) }
    Start-Sleep -Milliseconds 450
}

function Get-Front {
    $h = [Takyon.Drive4]::GetForegroundWindow()
    $procId = 0
    [void][Takyon.Drive4]::GetWindowThreadProcessId($h, [ref]$procId)
    $p = Get-Process -Id $procId -ErrorAction SilentlyContinue
    @{ Handle = $h; Name = $(if ($p) { $p.ProcessName } else { "?" }) }
}

function Show-Palette {
    Send-Chord @($VK.Ctrl, $VK.Alt, $VK.Shift) $VK.F9
    Start-Sleep -Milliseconds 600
}

# The guard. Nothing is ever typed into a window that is not the Palette.
#
# One re-summon before giving up: the Palette dismisses on focus loss, so any
# other window taking the foreground for a moment ends the run. Re-summoning
# recovers that without ever weakening the rule below, which is the part that
# matters - injected keys go wherever focus is.
function Send-Text([string]$Text) {
    $f = Get-Front
    if ($f.Name -ne "takyon") {
        Show-Palette
        $f = Get-Front
    }
    if ($f.Name -ne "takyon") {
        Write-Warning "refused to type '$Text': foreground is '$($f.Name)'"
        return $false
    }
    # SendKeys rather than keybd_event, and the difference is not cosmetic.
    # Holding VK_SHIFT around an injected keypress did not take: `12*1.18` arrived
    # as `1281.18`, which is a *valid expression*, so the Palette answered
    # 1,281.18 and every screenshot looked like a pass. The clipboard assertion at
    # the end is what caught it. SendKeys goes through the message queue and does
    # its own shifting, so there is nothing to get wrong.
    foreach ($c in $Text.ToCharArray()) {
        [System.Windows.Forms.SendKeys]::SendWait((ConvertTo-SendKeys ([string]$c)))
        Start-Sleep -Milliseconds 45
    }
    Start-Sleep -Milliseconds 550
    return $true
}

function Clear-Query { for ($i = 0; $i -lt 40; $i++) { Send-Key $VK.Back 6 } }

function Save-Shot([string]$Tag) {
    $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($b.X, $b.Y, 0, 0, $bmp.Size)
    $bmp.Save((Join-Path $OutDir "$Tag.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    $f = Get-Front
    $r = New-Object Takyon.Drive4+RECT
    [void][Takyon.Drive4]::GetWindowRect($f.Handle, [ref]$r)
    Write-Output ("{0,-28} front={1,-9} {2}x{3}" -f $Tag, $f.Name, ($r.Right - $r.Left), ($r.Bottom - $r.Top))
}

# One query, one capture. The window height is the cheap signal: a Palette with
# no rows is one row tall, so "did this query answer at all" is readable from the
# printed size without opening a single image.
function Step([string]$Tag, [string]$Text) {
    Clear-Query
    if (Send-Text $Text) { Save-Shot $Tag }
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
Get-ChildItem $OutDir -Filter *.png -ErrorAction SilentlyContinue | Remove-Item -Force

if (-not (Test-Path $Exe)) { throw "no build at $Exe - run 'bun run build' first" }
$underTest = (Resolve-Path $Exe).Path
Get-Process takyon -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -ne $underTest } |
    ForEach-Object { Write-Output "stopping imposter pid $($_.Id) $($_.Path)"; Stop-Process -Id $_.Id -Force }

$env:TAKYON_HOTKEY = $Hotkey
$log = Join-Path $OutDir "stderr.txt"
$app = Start-Process -FilePath $Exe -RedirectStandardError $log -RedirectStandardOutput (Join-Path $OutDir "stdout.txt") -PassThru
Write-Output "under test: pid $($app.Id)"

# Let the application walk finish. Every step below wants a settled index, so
# that a missing app row is a real absence rather than a race.
Start-Sleep -Seconds 8
Show-Palette
Save-Shot "start-empty"

Write-Output "--- C: arithmetic"
Step "C1-12x1.18"      "12*1.18"
Step "C4-10plus30pct"  "10+30%"
Step "C5-12plus30pct"  "12+30%"
Step "C6-200x10pct"    "200*10%"
Step "C7-exponent"     "2^3^2"
Step "C8-divide-zero"  "1/0"

Write-Output "--- D: detection, the part that matters"
Step "D1-1password"    "1password"
Step "D2-x264"         "x264"
Step "D3-202"          "202"
Step "D4-2024"         "2024"
Step "D5-2022"         "2022"
Step "D6-trailing-op"  "45+"
Step "D7-log"          "log"

Write-Output "--- E: conversion, offline"
Step "E1-kg-to-lb"     "40 kg to lb"
Step "E3-c-to-f"       "100 c to f"
Step "E4-gb-to-mb"     "1 gb to mb"
Step "E5-usd-to-inr"   "100 usd to inr"
Step "E6-cross-dim"    "40 kg to cm"

Write-Output "--- F: the forcing character"
Step "F1-forced-45"    "=45"
Step "F2-forced-app"   "=1password"

Write-Output "--- C9: the action menu on an answer"
Clear-Query
if (Send-Text "12*1.18") {
    Send-Chord @($VK.Ctrl) $VK.K
    Save-Shot "C9-menu-on-answer"
    Send-Key $VK.Esc 400
}

# C3, and the strongest assertion this script makes: the clipboard is the one
# thing it can read back rather than photograph. A sentinel first, so "unchanged"
# is distinguishable from "copied the right thing by luck".
Write-Output "--- C3: Enter copies the answer"
Set-Clipboard -Value "takyon-verify-sentinel"
Clear-Query
if (Send-Text "12*1.18") {
    Send-Key $VK.Enter 900
    $clip = Get-Clipboard
    Write-Output "clipboard after Enter: '$clip'"
    if ($clip -eq "14.16") { Write-Output "C3 PASS" } else { Write-Output "C3 FAIL - expected '14.16'" }
}

Start-Sleep -Milliseconds 400
Get-Content $log -ErrorAction SilentlyContinue
Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
Write-Output "shots in $OutDir"
Write-Output "Needs a person: F3-F8 (the Settings switch), and E1/E5 with a network monitor open."
