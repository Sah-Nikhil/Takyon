<#
.SYNOPSIS
Drive docs/verify/v0.11.2.md against a release build: Agent fallback, the `!s`
fast model, and opencode v2.

Runs under its own LOCALAPPDATA, so it never reads or writes the real
settings.db. It seeds that copy with a Claude Code `!c` model that does not
exist. That is the one Agent failure a script can cause on purpose, and it
takes the same path a usage limit does (ADR-0041): a synthetic assistant line,
then an error result, with no answer text.

Same two rules as scripts/verify-drive-v0.11.1.ps1. It refuses to type unless
the foreground window belongs to takyon, and it launches the binary itself.
It stops any running Takyon first, the installed one included, and relaunches
D:\Takyon\takyon.exe at the end if that exists.

Costs tokens: four short Turns. Screenshots and process command lines go to
-OutDir, to be read by a person.
#>
[CmdletBinding()]
param(
    [string]$Exe,
    [string]$OutDir = "$env:TEMP\takyon-verify-v0.11.2",
    [string]$Hotkey = "Ctrl+Alt+Shift+F9",
    [string]$Installed = "D:\Takyon\takyon.exe",
    [int]$AnswerSeconds = 30
)

$ErrorActionPreference = "Stop"
if (-not $Exe) { $Exe = Join-Path (& "$PSScriptRoot\lib\cargo-target-dir.ps1") "release\takyon.exe" }
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type -Namespace Takyon -Name Drive112 -MemberDefinition @'
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(IntPtr h, out int pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll", SetLastError=true)] public static extern void keybd_event(byte vk, byte sc, uint f, System.UIntPtr x);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
'@
[void][Takyon.Drive112]::SetProcessDPIAware()

$UP = 0x0002
$VK = @{ Esc = 0x1B; Ctrl = 0x11; Alt = 0x12; Shift = 0x10; F9 = 0x78; Enter = 0x0D; A = 0x41 }

function Send-Chord([byte[]]$Mods, [byte]$Key) {
    foreach ($m in $Mods) { [Takyon.Drive112]::keybd_event($m, 0, 0, [UIntPtr]::Zero) }
    [Takyon.Drive112]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 40
    [Takyon.Drive112]::keybd_event($Key, 0, $UP, [UIntPtr]::Zero)
    $rev = $Mods.Clone(); [array]::Reverse($rev)
    foreach ($m in $rev) { [Takyon.Drive112]::keybd_event($m, 0, $UP, [UIntPtr]::Zero) }
    Start-Sleep -Milliseconds 400
}

function Send-Key([byte]$vk, [int]$rest = 300) {
    [Takyon.Drive112]::keybd_event($vk, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 20
    [Takyon.Drive112]::keybd_event($vk, 0, $UP, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds $rest
}

function Get-FrontName {
    $h = [Takyon.Drive112]::GetForegroundWindow()
    $procId = 0
    [void][Takyon.Drive112]::GetWindowThreadProcessId($h, [ref]$procId)
    $p = Get-Process -Id $procId -ErrorAction SilentlyContinue
    if ($p) { $p.ProcessName } else { "?" }
}

# Typed rather than pasted: v0.5's clipboard history would record a paste.
function Send-Text([string]$Text) {
    $front = Get-FrontName
    if ($front -ne "takyon") { throw "refusing to type into '$front'" }
    [System.Windows.Forms.SendKeys]::SendWait(($Text -replace '[+^%~(){}\[\]]', '{$0}'))
    Start-Sleep -Milliseconds 500
}

function Summon {
    for ($i = 0; $i -lt 3; $i++) {
        if ((Get-FrontName) -eq "takyon") { return }
        Send-Chord @($VK.Ctrl, $VK.Alt, $VK.Shift) $VK.F9
        Start-Sleep -Milliseconds 700
    }
    throw "the Palette never took the foreground (front is '$(Get-FrontName)')"
}

# Back to an empty Palette: hide it with the hotkey, then summon it again. Every
# show opens empty (ADR-0001). Not Escape: on the list Escape hides the Palette,
# and the foreground then lands on whatever the user had open.
function Reset-Input {
    if ((Get-FrontName) -eq "takyon") {
        Send-Chord @($VK.Ctrl, $VK.Alt, $VK.Shift) $VK.F9
        Start-Sleep -Milliseconds 800
    }
    Summon
    Start-Sleep -Milliseconds 400
}

# The foreground window, captured. The Palette is the only window that matters.
function Save-Shot([string]$Name) {
    $h = [Takyon.Drive112]::GetForegroundWindow()
    $r = New-Object Takyon.Drive112+RECT
    [void][Takyon.Drive112]::GetWindowRect($h, [ref]$r)
    $w = [Math]::Max(1, $r.Right - $r.Left); $hgt = [Math]::Max(1, $r.Bottom - $r.Top)
    $bmp = New-Object System.Drawing.Bitmap $w, $hgt
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($r.Left, $r.Top, 0, 0, $bmp.Size)
    $path = Join-Path $OutDir "$Name.png"
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Output "shot: $path"
}

# Command lines of every Agent process alive now. How `--model` is proven.
function Save-AgentArgs([string]$Name) {
    $found = Get-CimInstance Win32_Process |
        Where-Object { $_.Name -match '^(claude|codex|opencode|node|bun)' -and $_.CommandLine -match '(-p |exec |run )' } |
        ForEach-Object { "$($_.Name): $($_.CommandLine)" }
    $found | Set-Content (Join-Path $OutDir "$Name-args.txt")
    $found | ForEach-Object { Write-Output "  $_" }
}

# Settings, written straight into the isolated copy. Python's sqlite3, because
# Windows ships no SQLite shell and the app must not be the one writing.
function Set-Prefs([hashtable]$Values, [string[]]$Remove = @()) {
    $db = Join-Path $DataDir "settings.db"
    $pairs = ($Values.GetEnumerator() | ForEach-Object { "($(ConvertTo-Json $_.Key),$(ConvertTo-Json $_.Value))" }) -join ","
    $drops = ($Remove | ForEach-Object { ConvertTo-Json $_ }) -join ","
    $py = @"
import sqlite3
c = sqlite3.connect(r'$db')
c.execute('CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)')
for k, v in [$pairs]: c.execute('INSERT OR REPLACE INTO settings VALUES (?, ?)', (k, v))
for k in [$drops]: c.execute('DELETE FROM settings WHERE key = ?', (k,))
c.commit()
"@
    $py | python -
    if ($LASTEXITCODE -ne 0) { throw "could not write $db" }
}

function Start-UnderTest([string]$Tag) {
    $env:TAKYON_HOTKEY = $Hotkey
    $env:LOCALAPPDATA = $Isolated
    $p = Start-Process -FilePath $Exe -PassThru `
        -RedirectStandardError (Join-Path $OutDir "$Tag-stderr.txt") `
        -RedirectStandardOutput (Join-Path $OutDir "$Tag-stdout.txt")
    $env:LOCALAPPDATA = $RealLocal
    Start-Sleep -Seconds 6
    Write-Output "under test ($Tag): pid $($p.Id)"
    $p
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
if (-not (Test-Path $Exe)) { throw "no build at $Exe - run 'bun run build' first" }
$RealLocal = $env:LOCALAPPDATA
$Isolated = Join-Path $OutDir "localappdata"
$DataDir = Join-Path $Isolated "v3sper\takyon"
if (Test-Path $Isolated) { Remove-Item -Recurse -Force $Isolated }
New-Item -ItemType Directory -Force -Path $DataDir | Out-Null
New-Item -ItemType File -Force -Path (Join-Path $DataDir "first-run-complete") | Out-Null

$wasRunning = @(Get-Process takyon -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Installed }).Count -gt 0
Get-Process takyon -ErrorAction SilentlyContinue | ForEach-Object {
    Write-Output "stopping pid $($_.Id) $($_.Path)"
    Stop-Process -Id $_.Id -Force
}
Start-Sleep -Milliseconds 500

$app = $null
try {
    # A: Claude's !c model does not exist, Codex is second.
    Set-Prefs @{
        "agents.order"        = '["claude","codex","opencode"]'
        "agents.model.claude" = "takyon-model-that-does-not-exist"
    }
    $app = Start-UnderTest "a"
    Reset-Input
    Send-Text "!c Reply with the single word: ok"
    Send-Key $VK.Enter 600
    Start-Sleep -Seconds $AnswerSeconds
    Save-Shot "a1-fallback"

    Reset-Input
    Send-Text "!c hi"
    Start-Sleep -Seconds 1
    Save-Shot "a2-row-names-codex"

    # Claude is benched, so !s skips it without a spawn and Codex writes.
    Reset-Input
    Send-Text "!s population of iceland"
    Send-Key $VK.Enter 600
    Start-Sleep -Seconds $AnswerSeconds
    Save-Shot "a3-search-codex"
    Stop-Process -Id $app.Id -Force; $app = $null
    Start-Sleep -Milliseconds 800

    # B: a fresh process has no bench, Claude's !c model is restored, and !s
    # writes with the fast default. Shots on a clock show when it answers.
    Set-Prefs @{} -Remove @("agents.model.claude")
    $app = Start-UnderTest "b"
    Reset-Input
    Send-Text "!s who won the 2022 world cup"
    Send-Key $VK.Enter 300
    $last = 0
    foreach ($t in 2, 4, 6, 9, 13, 20) {
        Start-Sleep -Seconds ($t - $last)
        $last = $t
        if ($t -eq 4) { Save-AgentArgs "b-search" }
        Save-Shot ("b-search-{0:d2}s" -f $t)
    }
    Stop-Process -Id $app.Id -Force; $app = $null
    Start-Sleep -Milliseconds 800

    # C: opencode first. v2.4.1 refused every Turn before v0.11.2.
    Set-Prefs @{ "agents.order" = '["opencode","claude","codex"]' }
    $app = Start-UnderTest "c"
    Reset-Input
    Send-Text "!c Reply with the single word: ok"
    Send-Key $VK.Enter 600
    Start-Sleep -Seconds 2
    Save-AgentArgs "c-opencode"
    Start-Sleep -Seconds ($AnswerSeconds - 2)
    Save-Shot "c1-opencode"
}
finally {
    if ($app) { Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue }
    $env:LOCALAPPDATA = $RealLocal
    if ($wasRunning -and (Test-Path $Installed)) {
        Start-Process -FilePath $Installed -WorkingDirectory (Split-Path $Installed)
        Write-Output "relaunched $Installed"
    }
}
Write-Output "Done. Read the shots in $OutDir against docs/verify/v0.11.2.md."
