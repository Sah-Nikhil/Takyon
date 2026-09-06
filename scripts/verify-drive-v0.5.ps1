<#
.SYNOPSIS
Drive the automatable half of docs/verify/v0.5.md against a release build.

Runs the build under an isolated LOCALAPPDATA, so `clips.db`, `settings.db` and
`creds\` land in a temp tree and the real clipboard history is never touched.
That is not tidiness: this script copies things to the clipboard, and a run
against the real database would file the operator's own clipboard into it.

Same two rules as the other drivers: it refuses to type unless the foreground
window is takyon, and it launches the binary itself, because a GUI process
started from a tool call is reaped when that call returns.

**Every consequential key is guarded, not just typing.** `Send-Text` always
checked the foreground; the individual keys did not, and that gap was real -
a browser took the foreground mid-run and a paste-back landed a real URL in the
wrong window. Enter triggers paste-back, Ctrl+Enter and Ctrl+A/Ctrl+C write the
clipboard, and Ctrl+Backspace destroys a clip here and a word of someone's text
anywhere else. All of those go through `Send-KeyTo` / `Send-ChordTo` (by window)
or `Send-ChordToPid` (by process, for the Notepad this script started, never one
the operator had open). A refused key is a SKIP, never a stray keystroke.

Left deliberately unguarded: the global hotkey, which has to fire whatever holds
the foreground, and `Escape`, which does nothing anywhere.

**Read the clipboard, not the pixels** (the v0.4 lesson). Window heights say a
list has rows; only the clipboard says the right bytes came back. The two
strongest steps are V2b - find a stored clip through `!v`, press Ctrl+Enter, get
the original string - and P1, which pastes into Notepad and reads it back out.

Needs `sqlite3` on PATH for the row counts, the blocklist and the retention
steps. Without it those report SKIP rather than being guessed at.
#>
[CmdletBinding()]
param(
    [string]$Exe = "apps\desktop\src-tauri\target\release\takyon.exe",
    [string]$OutDir = "$env:TEMP\takyon-verify-v0.5",
    # Free chord. Alt+Space is contested by Raycast and PowerToys Run on the
    # machines this is developed on, and a failed registration adds the amber
    # banner, which changes every window size printed below.
    [string]$Hotkey = "Ctrl+Alt+Shift+F9"
)

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

[void][Takyon.Drive5]::SetProcessDPIAware()

$UP = 0x0002
$VK = @{ Back = 0x08; Enter = 0x0D; Esc = 0x1B; Ctrl = 0x11; Alt = 0x12; Shift = 0x10
         F9 = 0x78; K = 0x4B; A = 0x41; C = 0x43 }

# The strings this run files into history. A GUID suffix so a re-run cannot match
# the previous run's rows, and a shape no installed application shares - V3 types
# one of these Bangless and must get nothing back.
$run = [guid]::NewGuid().ToString('N').Substring(0, 8)
$ALPHA = "takyon-clip-alpha-$run"
$BETA = "takyon-clip-beta-$run"
$GAMMA = "takyon-clip-gamma-$run"
$DELTA = "takyon-clip-delta-$run"
$DOOMED = "takyon-clip-doomed-$run"
$SENTINEL = "takyon-verify-sentinel-$run"

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
    [Takyon.Drive5]::keybd_event($vk, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 20
    [Takyon.Drive5]::keybd_event($vk, 0, $UP, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds $rest
}

function Send-Chord([byte[]]$Mods, [byte]$Key) {
    foreach ($m in $Mods) { [Takyon.Drive5]::keybd_event($m, 0, 0, [UIntPtr]::Zero) }
    [Takyon.Drive5]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 40
    [Takyon.Drive5]::keybd_event($Key, 0, $UP, [UIntPtr]::Zero)
    $rev = $Mods.Clone(); [array]::Reverse($rev)
    foreach ($m in $rev) { [Takyon.Drive5]::keybd_event($m, 0, $UP, [UIntPtr]::Zero) }
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
        $h = [Takyon.Drive5]::GetForegroundWindow()
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
    $h = [Takyon.Drive5]::GetForegroundWindow()
    $procId = 0
    [void][Takyon.Drive5]::GetWindowThreadProcessId($h, [ref]$procId)
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
    $r = New-Object Takyon.Drive5+RECT
    [void][Takyon.Drive5]::GetWindowRect($f.Handle, [ref]$r)
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

# Every byte of the clipboard database, WAL and shm included.
#
# A shared stream, not ReadAllBytes: the application under test is usually still
# running and holds these open, and the WAL's -shm is mapped. Reading a live file
# is the point - this is what a stolen copy would look like.
function Get-DbBytes {
    $all = New-Object System.Collections.Generic.List[byte]
    foreach ($f in Get-ChildItem (Join-Path $DataDir "clips.db*") -ErrorAction SilentlyContinue) {
        try {
            $fs = [IO.File]::Open($f.FullName, [IO.FileMode]::Open, [IO.FileAccess]::Read,
                [IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete)
        } catch { continue }
        try {
            $bytes = New-Object byte[] $fs.Length
            [void]$fs.Read($bytes, 0, $bytes.Length)
            $all.AddRange($bytes)
        } finally { $fs.Dispose() }
    }
    return $all.ToArray()
}

# Both encodings, always: CF_UNICODETEXT is UTF-16, so a UTF-8-only search passes
# against a file that is leaking.
function Test-Leak([string]$Needle) {
    $bytes = Get-DbBytes
    if ($bytes.Length -eq 0) { return $false }
    $text = [Text.Encoding]::Unicode.GetString($bytes) + [Text.Encoding]::UTF8.GetString($bytes)
    return ($text -match [regex]::Escape($Needle))
}

# Is this exact byte run still in the file?
#
# For the retention steps the honest question is about the *ciphertext*. The
# plaintext was never in the file, so grepping for it after a sweep proves
# nothing at all.
function Test-Bytes([byte[]]$Needle) {
    $hay = Get-DbBytes
    if ($Needle.Length -eq 0 -or $hay.Length -lt $Needle.Length) { return $false }
    $limit = $hay.Length - $Needle.Length
    for ($i = 0; $i -le $limit; $i++) {
        if ($hay[$i] -ne $Needle[0]) { continue }
        $hit = $true
        for ($j = 1; $j -lt $Needle.Length; $j++) {
            if ($hay[$i + $j] -ne $Needle[$j]) { $hit = $false; break }
        }
        if ($hit) { return $true }
    }
    return $false
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

# Whatever the operator had copied, given back at the end. This script sets the
# clipboard a dozen times; leaving it holding a test sentinel is rude.
$restore = Get-Clipboard -Raw -ErrorAction SilentlyContinue

if (-not (Test-Path $Exe)) { throw "no build at $Exe - run 'bun run build' first" }
$underTest = (Resolve-Path $Exe).Path
Get-Process takyon -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -ne $underTest } |
    ForEach-Object { Write-Output "stopping imposter pid $($_.Id) $($_.Path)"; Stop-Process -Id $_.Id -Force }

# The isolation. `identity::data_dir()` is LOCALAPPDATA + v3sper\takyon, so
# redirecting the variable for the child moves the whole data tree - history, key
# and settings alike - into the temp directory this run owns.
$Sandbox = Join-Path $OutDir "appdata"
$DataDir = Join-Path $Sandbox "v3sper\takyon"
Remove-Item $Sandbox -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $DataDir | Out-Null

$env:TAKYON_HOTKEY = $Hotkey
$env:LOCALAPPDATA = $Sandbox
$app = Start-App
Write-Output "under test: pid $($app.Id), data in $DataDir"
if (-not $script:sqlite) { Write-Warning "sqlite3 not on PATH - counts, blocklist and retention are skipped" }

try {
    Write-Output "--- K, S: key material, capture and storage"
    Copy-AndSettle $ALPHA
    Copy-AndSettle $BETA
    Report "S0" (Test-Path (Join-Path $DataDir "clips.db")) "clips.db exists"

    $key = Join-Path $DataDir "creds\clip.key.dpapi"
    $keyLen = if (Test-Path $key) { (Get-Item $key).Length } else { 0 }
    Report "K1" ($keyLen -gt 32) "key blob is $keyLen bytes, and a key is 32"
    Report "S1" (-not (Test-Leak $ALPHA)) "no plaintext in clips.db, its WAL or its shm"

    if ($script:sqlite) {
        $n = Get-RowCount
        Report "S1b" ($n -eq 2) "two copies made two rows, count is $n"

        # The accepted leak, asserted so it stays deliberate (ADR-0008); and W2 in
        # the same query, because the row names the process that owned the
        # clipboard rather than whatever happened to be in front.
        $meta = Invoke-Db "clips.db" "SELECT source_exe || '|' || len FROM clips ORDER BY id DESC LIMIT 1;"
        Report "S2" ($meta -match '\|\d+$') "metadata is plaintext: '$meta'"
        # Not "which" exe, deliberately. `Set-Clipboard` leaves no clipboard owner,
        # so attribution falls back to the foreground window - which is whatever
        # the operator happened to be looking at. Asserting a specific name here
        # is what made the original NULL bug look like a naming mismatch.
        Report "W2" ($meta -match '(?i)\.exe\|\d+$') "a source was recorded at all"

        # S3: a repeat of the newest clip moves that row rather than adding one.
        Copy-AndSettle $GAMMA
        $afterFirst = Get-RowCount
        Copy-AndSettle $GAMMA
        $n = Get-RowCount
        Report "S3" ($n -eq $afterFirst) "repeat of the newest clip kept the count at $n"

        # S4: but a repeat of an *older* one is genuinely a new event.
        Copy-AndSettle $DELTA
        Copy-AndSettle $GAMMA
        $n = Get-RowCount
        Report "S4" ($n -eq ($afterFirst + 2)) "A, B, A made $n rows from $afterFirst"

        # S5: past the cap the copy is skipped, never truncated.
        $before = Get-RowCount
        Copy-AndSettle ("x" * (5 * 1024 * 1024))
        $n = Get-RowCount
        Report "S5" ($n -eq $before) "a 5 MB copy left the count at $n"

        # S6: text only at v0.5, so an image is ignored rather than half-stored.
        $before = Get-RowCount
        $bmp = New-Object System.Drawing.Bitmap 32, 32
        [System.Windows.Forms.Clipboard]::SetImage($bmp)
        Start-Sleep -Milliseconds 1000
        $bmp.Dispose()
        $n = Get-RowCount
        Report "S6" ($n -eq $before) "an image copy left the count at $n"
    } else {
        foreach ($id in "S1b", "S2", "W2", "S3", "S4", "S5", "S6") { Skip $id "needs sqlite3" }
    }

    Write-Output "--- C: the command, and the surface it opens"
    Show-Palette
    # The yardstick for every physical-pixel comparison below, taken before
    # anything is typed: an empty Palette is EMPTY_HEIGHT by definition.
    $script:hEmpty = Save-Shot "start-empty"
    Clear-Query
    if (Send-Text "clipboard") {
        [void](Save-Shot "C1-command-row")
        [void](Send-ChordTo "^takyon$" @($VK.Ctrl) $VK.K)
        [void](Save-Shot "C1-menu")
        $menu = Get-MenuText
        Send-Key $VK.Esc 400
        Report "C1" ($menu -match "Open Command") `
            "'clipboard' put the command on top; its menu reads '$menu'"
        Report "C3" ($menu -notmatch "Run as administrator") `
            "and it is not offered application actions"
    } else {
        Report "C1" $false "could not type"
    }

    Clear-Query
    if (Send-Text "his") {
        [void](Save-Shot "C2-his")
        # Height proves rows exist, never which row is first. The action menu is
        # built from the *selected* Entry, and only a Command offers this action,
        # so reading it back is what proves the command took the top row.
        [void](Send-ChordTo "^takyon$" @($VK.Ctrl) $VK.K)
        [void](Save-Shot "C2-menu")
        $menu = Get-MenuText
        Send-Key $VK.Esc 400
        Report "C2" ($menu -match "Open Command") `
            "'his' put the command on top; its menu reads '$menu'"
    } else {
        Report "C2" $false "could not type"
    }

    # Enter opens the surface. The window growing to VIEW_HEIGHT is the assertion
    # a mocked visual layer structurally cannot make (TBC-0007): there is no
    # native window in the browser to be the wrong size.
    Clear-Query
    if (Send-Text "clipboard") {
        # Enter opens the surface here, and submits a form in anything else.
        [void](Send-KeyTo "^takyon$" $VK.Enter 1500)
        $hView = Save-Shot "C4-surface"
        $front = (Get-Front).Name
        Report "C4a" ($front -eq "takyon") "the Palette stayed up, foreground '$front'"
        # Physical pixels, and the rect includes the shadow border - so the empty
        # Palette is the yardstick rather than a logical constant. EMPTY_HEIGHT is
        # 68 and VIEW_HEIGHT is 560, so the surface must be ~8x the empty window.
        $ratio = if ($script:hEmpty -gt 0) { $hView / $script:hEmpty } else { 0 }
        Report "C4b" ($ratio -gt 6.5 -and $ratio -lt 9) `
            "the window grew to ${hView}px, ${ratio}x the empty Palette (VIEW_HEIGHT/EMPTY_HEIGHT is 8.2)"

        # C7: the filter narrows the list. Height is the readable signal again.
        [void](Send-Text "alpha-$run")
        $hFiltered = Save-Shot "C7-filtered"
        Report "C7" ($hFiltered -eq $hView) "the surface keeps its height while filtering"

        # C8: Escape goes back rather than dismissing.
        Send-Key $VK.Esc 900
        $hBack = Save-Shot "C8-back"
        $front = (Get-Front).Name
        Report "C8a" ($front -eq "takyon") "Escape left the Palette up, foreground '$front'"
        Report "C8b" ($hBack -lt 200) "and shrank it back to ${hBack}px"
    }

    Write-Output "--- V: the !v Mode"
    Show-Palette
    Clear-Query
    $hAll = if (Send-Text "!v") { Save-Shot "V1-history" } else { 0 }
    Report "V1" ($hAll -gt 120) "the !v window grew to ${hAll}px, so it has rows"

    Clear-Query
    $hOne = if (Send-Text "!v alpha-$run") { Save-Shot "V2-filtered" } else { 0 }
    Report "V2a" ($hOne -gt 100 -and $hOne -lt $hAll) "filtered to ${hOne}px from ${hAll}px"

    # The assertion no screenshot could fake: Ctrl+Enter on the filtered row hands
    # back the stored clip, which means it was found and decrypted.
    Set-Clipboard -Value $SENTINEL
    Start-Sleep -Milliseconds 700
    Show-Palette
    Clear-Query
    if (Send-Text "!v alpha-$run") {
        # Ctrl+Enter writes the clipboard, so it never goes to another window.
        if (Send-ChordTo "^takyon$" @($VK.Ctrl) $VK.Enter) {
            Start-Sleep -Milliseconds 1100
            $got = (Get-Clipboard -Raw).Trim()
            Report "V2b" ($got -eq $ALPHA) "Ctrl+Enter returned '$got'"
        } else {
            Skip "V2b" "the Palette lost the foreground before Ctrl+Enter"
        }
    }

    # V4/V5: the parser at its two edges. An unknown Bang and a Bang that is not
    # at position 0 both fall through to Bangless, where these tokens match
    # nothing at all.
    Show-Palette
    Clear-Query
    $hUnknown = if (Send-Text "!s alpha-$run") { Save-Shot "V4-unknown-bang" } else { 999 }
    Report "V4" ($hUnknown -lt 120) "an unknown Bang fell through, window ${hUnknown}px"

    Clear-Query
    $hSpaced = if (Send-Text " !v") { Save-Shot "V5-leading-space" } else { 999 }
    Report "V5" ($hSpaced -lt 120) "a leading space is not a Bang, window ${hSpaced}px"

    # V6: what the menu rows *say* is asserted in Playwright. What this adds is
    # that the native window made room for them - the one thing a mocked visual
    # layer structurally cannot see (TBC-0007).
    Clear-Query
    if (Send-Text "!v alpha-$run") {
        $hRow = Save-Shot "V6-before-menu"
        [void](Send-ChordTo "^takyon$" @($VK.Ctrl) $VK.K)
        $hMenu = Save-Shot "V6-menu"
        # A weak assertion, and labelled as one: the menu overlays the list rather
        # than sitting below it, so a Palette already tall enough does not grow.
        # What the rows *say* is asserted in Playwright.
        Report "V6" ($hMenu -ge $hRow) "Ctrl+K left the window at ${hMenu}px, >= ${hRow}px"
        Send-Key $VK.Esc 400
    }

    Write-Output "--- V3: a Bangless query can never reach a clip"
    Set-Clipboard -Value $SENTINEL
    Start-Sleep -Milliseconds 700
    Show-Palette
    Clear-Query
    if (Send-Text "alpha-$run") {
        $hBangless = Save-Shot "V3-bangless"
        # Must do nothing, but it is still a keypress: only sent to the Palette.
        [void](Send-KeyTo "^takyon$" $VK.Enter 900)
        $after = (Get-Clipboard -Raw).Trim()
        Report "V3a" ($hBangless -lt 120) "Bangless window stayed ${hBangless}px, so no rows"
        Report "V3b" ($after -eq $SENTINEL) "clipboard after Enter is unchanged"
    }

    Write-Output "--- P1: paste-back into a real window"
    # Notepad is the target because it can be read back: paste, then select-all
    # and copy, and the clipboard holds whatever actually landed in the window.
    # Only ever touch the Notepad this script started. Modern Notepad is a
    # packaged app whose launcher exits immediately, so the returned process is
    # not necessarily the window - the difference between "ours" and "theirs" is
    # taken by diffing the process list, and an operator's unsaved document is
    # never in the set this cleans up.
    $padsBefore = @(Get-Process notepad -ErrorAction SilentlyContinue | ForEach-Object Id)
    [void](Start-Process notepad)
    Start-Sleep -Seconds 3
    $script:ourPads = @(Get-Process notepad -ErrorAction SilentlyContinue |
        Where-Object { $padsBefore -notcontains $_.Id } | ForEach-Object Id)
    # Empty it first. Windows 11 Notepad restores unsaved tabs, so a fresh window
    # is not a blank one - the previous run's text was still there, and the paste
    # appended to it. That read as a paste-back bug and was not one.
    if ($script:ourPads.Count -gt 0) {
        [void](Send-ChordToPid $script:ourPads @($VK.Ctrl) $VK.A)
        [void](Send-KeyToPid $script:ourPads $VK.Back 300)
    }
    Show-Palette
    Clear-Query
    if ($script:ourPads.Count -eq 0) {
        Skip "P1a" "no Notepad of ours to paste into"
        Skip "P1b" "no Notepad of ours to paste into"
        $pasted = $false
    }
    elseif (Send-Text "!v delta-$run") {
        # Enter is the paste. Sent only while the Palette holds the foreground,
        # because Takyon pastes into whatever has focus - and that is the
        # operator's window if something stole it a moment ago.
        $pasted = Send-KeyTo "^takyon$" $VK.Enter 1800
        $front = (Get-Front).Name
        # Reading back writes the clipboard too, so it is guarded by pid:
        # Ctrl+A/Ctrl+C into someone else's editor would replace their clipboard
        # with their own document.
        $read = $pasted -and (Send-ChordToPid $script:ourPads @($VK.Ctrl) $VK.A)
        $read = $read -and (Send-ChordToPid $script:ourPads @($VK.Ctrl) $VK.C)
        Start-Sleep -Milliseconds 800
        $landed = if ($read) { (Get-Clipboard -Raw).Trim() } else { "<not read>" }
        # Weaker than it looks, deliberately: what matters is that the Palette
        # got out of the way, not which window won the race afterwards. P1b is
        # the real assertion - the text arrived in Notepad.
        if (-not $pasted) {
            Skip "P1a" "the Palette lost the foreground before Enter; nothing was sent"
            Skip "P1b" "the Palette lost the foreground before Enter; nothing was sent"
        } else {
            Report "P1a" ($front -notmatch "(?i)takyon") "the Palette dismissed; foreground is '$front'"
            Report "P1b" ($landed -eq $DELTA) "Notepad received '$landed'"
        }
    }
    foreach ($id in $script:ourPads) { Stop-Process -Id $id -Force -ErrorAction SilentlyContinue }
    Start-Sleep -Milliseconds 800

    Write-Output "--- P3, P4: copy without pasting, and delete"
    Set-Clipboard -Value $SENTINEL
    Start-Sleep -Milliseconds 700
    Show-Palette
    Clear-Query
    if (Send-Text "!v beta-$run") {
        if (Send-ChordTo "^takyon$" @($VK.Ctrl) $VK.Enter) {
            Start-Sleep -Milliseconds 1100
            $got = (Get-Clipboard -Raw).Trim()
            Report "P3" ($got -eq $BETA) "Ctrl+Enter loaded the clipboard without pasting"
        } else {
            Skip "P3" "the Palette lost the foreground before Ctrl+Enter"
        }
    }

    $before = if ($script:sqlite) { Get-RowCount } else { 0 }
    Show-Palette
    Clear-Query
    if (Send-Text "!v beta-$run") {
        $hBefore = Save-Shot "P4-before"
        # Ctrl+Backspace destroys a clip here, and deletes a word of someone's
        # text anywhere else. Guarded for that reason.
        [void](Send-ChordTo "^takyon$" @($VK.Ctrl) $VK.Back)
        Start-Sleep -Milliseconds 1000
        $hAfter = Save-Shot "P4-after"
        $front = (Get-Front).Name
        Report "P4a" ($hAfter -lt $hBefore) "window shrank ${hBefore}px to ${hAfter}px"
        Report "P4b" ($front -eq "takyon") "foreground is still '$front'"
        if ($script:sqlite) {
            $n = Get-RowCount
            Report "P4c" ($n -eq ($before - 1)) "the row is gone, $n left from $before"
        }
    }
    Send-Key $VK.Esc 400

    if ($script:sqlite) {
        Write-Output "--- BT: the Bang is a toggle"
        Stop-App $app
        [void](Invoke-Db "settings.db" "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL); INSERT OR REPLACE INTO settings (key, value) VALUES ('clips.bang', '0');")
        $app = Start-App
        Show-Palette
        Clear-Query
        $hOff = if (Send-Text "!v") { Save-Shot "BT2-bang-off" } else { 999 }
        Report "BT2" ($hOff -lt 120) "with the Bang off, !v matched nothing (${hOff}px)"

        Clear-Query
        if (Send-Text "clipboard") {
            [void](Save-Shot "BT3-command-still")
            [void](Send-ChordTo "^takyon$" @($VK.Ctrl) $VK.K)
            $menu = Get-MenuText
            Send-Key $VK.Esc 400
            Report "BT3" ($menu -match "Open Command") `
                "the command still takes the top row with the Bang off"
        } else {
            Report "BT3" $false "could not type"
        }

        Stop-App $app
        [void](Invoke-Db "settings.db" "INSERT OR REPLACE INTO settings (key, value) VALUES ('clips.bang', '1');")
        $app = Start-App
        Show-Palette
        Clear-Query
        $hOn = if (Send-Text "!v") { Save-Shot "BT4-bang-on" } else { 0 }
        Report "BT4" ($hOn -gt 120) "turning it back on restored !v (${hOn}px)"
        Send-Key $VK.Esc 300

        Write-Output "--- K2: history survives a restart"
        Stop-App $app
        $app = Start-App
        Set-Clipboard -Value $SENTINEL
        Start-Sleep -Milliseconds 700
        Show-Palette
        Clear-Query
        if (Send-Text "!v alpha-$run") {
            if (Send-ChordTo "^takyon$" @($VK.Ctrl) $VK.Enter) {
                Start-Sleep -Milliseconds 1100
                $got = (Get-Clipboard -Raw).Trim()
                Report "K2" ($got -eq $ALPHA) "a clip from before the restart still decrypts"
            } else {
                Skip "K2" "the Palette lost the foreground before Ctrl+Enter"
            }
        }

        Write-Output "--- X4, X5: the blocklist"
        Stop-App $app
        $owner = Invoke-Db "clips.db" "SELECT source_exe FROM clips ORDER BY id DESC LIMIT 1;"
        # A blocklist keyed on nothing blocks nothing, so an unattributed row is a
        # skip rather than a pass. This is exactly how the NULL `source_exe` bug
        # hid: X4 could never have failed, because it could never have run.
        $ownerExe = if ([string]::IsNullOrWhiteSpace($owner)) { "" } else { (Split-Path $owner -Leaf).ToLower() }
        if (-not $ownerExe) {
            Skip "X4" "the last row has no source_exe to block"
            Skip "X5" "the last row has no source_exe to block"
            $app = Start-App
        } else {
        [void](Invoke-Db "settings.db" "CREATE TABLE IF NOT EXISTS blocklist (exe TEXT PRIMARY KEY); INSERT OR IGNORE INTO blocklist (exe) VALUES ('$ownerExe');")
        $app = Start-App
        $before = Get-RowCount
        Copy-AndSettle "takyon-clip-blocked-$run"
        $n = Get-RowCount
        Report "X4" ($n -eq $before) "a copy from blocklisted '$ownerExe' made no row"

        Stop-App $app
        [void](Invoke-Db "settings.db" "DELETE FROM blocklist;")
        $app = Start-App
        $before = Get-RowCount
        Copy-AndSettle "takyon-clip-unblocked-$run"
        $n = Get-RowCount
        Report "X5" ($n -eq ($before + 1)) "un-blocking let capture resume, $before to $n"
        }

        Write-Output "--- R: retention, which destroys data"
        Copy-AndSettle $DOOMED
        Stop-App $app

        # Age the newest row past a day and remember its ciphertext. The plaintext
        # was never in the file, so the only honest question after a sweep is
        # whether these exact bytes are gone.
        $doomedId = Invoke-Db "clips.db" "SELECT id FROM clips ORDER BY id DESC LIMIT 1;"
        $hex = Invoke-Db "clips.db" "SELECT hex(ciphertext) FROM clips WHERE id = $doomedId;"
        $doomed = [byte[]]::new($hex.Length / 2)
        for ($i = 0; $i -lt $doomed.Length; $i++) {
            $doomed[$i] = [Convert]::ToByte($hex.Substring($i * 2, 2), 16)
        }
        $cut = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds() - (2 * 86400)
        [void](Invoke-Db "clips.db" "UPDATE clips SET created_at = $cut WHERE id = $doomedId;")
        $aged = [int](Invoke-Db "clips.db" "SELECT COUNT(*) FROM clips WHERE created_at < $($cut + 1);")
        $survivors = [int](Invoke-Db "clips.db" "SELECT COUNT(*) FROM clips WHERE created_at >= $($cut + 1);")
        Report "R1" ($aged -eq 1) "exactly $aged row is older than a one-day window"
        Report "R3a" (Test-Bytes $doomed) "its ciphertext is in the file before the sweep"

        # R5 first, because it is the dangerous direction: the one-month default
        # must not sweep over a chosen `forever`.
        [void](Invoke-Db "settings.db" "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL); INSERT OR REPLACE INTO settings (key, value) VALUES ('clips.retention', 'forever');")
        $app = Start-App
        Start-Sleep -Seconds 2
        $n = Get-RowCount
        Report "R5" ($n -eq ($survivors + 1)) "forever swept nothing, $n rows"
        Stop-App $app

        [void](Invoke-Db "settings.db" "INSERT OR REPLACE INTO settings (key, value) VALUES ('clips.retention', '1-day');")
        $app = Start-App
        Start-Sleep -Seconds 2
        $n = Get-RowCount
        Report "R2" ($n -eq $survivors) "1-day swept the aged row, $n left"
        Report "R4" ($n -eq $survivors) "and it ran at startup, before the Palette was opened"
        Report "R3b" (-not (Test-Bytes $doomed)) "the destroyed row's ciphertext is gone from the file"
    } else {
        foreach ($id in "K2", "X4", "X5", "R1", "R2", "R3a", "R3b", "R4", "R5") { Skip $id "needs sqlite3" }
    }
}
finally {
    Start-Sleep -Milliseconds 400
    Get-Content (Join-Path $OutDir "stderr.txt") -ErrorAction SilentlyContinue
    Get-Process takyon -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -eq $underTest } |
        ForEach-Object { Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue }
    foreach ($id in $script:ourPads) { Stop-Process -Id $id -Force -ErrorAction SilentlyContinue }
    if ($restore) { Set-Clipboard -Value $restore } else { Set-Clipboard -Value " " }
}

Write-Output ""
Write-Output "$($script:pass) passed, $($script:fail) failed, $($script:skip) skipped"
Write-Output "shots in $OutDir; sandbox data in $DataDir (delete it when done)"
Write-Output "Needs a person: X1-X3 (a password manager, a UAC credential box),"
Write-Output "K3 (a second Windows account), P2 (an elevated window, which also needs"
Write-Output "the UIAccess signature that is still a v1.0 blocker), W1/W3 (Alt+Tab, a soak)."
