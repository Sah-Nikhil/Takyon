<#
.SYNOPSIS
Drive the automatable half of docs/verify/v0.11.1.md — section C.

The exit criterion of v0.11.1 is a negative: after the Palette hides, no process
Takyon started is still running. Nothing in the Playwright suite can see that,
because it never reaches Tauri, and nothing in the Rust suite can see it through
a real dismissal, because dismissal is a window event.

Same two rules as scripts/verify-drive-v0.8.ps1. It refuses to type unless the
foreground window belongs to takyon, and it launches the binary itself, because a
GUI process started from a tool call is reaped when that call returns.

Each Turn costs tokens: one per dismiss route, plus one for `!s` with -Search.
#>
[CmdletBinding()]
param(
    [string]$Exe = "apps\desktop\src-tauri\target\release\takyon.exe",
    [string]$OutDir = "$env:TEMP\takyon-verify-v0.11.1",
    # Free chord. Alt+Space is contested by Raycast and PowerToys Run on most
    # machines this is developed on.
    [string]$Hotkey = "Ctrl+Alt+Shift+F9",
    # Long enough that the Agent is certainly mid-answer when the Palette hides:
    # that is the state the whole phase is about.
    [string]$Question = "Name every planet in the solar system, one line each, with its diameter.",
    [int]$AnswerSeconds = 12,
    # `!s` fetches pages before it asks. Off by default: it leaves the machine.
    [switch]$Search
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type -Namespace Takyon -Name Drive111 -MemberDefinition @'
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(IntPtr h, out int pid);
  [DllImport("user32.dll", SetLastError=true)] public static extern void keybd_event(byte vk, byte sc, uint f, System.UIntPtr x);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(System.Drawing.Point p);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, System.UIntPtr extra);
'@

[void][Takyon.Drive111]::SetProcessDPIAware()

$UP = 0x0002
$VK = @{ Esc = 0x1B; Ctrl = 0x11; Alt = 0x12; Shift = 0x10; F9 = 0x78; Enter = 0x0D }

# WebView2's own processes are descendants too, and are not what this counts.
$CHROME = @("msedgewebview2", "conhost")

function Send-Chord([byte[]]$Mods, [byte]$Key) {
    foreach ($m in $Mods) { [Takyon.Drive111]::keybd_event($m, 0, 0, [UIntPtr]::Zero) }
    [Takyon.Drive111]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 40
    [Takyon.Drive111]::keybd_event($Key, 0, $UP, [UIntPtr]::Zero)
    $rev = $Mods.Clone(); [array]::Reverse($rev)
    foreach ($m in $rev) { [Takyon.Drive111]::keybd_event($m, 0, $UP, [UIntPtr]::Zero) }
    Start-Sleep -Milliseconds 600
}

function Send-Key([byte]$vk, [int]$rest = 300) {
    [Takyon.Drive111]::keybd_event($vk, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 20
    [Takyon.Drive111]::keybd_event($vk, 0, $UP, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds $rest
}

function Get-FrontName {
    $h = [Takyon.Drive111]::GetForegroundWindow()
    $procId = 0
    [void][Takyon.Drive111]::GetWindowThreadProcessId($h, [ref]$procId)
    $p = Get-Process -Id $procId -ErrorAction SilentlyContinue
    if ($p) { $p.ProcessName } else { "?" }
}

# Typed rather than pasted: the clipboard is a shared resource and v0.5's history
# would record whatever this script put there.
function Send-Text([string]$Text) {
    $front = Get-FrontName
    if ($front -ne "takyon") { throw "refusing to type into '$front'" }
    [System.Windows.Forms.SendKeys]::SendWait(($Text -replace '[+^%~(){}\[\]]', '{$0}'))
    Start-Sleep -Milliseconds 500
}

# One press of the hotkey. A toggle: it is also how a dismissal is driven.
function Press-Hotkey {
    Send-Chord @($VK.Ctrl, $VK.Alt, $VK.Shift) $VK.F9
    Start-Sleep -Milliseconds 700
}

# Press until the Palette actually has the foreground, or say so and stop.
function Summon {
    for ($i = 0; $i -lt 3; $i++) {
        if ((Get-FrontName) -eq "takyon") { return }
        Press-Hotkey
    }
    throw "the Palette never took the foreground (front is '$(Get-FrontName)')"
}

# Every process under $rootPid started since $since, WebView2's own excluded.
#
# Two guards against Windows recycling pids. A child counts only if it was
# created after its parent, and only if it was created after the moment the Turn
# started - a stale parent pid is otherwise enough to drag this session's own
# unrelated `git` and `node` processes into the tree.
function Get-AgentTree([int]$rootPid, [datetime]$since) {
    $all = Get-CimInstance Win32_Process |
        Select-Object ProcessId, ParentProcessId, Name, CreationDate
    $born = @{}
    foreach ($p in $all) { $born[[int]$p.ProcessId] = $p.CreationDate }
    $children = @{}
    foreach ($p in $all) {
        $parent = [int]$p.ParentProcessId
        if (-not $children.ContainsKey($parent)) { $children[$parent] = @() }
        $children[$parent] += , $p
    }
    $found = @()
    $queue = [System.Collections.Queue]::new()
    $queue.Enqueue($rootPid)
    $seen = @{}
    while ($queue.Count) {
        $current = [int]$queue.Dequeue()
        if ($seen.ContainsKey($current)) { continue }
        $seen[$current] = $true
        foreach ($child in $children[$current]) {
            if ($child.CreationDate -lt $since) { continue }
            if ($born[$current] -and $child.CreationDate -lt $born[$current]) { continue }
            $name = [System.IO.Path]::GetFileNameWithoutExtension($child.Name)
            $queue.Enqueue([int]$child.ProcessId)
            if ($CHROME -notcontains $name) { $found += , "$name($($child.ProcessId))" }
        }
    }
    , $found
}

# Poll until an Agent process exists, up to $AnswerSeconds. Returns the tree.
function Wait-ForAgent([int]$rootPid, [datetime]$since) {
    $deadline = (Get-Date).AddSeconds($AnswerSeconds)
    while ((Get-Date) -lt $deadline) {
        $tree = Get-AgentTree $rootPid $since
        if ($tree.Count) { return , $tree }
        Start-Sleep -Milliseconds 200
    }
    , @()
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

if (-not (Test-Path $Exe)) { throw "no build at $Exe - run 'bun run build' first" }
$underTest = (Resolve-Path $Exe).Path

# Any running Takyon swallows every summon through single-instance and the build
# under test never runs, silently.
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

trap {
    Write-Warning "aborted: $_"
    Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
    break
}

Start-Sleep -Seconds 6

$failures = 0
function Test-Route([string]$Name, [string]$Bang, [scriptblock]$Dismiss) {
    Summon
    Send-Text "$Bang $Question"
    $since = (Get-Date).AddSeconds(-1)
    Send-Key $VK.Enter 600
    # Dismissed the moment the Agent is on the process table. Sampling on a timer
    # instead races the answer: a fast Turn is over in eight seconds and there is
    # then nothing to kill, which reads as a pass.
    $during = Wait-ForAgent $app.Id $since
    Write-Output "$Name : mid-answer tree = $($during -join ', ')"
    if (-not $during.Count) {
        Write-Warning "$Name FAILED: no Agent process appeared within $AnswerSeconds s"
        $script:failures++
    }

    $moved = & $Dismiss
    if ($moved -is [bool] -and -not $moved) {
        Write-Warning "$Name not judged: the dismissal never happened"
        $script:failures++
        return
    }
    Start-Sleep -Seconds 3
    $after = Get-AgentTree $app.Id $since
    if ($after.Count) {
        Write-Warning "$Name FAILED: survived dismissal - $($after -join ', ')"
        $script:failures++
    }
    else {
        Write-Output "$Name ok: nothing survived the dismissal"
    }
    if (-not (Get-Process -Id $app.Id -ErrorAction SilentlyContinue)) {
        Write-Warning "$Name FAILED: Takyon itself died"
        $script:failures++
    }
}

Test-Route "C1 hotkey" "!c" { Press-Hotkey }
Test-Route "C2 escape" "!c" { Send-Key $VK.Esc 400; Send-Key $VK.Esc 400 }
# Focus loss, the third dismiss route. A window that has just launched is given
# the foreground by Windows itself, which is the one way a script can take it
# from the Palette: SetForegroundWindow and AppActivate are both refused here.
function Move-FocusAway {
    $window = Start-Process notepad -PassThru
    for ($i = 0; $i -lt 20; $i++) {
        Start-Sleep -Milliseconds 300
        if ((Get-FrontName) -ne "takyon") {
            Stop-Process -Id $window.Id -Force -ErrorAction SilentlyContinue
            return $true
        }
    }
    Stop-Process -Id $window.Id -Force -ErrorAction SilentlyContinue
    Write-Warning "could not move the foreground off the Palette - C3 proves nothing"
    return $false
}

Test-Route "C3 focus loss" "!c" { Move-FocusAway }
if ($Search) {
    Test-Route "C4 web search" "!s" { Press-Hotkey }
}

# The crash case: KILL_ON_JOB_CLOSE, with no code of ours running.
Summon
Send-Text "!c $Question"
$since = (Get-Date).AddSeconds(-1)
Send-Key $VK.Enter 600
$before = Wait-ForAgent $app.Id $since
Write-Output "C5 crash : mid-answer tree = $($before -join ', ')"
if (-not $before.Count) {
    Write-Warning "C5 not judged: no Agent process appeared to orphan"
    $failures++
}
Stop-Process -Id $app.Id -Force
Start-Sleep -Seconds 3
$orphans = @($before | Where-Object {
        $running = Get-Process -Id ([int]($_ -replace '.*\((\d+)\)$', '$1')) -ErrorAction SilentlyContinue
        $null -ne $running
    })
if ($orphans.Count) {
    Write-Warning "C5 FAILED: orphaned after a kill - $($orphans -join ', ')"
    $failures++
}
else {
    Write-Output "C5 ok: killing Takyon took every Agent process with it"
}

Get-Content $log -ErrorAction SilentlyContinue
Write-Output ""
Write-Output "$failures failure(s). Logs in $OutDir"
Write-Output "still needs a person: docs/verify/v0.11.1.md section A - the npm"
Write-Output "install itself, on a machine where 'where.exe opencode' names a .cmd."
