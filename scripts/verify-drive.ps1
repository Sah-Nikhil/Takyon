<#
.SYNOPSIS
Drive the Palette through the automatable half of docs/verify/v0.2.md.

Sections C, D and F are icons, window shape and keyboard, which a person can
check but cannot check repeatably. This runs them against a release build,
captures the screen after every step, and prints the window size each step
produced - the sizes alone catch most regressions without opening an image.

Two things it must do and does. It refuses to type unless GetForegroundWindow
resolves to takyon, because injected keys go wherever focus is. And it launches
the binary itself, because a GUI process started from a tool call is reaped when
that call returns. Steps needing a person are named at the end of the run.
#>
[CmdletBinding()]
param(
    [string]$Exe = "apps\desktop\src-tauri\target\release\takyon.exe",
    [string]$OutDir = "$env:TEMP\takyon-verify",
    # Free chord. Alt+Space is contested by Raycast and PowerToys Run on most
    # machines this is developed on, and a failed registration adds the amber
    # banner, which changes every window size measured below.
    [string]$Hotkey = "Ctrl+Alt+Shift+F9"
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type -Namespace Takyon -Name Drive -MemberDefinition @'
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(IntPtr h, out int pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll", SetLastError=true)] public static extern void keybd_event(byte vk, byte sc, uint f, System.UIntPtr x);
  public struct RECT { public int Left, Top, Right, Bottom; }
'@

# Without this the capture photographs the top-left fraction of a scaled
# display and every measured size is wrong by the scale factor. Silently.
[void][Takyon.Drive]::SetProcessDPIAware()

$UP = 0x0002
$VK = @{ Back = 0x08; Esc = 0x1B; Space = 0x20; Down = 0x28; Ctrl = 0x11; Alt = 0x12; Shift = 0x10; F9 = 0x78; K = 0x4B; C = 0x43 }

function Send-Key([byte]$vk, [int]$rest = 30) {
    [Takyon.Drive]::keybd_event($vk, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 20
    [Takyon.Drive]::keybd_event($vk, 0, $UP, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds $rest
}

function Send-Chord([byte[]]$Mods, [byte]$Key) {
    foreach ($m in $Mods) { [Takyon.Drive]::keybd_event($m, 0, 0, [UIntPtr]::Zero) }
    [Takyon.Drive]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 40
    [Takyon.Drive]::keybd_event($Key, 0, $UP, [UIntPtr]::Zero)
    $rev = $Mods.Clone(); [array]::Reverse($rev)
    foreach ($m in $rev) { [Takyon.Drive]::keybd_event($m, 0, $UP, [UIntPtr]::Zero) }
    Start-Sleep -Milliseconds 450
}

function Get-Front {
    $h = [Takyon.Drive]::GetForegroundWindow()
    $procId = 0
    [void][Takyon.Drive]::GetWindowThreadProcessId($h, [ref]$procId)
    $p = Get-Process -Id $procId -ErrorAction SilentlyContinue
    @{ Handle = $h; Name = $(if ($p) { $p.ProcessName } else { "?" }) }
}

function Show-Palette {
    Send-Chord @($VK.Ctrl, $VK.Alt, $VK.Shift) $VK.F9
    Start-Sleep -Milliseconds 600
}

# The guard. Nothing is ever typed into a window that is not the Palette.
#
# It re-summons rather than giving up on the first miss. The Palette hides on
# focus loss by design, so any window that steals foreground mid-run - Explorer
# finishing a launch, a heavy app painting its splash - takes it away and every
# later step then refuses. That is the guard working, but it made the script
# unrunnable on a busy desktop rather than merely careful.
function Send-Text([string]$Text) {
    for ($try = 1; (Get-Front).Name -ne "takyon" -and $try -le 3; $try++) {
        Write-Output "  (re-summoning; foreground was '$((Get-Front).Name)')"
        # The hotkey toggles, and a Palette that lost focus is already hidden, so
        # this shows it rather than hiding it.
        Show-Palette
    }
    $f = Get-Front
    if ($f.Name -ne "takyon") {
        Write-Warning "refused to type '$Text': foreground is '$($f.Name)' after 3 attempts"
        return $false
    }
    foreach ($c in $Text.ToCharArray()) {
        if ($c -match '[a-zA-Z0-9]') { Send-Key ([byte][int][char]([string]$c).ToUpper()) 55 }
        elseif ($c -eq ' ') { Send-Key $VK.Space 55 }
    }
    Start-Sleep -Milliseconds 500
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
    $r = New-Object Takyon.Drive+RECT
    [void][Takyon.Drive]::GetWindowRect($f.Handle, [ref]$r)
    Write-Output ("{0,-26} front={1,-9} {2}x{3}" -f $Tag, $f.Name, ($r.Right - $r.Left), ($r.Bottom - $r.Top))
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

# A2 wants the Palette open before the walk finishes, so this summon comes first
# and without the settling delay every other step gets.
Start-Sleep -Milliseconds 250
Show-Palette
Save-Shot "A2-during-walk"
Send-Key $VK.Esc 300

Start-Sleep -Seconds 6
Get-Content $log

Show-Palette
Save-Shot "D1-empty-query"

# C1 asks for a cold profile. icons.bin currently holds zero icons, so every
# launch already is one - see docs/tbd/v0.2.md section 10. Two captures 900 ms
# apart show whether rows waited for their icons.
Clear-Query
if (Send-Text "phot") { Save-Shot "C1-cold-immediate"; Start-Sleep -Milliseconds 900; Save-Shot "C1-cold-settled" }

Clear-Query; if (Send-Text "adobe photoshop 2022") { Save-Shot "B4-exact-full-name" }
Clear-Query; if (Send-Text "c") { Save-Shot "D3-eight-row-cap" }

# D8: dismiss holding a full list, then re-summon. One input row, not a tall box.
Send-Key $VK.Esc 400
Show-Palette
Save-Shot "D8-resummon-after-full-list"

# C4: the same query again in the same session, icons already resolved.
Clear-Query; if (Send-Text "phot") { Save-Shot "C4-resummon-icons" }

Clear-Query
if (Send-Text "charmap") {
    Save-Shot "D4-one-row-no-scrollbar"
    Send-Chord @($VK.Ctrl) $VK.K
    Save-Shot "D5-menu-fits"
    if (Send-Text "admin") { Save-Shot "F6-menu-filter" }
    Send-Key $VK.Esc 400
    Save-Shot "F4-esc-closes-menu-only"
}

# E7 is the one action whose result is checkable and which starts nothing.
Set-Clipboard -Value "takyon-verify-sentinel"
Clear-Query
if (Send-Text "charmap") {
    Send-Chord @($VK.Ctrl, $VK.Shift) $VK.C
    Start-Sleep -Milliseconds 800
    Write-Output "E7 clipboard: $(Get-Clipboard)"
}

Show-Palette
Clear-Query; if (Send-Text "phot") { Send-Key $VK.Down 250; Save-Shot "D9-row-inset" }
Send-Key $VK.Esc 400
Save-Shot "F5-esc-hides-palette"

Write-Output "--- icons.bin ---"
$blob = "$env:LOCALAPPDATA\v3sper\launcher\icons.bin"
if (Test-Path $blob) { Get-Item $blob | Select-Object Length, LastWriteTime | Format-List }

Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
Write-Output "stopped pid $($app.Id)"
Write-Output ""
Write-Output "Needs a person, not this script:"
Write-Output "  A6 E3        a Steam library with a game in it"
Write-Output "  A8           an uninstall"
Write-Output "  E5 F2        the UAC prompt"
Write-Output "  D2           'no animation' - a still capture cannot see it"
Write-Output "  B5           the seq rule while typing - needs video"
Write-Output "  E4 E6 F1 F3  these start real applications; run them by hand"
Write-Output ""
Write-Output "shots: $OutDir"
