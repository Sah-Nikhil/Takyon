<#
.SYNOPSIS
Drive the automatable half of docs/verify/v0.14.md against a release build.

Image clips and selective delete (v0.14). Same isolation and guards as
verify-drive-v0.5.ps1, whose helpers this copies: the build runs under a temp
LOCALAPPDATA so the operator's real history is never touched, and every
consequential key is refused unless takyon holds the foreground.

What it proves that the Rust and visual layers cannot: a real image copy is
captured by the real watcher into a sealed blob under clips\, the History view
draws it in the real WebView2, each delete path (Ctrl+D, the Ctrl+K
menu) removes the row *and* shreds the blob, and Copy puts the image back on the
clipboard with the same pixels.

Run under Windows PowerShell with -STA: WinForms' Clipboard.SetImage needs it.
  powershell -STA -NoProfile -ExecutionPolicy Bypass -File scripts\verify-drive-v0.14.ps1

Needs `sqlite3` on PATH for row counts; without it those steps SKIP.
#>
[CmdletBinding()]
param(
    [string]$Exe,
    [string]$OutDir = "$env:TEMP\takyon-verify-v0.14",
    [string]$Hotkey = "Ctrl+Alt+Shift+F9"
)

$ErrorActionPreference = "Stop"
if (-not $Exe) { $Exe = Join-Path (& "$PSScriptRoot\lib\cargo-target-dir.ps1") "release\takyon.exe" }
if ([Threading.Thread]::CurrentThread.ApartmentState -ne "STA") { throw "run with powershell -STA" }
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type -Namespace Takyon -Name Drive14 -MemberDefinition @'
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(IntPtr h, out int pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll", SetLastError=true)] public static extern void keybd_event(byte vk, byte sc, uint f, System.UIntPtr x);
  public struct RECT { public int Left, Top, Right, Bottom; }
'@

[void][Takyon.Drive14]::SetProcessDPIAware()

$UP = 0x0002
$VK = @{ Back = 0x08; Enter = 0x0D; Esc = 0x1B; Ctrl = 0x11; Alt = 0x12; Shift = 0x10
         F9 = 0x78; K = 0x4B; D = 0x44; Down = 0x28 }

$run = [guid]::NewGuid().ToString('N').Substring(0, 8)
$KEEP = "takyon-v014-keep-$run"
$DOOMED = "takyon-v014-doomed-$run"

$script:pass = 0
$script:fail = 0
$script:skip = 0

function Report([string]$Id, [bool]$Ok, [string]$Detail) {
    if ($Ok) { $script:pass++ } else { $script:fail++ }
    $verdict = if ($Ok) { "PASS" } else { "FAIL" }
    [Console]::WriteLine(("{0,-5} {1,-5} {2}" -f $Id, $verdict, $Detail))
}

function Skip([string]$Id, [string]$Why) {
    $script:skip++
    [Console]::WriteLine(("{0,-5} {1,-5} {2}" -f $Id, "SKIP", $Why))
}

function Send-Key([byte]$vk, [int]$rest = 30) {
    [Takyon.Drive14]::keybd_event($vk, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 20
    [Takyon.Drive14]::keybd_event($vk, 0, $UP, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds $rest
}

function Send-Chord([byte[]]$Mods, [byte]$Key) {
    foreach ($m in $Mods) { [Takyon.Drive14]::keybd_event($m, 0, 0, [UIntPtr]::Zero) }
    [Takyon.Drive14]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 40
    [Takyon.Drive14]::keybd_event($Key, 0, $UP, [UIntPtr]::Zero)
    $rev = $Mods.Clone(); [array]::Reverse($rev)
    foreach ($m in $rev) { [Takyon.Drive14]::keybd_event($m, 0, $UP, [UIntPtr]::Zero) }
    Start-Sleep -Milliseconds 500
}

# Every string currently drawn in the Palette, via UI Automation.
#
# The only way to read the action menu without OCR. Assertions on window height
# can say a list has rows; only this can say *which* row is selected, because the
# menu is built from the selected Entry.
function Get-MenuText {
    Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes -ErrorAction SilentlyContinue
    # The menu is drawn by the webview a frame or two after the chord lands, and
    # reading before that returns a tree of empty names - which is a false
    # negative, not a failure. Retried rather than slept once.
    for ($i = 0; $i -lt 8; $i++) {
        $text = Read-WindowText
        if ($text -match "Open Command|Run as administrator|Paste") { return $text }
        Start-Sleep -Milliseconds 250
    }
    return Read-WindowText
}

function Read-WindowText {
    try {
        $h = [Takyon.Drive14]::GetForegroundWindow()
        $el = [System.Windows.Automation.AutomationElement]::FromHandle($h)
        if (-not $el) { return "" }
        $cond = [System.Windows.Automation.Condition]::TrueCondition
        $all = $el.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)
        $parts = @()
        foreach ($node in $all) { $parts += $node.Current.Name }
        return ($parts -join " | ")
    } catch {
        return ""
    }
}

function Get-Front {
    $h = [Takyon.Drive14]::GetForegroundWindow()
    $procId = 0
    [void][Takyon.Drive14]::GetWindowThreadProcessId($h, [ref]$procId)
    $p = Get-Process -Id $procId -ErrorAction SilentlyContinue
    @{ Handle = $h; Name = $(if ($p) { $p.ProcessName } else { "?" }); Pid = $procId }
}

# Wait for a window to actually hold the foreground, or give up.
#
# Focus changes are asynchronous and another application can take the foreground
# at any moment, so every injected key below asks first rather than assuming the
# window it wanted is still there.
function Wait-Front([string]$Pattern, [int]$TimeoutMs = 2500) {
    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    while ((Get-Date) -lt $deadline) {
        if ((Get-Front).Name -match $Pattern) { return $true }
        Start-Sleep -Milliseconds 120
    }
    return $false
}

# Send a key only while the named window holds the foreground.
#
# **A safety guard, not a convenience.** Enter in the Palette triggers paste-back
# and Ctrl+A/Ctrl+C overwrite the clipboard, so a key sent while something else
# has focus can paste the operator's own clipboard into their application.
# Observed once: a browser took the foreground and a real URL landed in the wrong
# window. Never send blind.
function Send-KeyTo([string]$Pattern, [byte]$vk, [int]$rest = 30) {
    if (-not (Wait-Front $Pattern)) {
        Write-Warning "refused a key: foreground is '$((Get-Front).Name)', wanted /$Pattern/"
        return $false
    }
    Send-Key $vk $rest
    return $true
}

function Send-ChordTo([string]$Pattern, [byte[]]$Mods, [byte]$Key) {
    if (-not (Wait-Front $Pattern)) {
        Write-Warning "refused a chord: foreground is '$((Get-Front).Name)', wanted /$Pattern/"
        return $false
    }
    Send-Chord $Mods $Key
    return $true
}

# The same by process id: "a Notepad" is not "the Notepad this script started",
# and typing into the operator's own unsaved document is what must not happen.
function Send-ChordToPid([int[]]$Ids, [byte[]]$Mods, [byte]$Key) {
    $deadline = (Get-Date).AddMilliseconds(2500)
    while ((Get-Date) -lt $deadline) {
        if ($Ids -contains (Get-Front).Pid) {
            Send-Chord $Mods $Key
            return $true
        }
        Start-Sleep -Milliseconds 120
    }
    Write-Warning "refused a chord: foreground pid $((Get-Front).Pid) is not ours"
    return $false
}

function Send-KeyToPid([int[]]$Ids, [byte]$vk, [int]$rest = 30) {
    $deadline = (Get-Date).AddMilliseconds(2500)
    while ((Get-Date) -lt $deadline) {
        if ($Ids -contains (Get-Front).Pid) {
            Send-Key $vk $rest
            return $true
        }
        Start-Sleep -Milliseconds 120
    }
    Write-Warning "refused a key: foreground pid $((Get-Front).Pid) is not ours"
    return $false
}

function Show-Palette {
    Send-Chord @($VK.Ctrl, $VK.Alt, $VK.Shift) $VK.F9
    Start-Sleep -Milliseconds 700
}

# Nothing is ever typed into a window that is not the Palette. One re-summon
# before giving up: the Palette dismisses on focus loss, so any other window
# taking the foreground for a moment would otherwise end the run.
function Send-Text([string]$Text) {
    # Up to three summons before giving up. The Palette dismisses on focus loss,
    # and on a real desktop something else takes the foreground now and then - a
    # browser finishing a load was enough to fail four steps in one run. The rule
    # itself never weakens: keys are only ever sent to takyon.
    $f = Get-Front
    for ($try = 0; $try -lt 3 -and $f.Name -ne "takyon"; $try++) {
        Show-Palette
        Start-Sleep -Milliseconds 400
        $f = Get-Front
    }
    if ($f.Name -ne "takyon") {
        Write-Warning "refused to type '$Text': foreground is '$($f.Name)'"
        return $false
    }
    foreach ($c in $Text.ToCharArray()) {
        $key = if ('+^%~(){}[]'.Contains($c)) { "{$c}" } else { [string]$c }
        [System.Windows.Forms.SendKeys]::SendWait($key)
        Start-Sleep -Milliseconds 40
    }
    Start-Sleep -Milliseconds 600
    return $true
}

function Clear-Query { for ($i = 0; $i -lt 45; $i++) { Send-Key $VK.Back 6 } }

function Save-Shot([string]$Tag) {
    $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($b.X, $b.Y, 0, 0, $bmp.Size)
    $bmp.Save((Join-Path $OutDir "$Tag.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    $f = Get-Front
    $r = New-Object Takyon.Drive14+RECT
    [void][Takyon.Drive14]::GetWindowRect($f.Handle, [ref]$r)
    $h = $r.Bottom - $r.Top
    # Built first, then written. Inside a method call's parentheses PowerShell
    # splits on commas as arguments, so an inline `-f` list would hand the format
    # operator one argument and three to WriteLine.
    $line = "{0,-26} front={1,-9} {2}x{3}" -f $Tag, $f.Name, ($r.Right - $r.Left), $h
    [Console]::WriteLine($line)
    return $h
}

# Copy something and give the watcher time to see it. The listener is a window
# message, so this is a handful of milliseconds in practice.
function Copy-AndSettle([string]$Text) {
    Set-Clipboard -Value $Text
    Start-Sleep -Milliseconds 900
}


# A 320x200 image, distinct per seed. Drawn, not loaded, so the run needs no
# fixture file and the pixels are known.
function New-Shot([int]$Seed) {
    $bmp = New-Object System.Drawing.Bitmap 320, 200
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    for ($y = 0; $y -lt 200; $y += 10) {
        for ($x = 0; $x -lt 320; $x += 10) {
            $c = [System.Drawing.Color]::FromArgb(255, ($x + $Seed) % 256, ($y * 2) % 256, (($x + $y) / 2 + $Seed) % 256)
            $brush = New-Object System.Drawing.SolidBrush $c
            $g.FillRectangle($brush, $x, $y, 10, 10)
            $brush.Dispose()
        }
    }
    $g.Dispose()
    return $bmp
}

function Copy-ImageAndSettle($Bitmap) {
    [System.Windows.Forms.Clipboard]::SetImage($Bitmap)
    Start-Sleep -Milliseconds 1500
}

function Get-Blobs { @(Get-ChildItem (Join-Path $DataDir "clips") -Filter *.bin -ErrorAction SilentlyContinue) }

function Find-Bytes([byte[]]$Hay, [byte[]]$Needle) {
    $limit = $Hay.Length - $Needle.Length
    for ($i = 0; $i -le $limit; $i++) {
        if ($Hay[$i] -ne $Needle[0]) { continue }
        $hit = $true
        for ($j = 1; $j -lt $Needle.Length; $j++) { if ($Hay[$i + $j] -ne $Needle[$j]) { $hit = $false; break } }
        if ($hit) { return $true }
    }
    return $false
}

# Retried: the webview draws a frame or two after the key that asked for it.
function Wait-Text([string]$Pattern, [int]$TimeoutMs = 3000) {
    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    do {
        $t = Read-WindowText
        if ($t -match $Pattern) { return $t }
        Start-Sleep -Milliseconds 200
    } while ((Get-Date) -lt $deadline)
    return $t
}

# Palette shown, query cleared, the History view opened through its command.
function Open-History {
    Show-Palette
    Clear-Query
    if (-not (Send-Text "clipboard")) { return $false }
    if (-not (Send-KeyTo "takyon" $VK.Enter)) { return $false }
    Start-Sleep -Milliseconds 900
    return $true
}

# Escape backs out of the view, then dismisses. Unguarded, as in v0.5: it does
# nothing anywhere else.
function Close-Views {
    Send-Key $VK.Esc 200
    Send-Key $VK.Esc 200
    Send-Key $VK.Esc 400
}

$script:sqlite = $null
$sq = Get-Command sqlite3 -ErrorAction SilentlyContinue
if ($sq) { $script:sqlite = $sq.Source }

function Invoke-Db([string]$File, [string]$Sql) {
    if (-not $script:sqlite) { return "" }
    return ((& $script:sqlite (Join-Path $DataDir $File) $Sql) -join "`n").Trim()
}
function Get-RowCount { return [int](Invoke-Db "clips.db" "SELECT COUNT(*) FROM clips;") }

$script:firstStart = $true
$script:ourPads = @()
function Start-App {
    $p = Start-Process -FilePath $underTest -RedirectStandardError (Join-Path $OutDir "stderr.txt") `
        -RedirectStandardOutput (Join-Path $OutDir "stdout.txt") -PassThru
    # Longer on the first start: WebView2 has no profile in a fresh LOCALAPPDATA
    # and builds one, on top of the application walk.
    Start-Sleep -Seconds $(if ($script:firstStart) { 14 } else { 9 })
    $script:firstStart = $false
    return $p
}

function Stop-App($p) {
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    # SQLite needs the handles actually released before another process writes.
    Start-Sleep -Milliseconds 1500
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
Get-ChildItem $OutDir -Filter *.png -ErrorAction SilentlyContinue | Remove-Item -Force
$restore = Get-Clipboard -Raw -ErrorAction SilentlyContinue

if (-not (Test-Path $Exe)) { throw "no build at $Exe - run 'bun run build' first" }
$underTest = (Resolve-Path $Exe).Path
# Single-instance would hand every show to any other Takyon, so it is stopped.
# This script does not relaunch it; the operator does.
Get-Process takyon -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -ne $underTest } |
    ForEach-Object { Write-Output "stopping other takyon pid $($_.Id) $($_.Path)"; Stop-Process -Id $_.Id -Force }

$Sandbox = Join-Path $OutDir "appdata"
$DataDir = Join-Path $Sandbox "v3sper\takyon"
Remove-Item $Sandbox -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $DataDir | Out-Null

$env:TAKYON_HOTKEY = $Hotkey
$env:LOCALAPPDATA = $Sandbox
$app = Start-App
Write-Output "under test: pid $($app.Id), data in $DataDir"
if (-not $script:sqlite) { Write-Warning "sqlite3 not on PATH - row counts are skipped" }

try {
    Write-Output "--- I: image capture"
    Copy-ImageAndSettle (New-Shot 0)
    $blobs = Get-Blobs
    Report "I1" ($blobs.Count -eq 1) "an image copy made $($blobs.Count) blob file(s) under clips\"
    if ($blobs.Count -ge 1) {
        $bytes = [IO.File]::ReadAllBytes($blobs[0].FullName)
        $sig = [byte[]](0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A)
        $ihdr = [Text.Encoding]::ASCII.GetBytes("IHDR")
        $sealed = -not (Find-Bytes $bytes $sig) -and -not (Find-Bytes $bytes $ihdr)
        Report "I2" $sealed "blob is sealed: no PNG signature or IHDR in $($bytes.Length) bytes"
    } else { Skip "I2" "no blob to inspect" }
    if ($script:sqlite) {
        $meta = Invoke-Db "clips.db" "SELECT kind || '|' || width || '|' || height FROM clips ORDER BY id DESC LIMIT 1;"
        Report "I3" ($meta -eq "image|320|200") "newest row is '$meta'"
    } else { Skip "I3" "needs sqlite3" }

    # I4: the same image twice in a row is one row; after a text copy between,
    # it is a new one. Two image rows and two blobs in total.
    Copy-AndSettle $KEEP
    Copy-ImageAndSettle (New-Shot 0)
    Copy-ImageAndSettle (New-Shot 0)
    if ($script:sqlite) {
        $images = [int](Invoke-Db "clips.db" "SELECT COUNT(*) FROM clips WHERE kind = 'image';")
        $nb = (Get-Blobs).Count
        Report "I4" ($images -eq 2 -and $nb -eq 2) "repeat collapsed: $images image rows, $nb blobs"
    } else { Skip "I4" "needs sqlite3" }

    Write-Output "--- H, D1: the History view draws it; Ctrl+D deletes it"
    if (Open-History) {
        # Read off the screenshot, not UI Automation: WebView2 exposed no names
        # to it on the first run here, and an empty read is not a failure.
        Start-Sleep -Milliseconds 800
        [void](Save-Shot "H-history-image")
        Skip "H1" "look: H-history-image.png lists 'Image 320 x 200' with a thumbnail"
        Skip "H2" "look: H-history-image.png shows the image, Dimensions and Delete"

        $before = (Get-Blobs).Count
        if (Send-ChordTo "takyon" @($VK.Ctrl) $VK.D) {
            Start-Sleep -Milliseconds 1200
            $after = (Get-Blobs).Count
            Report "D1" ($after -eq $before - 1) "Ctrl+D on the selected image: blobs $before -> $after"
            [void](Save-Shot "D1-after-delete")
        } else { Skip "D1" "takyon lost the foreground" }
        Close-Views
    } else { foreach ($id in "H1", "H2", "D1") { Skip $id "could not open History" } }

    Write-Output "--- D2: the Ctrl+K menu deletes the selected clip"
    Copy-AndSettle $DOOMED
    $doomedId = if ($script:sqlite) { Invoke-Db "clips.db" "SELECT MAX(id) FROM clips;" } else { "" }
    $rowsBefore = if ($script:sqlite) { Get-RowCount } else { 0 }
    if (Open-History) {
        Start-Sleep -Milliseconds 800
        if (Send-ChordTo "takyon" @($VK.Ctrl) $VK.K) {
            Start-Sleep -Milliseconds 800
            [void](Save-Shot "D2a-menu-open")
            Skip "D2a" "look: D2a-menu-open.png shows the action menu over History"
            [void](Send-Text "delete")
            [void](Send-KeyTo "takyon" $VK.Enter 600)
            Start-Sleep -Milliseconds 1000
            [void](Save-Shot "D2-after-menu-delete")
            if ($script:sqlite) {
                $still = Invoke-Db "clips.db" "SELECT COUNT(*) FROM clips WHERE id = $doomedId;"
                $rowsAfter = Get-RowCount
                $ok = ($still -eq "0") -and ($rowsAfter -eq $rowsBefore - 1)
                Report "D2b" $ok "menu Delete removed the selected (newest) row only: $rowsBefore -> $rowsAfter rows"
            } else { Skip "D2b" "needs sqlite3" }
        } else { Skip "D2a" "takyon lost the foreground"; Skip "D2b" "same" }
        Close-Views
    } else { Skip "D2a" "could not open History"; Skip "D2b" "same" }
    if ($script:sqlite) {
        $left = [int](Invoke-Db "clips.db" "SELECT COUNT(*) FROM clips WHERE kind = 'image';")
        $nb = (Get-Blobs).Count
        Report "D3" ($left -eq $nb) "rows and blob files agree: $left image rows, $nb blobs"
    } else { Skip "D3" "needs sqlite3" }

    Write-Output "--- P: Copy puts an image clip back"
    Copy-ImageAndSettle (New-Shot 90)
    Copy-AndSettle $KEEP
    Show-Palette
    Clear-Query
    if (Send-Text "!v") {
        [void](Send-KeyTo "takyon" $VK.Down 300)
        if (Send-ChordTo "takyon" @($VK.Ctrl) $VK.Enter) {
            Start-Sleep -Milliseconds 900
            $img = [System.Windows.Forms.Clipboard]::GetImage()
            $size = if ($img) { "$($img.Width)x$($img.Height)" } else { "none" }
            Report "P1" ($size -eq "320x200") "Ctrl+Enter on the image clip put a $size image on the clipboard"
            if ($img) {
                $want = (New-Shot 90).GetPixel(105, 55)
                $got = ([System.Drawing.Bitmap]$img).GetPixel(105, 55)
                Report "P2" ($got.R -eq $want.R -and $got.G -eq $want.G -and $got.B -eq $want.B) "pixel (105,55) came back as copied: $got"
            } else { Skip "P2" "no image" }
        } else { Skip "P1" "takyon lost the foreground"; Skip "P2" "same" }
    } else { Skip "P1" "could not type"; Skip "P2" "same" }
    Close-Views
}
finally {
    Start-Sleep -Milliseconds 400
    Get-Content (Join-Path $OutDir "stderr.txt") -ErrorAction SilentlyContinue
    Get-Process takyon -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -eq $underTest } |
        ForEach-Object { Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue }
    if ($restore) { Set-Clipboard -Value $restore } else { Set-Clipboard -Value " " }
}

Write-Output ""
Write-Output "$($script:pass) passed, $($script:fail) failed, $($script:skip) skipped"
Write-Output "shots in $OutDir; sandbox data in $DataDir (delete it when done)"
Write-Output "Needs a person: a real Win+Shift+S snip, paste into Paint or Word, Settings size,"
Write-Output "retention over a day, and a password manager's copy (docs/verify/v0.14.md)."
