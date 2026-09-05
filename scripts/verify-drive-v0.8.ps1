<#
.SYNOPSIS
Drive the automatable half of docs/verify/v0.8.md.

The Playwright suite reaches every v0.8 surface through Vite and none of them
through Tauri, so it cannot see the half that is the native window: whether the
Palette is the right height for the one-line `!c` row, whether it grows to the
surface height when an answer opens, and whether a follow-up stays in that one
window rather than trying to open another.

Same two rules as scripts/verify-drive.ps1. It refuses to type unless the
foreground window belongs to takyon, and it launches the binary itself, because a
GUI process started from a tool call is reaped when that call returns.

A real Turn costs tokens and needs the network. Sections 5, 6, 8 and 9 of the
script stay manual; this drives 1, 2, 3 and 4.
#>
[CmdletBinding()]
param(
    [string]$Exe = "apps\desktop\src-tauri\target\release\takyon.exe",
    [string]$OutDir = "$env:TEMP\takyon-verify-v0.8",
    # Free chord. Alt+Space is contested by Raycast and PowerToys Run on most
    # machines this is developed on.
    [string]$Hotkey = "Ctrl+Alt+Shift+F9",
    [string]$Question = "Reply with exactly one word: ok",
    # How long the first probe is given before the row is sampled. `!c` no
    # longer waits for it to ask — the enabled set is a stored preference — but
    # the row only names the Agent's own label once the probe has answered.
    [int]$ProbeSeconds = 12,
    # How long a Turn is given before its answer is sampled. A cold `claude`
    # start plus a model round trip; generous, because a false failure here reads
    # as a rendering bug.
    [int]$AnswerSeconds = 60
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type -Namespace Takyon -Name Drive9 -MemberDefinition @'
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(IntPtr h, out int pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll", SetLastError=true)] public static extern void keybd_event(byte vk, byte sc, uint f, System.UIntPtr x);
  public struct RECT { public int Left, Top, Right, Bottom; }
'@

# Without this the capture photographs the top-left fraction of a scaled display
# and every measured size is wrong by the scale factor. Silently.
[void][Takyon.Drive9]::SetProcessDPIAware()

$UP = 0x0002
$VK = @{ Esc = 0x1B; Ctrl = 0x11; Alt = 0x12; Shift = 0x10; F4 = 0x73; F9 = 0x78; Enter = 0x0D }

function Send-Chord([byte[]]$Mods, [byte]$Key) {
    foreach ($m in $Mods) { [Takyon.Drive9]::keybd_event($m, 0, 0, [UIntPtr]::Zero) }
    [Takyon.Drive9]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 40
    [Takyon.Drive9]::keybd_event($Key, 0, $UP, [UIntPtr]::Zero)
    $rev = $Mods.Clone(); [array]::Reverse($rev)
    foreach ($m in $rev) { [Takyon.Drive9]::keybd_event($m, 0, $UP, [UIntPtr]::Zero) }
    Start-Sleep -Milliseconds 600
}

function Send-Key([byte]$vk, [int]$rest = 300) {
    [Takyon.Drive9]::keybd_event($vk, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 20
    [Takyon.Drive9]::keybd_event($vk, 0, $UP, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds $rest
}

function Get-Front {
    $h = [Takyon.Drive9]::GetForegroundWindow()
    $procId = 0
    [void][Takyon.Drive9]::GetWindowThreadProcessId($h, [ref]$procId)
    $p = Get-Process -Id $procId -ErrorAction SilentlyContinue
    $r = New-Object Takyon.Drive9+RECT
    [void][Takyon.Drive9]::GetWindowRect($h, [ref]$r)
    @{
        Handle = $h
        Name   = $(if ($p) { $p.ProcessName } else { "?" })
        Width  = $r.Right - $r.Left
        Height = $r.Bottom - $r.Top
    }
}

# Typed rather than pasted: the clipboard is a shared resource and v0.5's history
# would record whatever this script put there.
function Send-Text([string]$Text) {
    $f = Get-Front
    if ($f.Name -ne "takyon") { throw "refusing to type into '$($f.Name)'" }
    [System.Windows.Forms.SendKeys]::SendWait(
        ($Text -replace '[+^%~(){}\[\]]', '{$0}')
    )
    Start-Sleep -Milliseconds 500
}

function Show-Palette {
    Send-Chord @($VK.Ctrl, $VK.Alt, $VK.Shift) $VK.F9
    Start-Sleep -Milliseconds 500
}

# Does the foreground window actually have a page in it?
#
# A window whose webview never loaded is a title bar over an opaque white
# rectangle. Every other check passes - it exists, it is the right size - and
# only the pixels say it is empty.
function Test-Painted([string]$Tag) {
    $f = Get-Front
    $r = New-Object Takyon.Drive9+RECT
    [void][Takyon.Drive9]::GetWindowRect($f.Handle, [ref]$r)
    $x = $r.Left + 40; $y = $r.Top + 90
    $w = ($r.Right - $r.Left) - 80; $h = ($r.Bottom - $r.Top) - 130
    if ($w -le 0 -or $h -le 0) { Write-Warning "${Tag}: window too small to sample"; return $false }

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
    # Write-Host, not Write-Output: this function returns a value, and anything
    # written to the pipeline is captured by the caller instead of the console.
    # The first draft did that, and the only check the script exists for was
    # silently invisible in its own report.
    if ($pct -gt 90) {
        Write-Warning "$Tag FAILED: $pct% of the window is blank white - the webview did not load"
        return $false
    }
    Write-Host "$Tag ok: window is painted ($pct% white)"
    return $true
}

function Save-Shot([string]$Tag) {
    $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($b.X, $b.Y, 0, 0, $bmp.Size)
    $bmp.Save((Join-Path $OutDir "$Tag.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    $f = Get-Front
    Write-Host ("{0,-26} front={1,-9} {2}x{3}" -f $Tag, $f.Name, $f.Width, $f.Height)
    return $f
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
Get-ChildItem $OutDir -Filter *.png -ErrorAction SilentlyContinue | Remove-Item -Force

if (-not (Test-Path $Exe)) { throw "no build at $Exe - run 'bun run build' first" }
$underTest = (Resolve-Path $Exe).Path

# Any running Takyon swallows every summon through single-instance and the build
# under test never runs, silently.
#
# **Every** one, not just a different build. A stale copy of this same binary,
# left by a run that threw before its cleanup, is the worse case: the imposter
# check passes, the new process hands off and exits, and the hotkey belongs to a
# process this script is not driving. The symptom is the type-guard below
# refusing on a foreground window that is neither Takyon nor obviously wrong.
Get-Process takyon -ErrorAction SilentlyContinue |
    ForEach-Object {
        $which = if ($_.Path -eq $underTest) { "stale" } else { "imposter" }
        Write-Output "stopping $which pid $($_.Id) $($_.Path)"
        Stop-Process -Id $_.Id -Force
    }
Start-Sleep -Milliseconds 500

$env:TAKYON_HOTKEY = $Hotkey
$log = Join-Path $OutDir "stderr.txt"
$app = Start-Process -FilePath $Exe -RedirectStandardError $log `
    -RedirectStandardOutput (Join-Path $OutDir "stdout.txt") -PassThru
Write-Output "under test: pid $($app.Id)"

# Stop the child however this script ends, including on a throw. Without it the
# type-guard's refusal leaves a Takyon running, and the *next* run then hands off
# to it through single-instance and drives the wrong process.
trap {
    Write-Warning "aborted: $_"
    Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
    break
}

Start-Sleep -Seconds 6
Get-Content $log -ErrorAction SilentlyContinue

# 1: `!c` is one row, and the native window is that tall rather than empty-height.
Show-Palette
Send-Text "!c"
Start-Sleep -Seconds $ProbeSeconds   # the first probe is three process spawns
$bang = Save-Shot "1-bang-only"
if ($bang.Height -lt 100) {
    Write-Warning "1 FAILED: palette is $($bang.Height)px - the !c row has no reserved space"
}
else {
    Write-Output "1 ok: palette grew to $($bang.Height)px for the !c row"
}

# 1b: a question turns the row into the Enter prompt. Still one row.
Send-Text " $Question"
$ready = Save-Shot "1b-question-typed"
Write-Output "1b: palette $($ready.Width)x$($ready.Height) with a question typed"

# 2: Enter grows the window to the surface height, once, and the answer paints.
Send-Key $VK.Enter 1200
$view = Save-Shot "2a-ask-opened"
if ($view.Height -lt 400) {
    Write-Warning "2 FAILED: the ask view is $($view.Height)px - set_view never resized the window"
}
else {
    Write-Output "2 ok: the ask view opened at $($view.Height)px"
}
Write-Output "2: waiting up to $AnswerSeconds s for an answer"
Start-Sleep -Seconds $AnswerSeconds
Save-Shot "2b-answered" | Out-Null
if (-not (Test-Painted "2-painted")) {
    Write-Warning "2 FAILED: the ask view drew nothing"
}

# 4: a follow-up continues in the same window. One takyon window, never two.
Send-Text "and why is that"
Send-Key $VK.Enter 3000
$chat = Save-Shot "4a-followup"
if ($chat.Name -ne "takyon") {
    Write-Warning "4 FAILED: the follow-up left the foreground on '$($chat.Name)'"
}
elseif ($chat.Handle -ne $view.Handle) {
    Write-Warning "4 FAILED: a second window opened - the follow-up must stay here"
}
else {
    Write-Output "4 ok: the conversation continued in the same window ($($chat.Height)px)"
}
if (-not (Test-Painted "4-painted")) {
    Write-Warning "4 FAILED: the conversation drew nothing"
}

# 3: the Chat Surface outlives the Palette, and closing it does not end Takyon.
Show-Palette
Send-Key $VK.Esc 800
Start-Sleep -Milliseconds 800
Save-Shot "3a-palette-dismissed" | Out-Null
$stillThere = Get-Process -Id $app.Id -ErrorAction SilentlyContinue
if (-not $stillThere) {
    Write-Warning "3 FAILED: dismissing the Palette ended the process"
}
else {
    Write-Output "3 ok: pid $($app.Id) alive after the Palette was dismissed"
}

Get-Content $log -ErrorAction SilentlyContinue
Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
Write-Output ""
Write-Output "shots in $OutDir"
Write-Output "still needs a person: sections 5 to 9 of docs/verify/v0.8.md - the"
Write-Output "tools-off proof, the locked model and effort, every Sign-in state, the"
Write-Output "no-console-flash check and the working-directory control. All of them"
Write-Output "need state this script must not create on the operator's machine."
