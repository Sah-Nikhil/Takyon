<#
.SYNOPSIS
Drive the parts of docs/verify/v0.7.md that a mocked webview cannot reach.

Two things the Playwright suite is structurally unable to check, because
`api.mock.ts` returns "" for every icon URL and never walks a disk:

  E — real application icons in the Settings alias list, served over the
      `takyon-icon://` scheme from a second webview rather than the Palette's.
  I — the file index answering `!e` against this machine's real drives.

Same two rules as the other drivers. It refuses to type unless the foreground
window belongs to takyon, because injected keys go wherever focus is. And it
launches the binary itself, because a GUI process started from a tool call is
reaped when that call returns.
#>
[CmdletBinding()]
param(
    [string]$Exe = "apps\desktop\src-tauri\target\release\takyon.exe",
    [string]$OutDir = "$env:TEMP\takyon-verify-v0.7",
    [string]$Hotkey = "Ctrl+Alt+Shift+F9"
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type -Namespace Takyon -Name Drive7 -MemberDefinition @'
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(IntPtr h, out int pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll", SetLastError=true)] public static extern void keybd_event(byte vk, byte sc, uint f, System.UIntPtr x);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, System.UIntPtr e);
  public struct RECT { public int Left, Top, Right, Bottom; }
'@

# Without this the capture photographs the top-left fraction of a scaled display
# and every measured size is wrong by the scale factor. Silently.
[void][Takyon.Drive7]::SetProcessDPIAware()

$UP = 0x0002
$VK = @{ Esc = 0x1B; Ctrl = 0x11; Alt = 0x12; Shift = 0x10; F9 = 0x78; Comma = 0xBC }

function Send-Chord([byte[]]$Mods, [byte]$Key) {
    foreach ($m in $Mods) { [Takyon.Drive7]::keybd_event($m, 0, 0, [UIntPtr]::Zero) }
    [Takyon.Drive7]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 40
    [Takyon.Drive7]::keybd_event($Key, 0, $UP, [UIntPtr]::Zero)
    $rev = $Mods.Clone(); [array]::Reverse($rev)
    foreach ($m in $rev) { [Takyon.Drive7]::keybd_event($m, 0, $UP, [UIntPtr]::Zero) }
    Start-Sleep -Milliseconds 600
}

function Get-Front {
    $h = [Takyon.Drive7]::GetForegroundWindow()
    $pid_ = 0
    [void][Takyon.Drive7]::GetWindowThreadProcessId($h, [ref]$pid_)
    $r = New-Object Takyon.Drive7+RECT
    [void][Takyon.Drive7]::GetWindowRect($h, [ref]$r)
    $p = Get-Process -Id $pid_ -ErrorAction SilentlyContinue
    [pscustomobject]@{
        Handle = $h; Name = $p.ProcessName; Rect = $r
        Width = $r.Right - $r.Left; Height = $r.Bottom - $r.Top
    }
}

# Never type into someone else's window. Injected keys go wherever focus is, and
# this script types into a search box.
function Assert-Ours([string]$Step) {
    $f = Get-Front
    if ($f.Name -ne "takyon") { throw "${Step}: foreground is '$($f.Name)', not takyon" }
    return $f
}

function Show-Palette {
    Send-Chord @($VK.Ctrl, $VK.Alt, $VK.Shift) $VK.F9
    Start-Sleep -Milliseconds 500
}

function Save-Shot([string]$Tag) {
    $f = Get-Front
    $bmp = New-Object System.Drawing.Bitmap($f.Width, $f.Height)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($f.Rect.Left, $f.Rect.Top, 0, 0, $bmp.Size)
    $path = Join-Path $OutDir "$Tag.png"
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Output "  shot: $path"
    return $path
}

function Send-Text([string]$Text) {
    [System.Windows.Forms.SendKeys]::SendWait($Text)
    Start-Sleep -Milliseconds 400
}

function Click-At([int]$X, [int]$Y) {
    [void][Takyon.Drive7]::SetCursorPos($X, $Y)
    Start-Sleep -Milliseconds 120
    [Takyon.Drive7]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [Takyon.Drive7]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 500
}

<#
  How many of the captured pixels are neither the near-black plate nor pure
  white. A page that rendered has colour in it; a dead webview is one flat
  rectangle, which is exactly how the v0.6 Settings bug presented.
#>
function Measure-Colour([string]$Path) {
    $bmp = [System.Drawing.Bitmap]::FromFile($Path)
    $rect = New-Object System.Drawing.Rectangle(0, 0, $bmp.Width, $bmp.Height)
    $data = $bmp.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadOnly,
                          [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $bytes = New-Object byte[] ($data.Stride * $bmp.Height)
    [System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $bytes, 0, $bytes.Length)
    $bmp.UnlockBits($data)

    $coloured = 0; $total = 0
    for ($i = 0; $i -lt $bytes.Length; $i += 64) {
        $b = $bytes[$i]; $g = $bytes[$i + 1]; $r = $bytes[$i + 2]
        $total++
        $spread = ([Math]::Max($r, [Math]::Max($g, $b)) - [Math]::Min($r, [Math]::Min($g, $b)))
        if ($spread -gt 24) { $coloured++ }
    }
    $bmp.Dispose()
    return [pscustomobject]@{ Coloured = $coloured; Total = $total }
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
# The file index walks on a background thread; give it room before asking it
# anything. The walk is ~2 s warm on this machine.
Start-Sleep -Seconds 12
Get-Content $log -ErrorAction SilentlyContinue

# I1: `!e` answers from the real index, over the real drives.
Show-Palette
Assert-Ours "I1" | Out-Null
Send-Text "!e tesseract"
Start-Sleep -Milliseconds 700
Save-Shot "I1-file-bang" | Out-Null
Send-Chord @() $VK.Esc

# E1: the alias list, with icons served over `takyon-icon://` from the *settings*
# webview. This is the step the Playwright suite cannot reach: its mock returns
# "" for every icon URL, so it only ever draws the initial placeholder.
Show-Palette
Send-Chord @($VK.Ctrl) $VK.Comma
Start-Sleep -Milliseconds 1200
$settings = Assert-Ours "E1"
Write-Output "settings window $($settings.Width)x$($settings.Height)"

# The sidebar's tier-two list starts below the divider. "Applications" is its
# first entry. Physical pixels, because `SetProcessDPIAware` is called above and
# this machine runs at 150% - the logical offset lands on Advanced instead.
$x = $settings.Rect.Left + 120
$y = $settings.Rect.Top + 449
Click-At $x $y
Start-Sleep -Milliseconds 900
$shot = Save-Shot "E1-applications-icons"

$colour = Measure-Colour $shot
$pct = [Math]::Round(100 * $colour.Coloured / $colour.Total, 1)
Write-Output "E1: $($colour.Coloured)/$($colour.Total) sampled pixels carry colour ($pct%)"
if ($colour.Coloured -lt 40) {
    Write-Warning "E1 SUSPECT: almost no colour. Real icons are the only colourful thing on this page - a grey list means takyon-icon:// did not resolve in the settings webview."
}
else {
    Write-Output "E1 ok: the alias list is drawing real icons"
}

Write-Output "leaving pid $($app.Id) running; close it from the tray when done"
