<#
.SYNOPSIS
  The post-idle first-show measurement, run detached so it survives the shell
  that started it.

.DESCRIPTION
  `bun run bench --idle 35` does this too, but it dies with its parent shell —
  which is fatal for a measurement whose whole point is waiting 35 minutes. This
  script is launched with Start-Process, samples memory throughout the wait, and
  writes a summary a reader can pick up long after everyone has gone home.

  What it measures, and why it is the number that matters (TBC-0002): every other
  benchmark figure is taken seconds after the previous show, in a tight loop.
  Windows has had no opportunity to reclaim the trimmed working set. A real user
  summons the Palette after half an hour of not touching it, and if that first
  show is dramatically slower than the second, the warm-window model is paying the
  memory *and* losing the latency.

  The memory samples are the other half. A single reading at the end cannot tell
  "trimmed once and stayed flat" apart from "decayed steadily under pressure", and
  those imply different things about what the first summon costs.

.EXAMPLE
  Start-Process powershell -WindowStyle Hidden -ArgumentList @(
    '-NoProfile','-ExecutionPolicy','Bypass','-File','.\scripts\bench-idle.ps1'
  )
#>

[CmdletBinding()]
param(
    [int]$IdleMinutes = 35,
    [int]$SampleIntervalSeconds = 120,
    # Deliberately not defaulted here. `$PSScriptRoot` is not populated while
    # param() is being bound, so a default that references it binds an empty
    # string and Split-Path throws before the script has run a single line —
    # and under `-WindowStyle Hidden` that failure is completely silent.
    [string]$OutDir
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
if (-not $OutDir) { $OutDir = Join-Path $repo 'bench\results' }
$exe = Join-Path $repo 'apps\desktop\src-tauri\target\release\takyon.exe'

if (-not (Test-Path $exe)) { throw "no release binary at $exe -- run 'bun run build' first" }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$stamp = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH-mm-ss')
$benchLog = Join-Path $OutDir "idle-$stamp.jsonl"
$rssLog = Join-Path $OutDir "idle-$stamp.rss.jsonl"
$summary = Join-Path $OutDir "idle-$stamp.summary.json"
$transcript = Join-Path $OutDir "idle-$stamp.txt"

function Log($m) {
    $line = "[{0:HH:mm:ss}] {1}" -f (Get-Date), $m
    Add-Content -Path $transcript -Value $line
}

Add-Type -Namespace B -Name I -MemberDefinition @"
[DllImport("user32.dll", SetLastError=true)] public static extern void keybd_event(byte v, byte s, uint f, System.UIntPtr e);
"@

function Send-AltSpace {
    [B.I]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero)
    [B.I]::keybd_event(0x20, 0, 0, [UIntPtr]::Zero)
    [B.I]::keybd_event(0x20, 0, 2, [UIntPtr]::Zero)
    [B.I]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
}
function Send-Escape {
    [B.I]::keybd_event(0x1B, 0, 0, [UIntPtr]::Zero)
    [B.I]::keybd_event(0x1B, 0, 2, [UIntPtr]::Zero)
}

# Whole-process-tree memory. WebView2's renderer is a child of its browser
# process, not of us, so a one-level walk misses where the memory actually is.
function Get-TreeMemory([int]$root) {
    $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId
    $seen = [System.Collections.Generic.HashSet[int]]::new()
    $null = $seen.Add($root)
    $q = [System.Collections.Generic.Queue[int]]::new(); $q.Enqueue($root)
    while ($q.Count -gt 0) {
        $p = $q.Dequeue()
        foreach ($r in $all) {
            if ($r.ParentProcessId -eq $p -and $seen.Add([int]$r.ProcessId)) { $q.Enqueue([int]$r.ProcessId) }
        }
    }
    $ws = 0L; $pv = 0L; $n = 0
    foreach ($id in $seen) {
        $proc = Get-Process -Id $id -ErrorAction SilentlyContinue
        if ($null -eq $proc) { continue }
        $ws += $proc.WorkingSet64; $pv += $proc.PrivateMemorySize64; $n++
    }
    [pscustomobject]@{ processes = $n; workingSet = $ws; privateBytes = $pv }
}

function Read-Shows {
    if (-not (Test-Path $benchLog)) { return @() }
    Get-Content $benchLog | Where-Object { $_.Trim() } | ForEach-Object { $_ | ConvertFrom-Json } |
        Where-Object { $_.event -eq 'show_to_first_pixel' }
}

# Anything already holding Alt+Space would swallow every press below.
Get-Process takyon -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1

Log "launching $exe"
$env:TAKYON_BENCH_LOG = $benchLog
$proc = Start-Process -FilePath $exe -PassThru
Start-Sleep -Seconds 5
Log "pid $($proc.Id)"

# Warm-up show, discarded. The window has never been painted, so its first show
# includes WebView2's first paint -- a real cost, but not the one being budgeted.
Send-AltSpace; Start-Sleep -Milliseconds 1200
Send-Escape;   Start-Sleep -Milliseconds 1500
$warm = @(Read-Shows)
Log "warm-up shows recorded: $($warm.Count)"
$warmMs = if ($warm.Count) { $warm[-1].ms } else { $null }

$samples = [int](($IdleMinutes * 60) / $SampleIntervalSeconds)
Log "idling $IdleMinutes min, $samples samples"
for ($i = 0; $i -lt $samples; $i++) {
    if (-not (Get-Process -Id $proc.Id -ErrorAction SilentlyContinue)) { Log 'process died during idle'; break }
    $m = Get-TreeMemory $proc.Id
    ([pscustomobject]@{
        minute = [math]::Round($i * $SampleIntervalSeconds / 60, 2)
        processes = $m.processes
        workingSetMb = [math]::Round($m.workingSet / 1MB, 2)
        privateMb = [math]::Round($m.privateBytes / 1MB, 2)
    } | ConvertTo-Json -Compress) | Add-Content -Path $rssLog
    Start-Sleep -Seconds $SampleIntervalSeconds
}

$before = Get-TreeMemory $proc.Id
Log ("pre-show working set: {0} MB across {1} processes" -f [math]::Round($before.workingSet/1MB,2), $before.processes)

$countBefore = @(Read-Shows).Count
Send-AltSpace
$deadline = (Get-Date).AddSeconds(10)
while ((Get-Date) -lt $deadline -and @(Read-Shows).Count -le $countBefore) { Start-Sleep -Milliseconds 50 }
$after = @(Read-Shows)
$coldMs = if ($after.Count -gt $countBefore) { $after[-1].ms } else { $null }
Log "post-idle first show: $coldMs ms"

# A second show immediately after, for the comparison the whole exercise is about.
Send-Escape; Start-Sleep -Milliseconds 800
$c2 = @(Read-Shows).Count
Send-AltSpace
$deadline = (Get-Date).AddSeconds(10)
while ((Get-Date) -lt $deadline -and @(Read-Shows).Count -le $c2) { Start-Sleep -Milliseconds 50 }
$again = @(Read-Shows)
$warmAfterMs = if ($again.Count -gt $c2) { $again[-1].ms } else { $null }
Log "immediately-following show: $warmAfterMs ms"
Send-Escape

[pscustomobject]@{
    idleMinutes            = $IdleMinutes
    firstShowAfterIdleMs   = $coldMs
    showImmediatelyAfterMs = $warmAfterMs
    warmupShowMs           = $warmMs
    preShowWorkingSetMb    = [math]::Round($before.workingSet / 1MB, 2)
    preShowPrivateMb       = [math]::Round($before.privateBytes / 1MB, 2)
    processes              = $before.processes
    benchLog               = $benchLog
    rssLog                 = $rssLog
} | ConvertTo-Json | Set-Content -Path $summary

Log "summary -> $summary"
Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
Log 'done'
