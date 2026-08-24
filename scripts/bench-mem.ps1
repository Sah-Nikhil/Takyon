# Sum the memory of a process AND everything below it, as JSON.
#
# WebView2 is not one process. A single Takyon runs a Rust host, a WebView2 browser
# process, a renderer and a GPU process -- and the renderer is a child of the
# browser process, not of us, so a one-level walk misses it. Measuring only the
# main process would report roughly the Rust binary's footprint and quietly claim
# the 150 MB budget was met with room to spare.
#
# Two numbers, because they answer different questions:
#   WorkingSet64       - what is resident in RAM right now. This is the number
#                        ADR-0003's trim-on-hide is trying to move, and the one to
#                        compare against the budget.
#   PrivateMemorySize64 - what has been committed. It does not drop when the
#                        working set is trimmed, so a large gap between the two is
#                        exactly the evidence that trimming did something.

param(
    [Parameter(Mandatory = $true)]
    [int]$RootPid
)

$all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name

# Breadth-first, with a visited set: Windows recycles pids, so a table can contain
# a cycle, and walking one naively never returns.
$seen = [System.Collections.Generic.HashSet[int]]::new()
$null = $seen.Add($RootPid)
$frontier = [System.Collections.Generic.Queue[int]]::new()
$frontier.Enqueue($RootPid)

while ($frontier.Count -gt 0) {
    $parent = $frontier.Dequeue()
    foreach ($p in $all) {
        if ($p.ParentProcessId -eq $parent -and $seen.Add([int]$p.ProcessId)) {
            $frontier.Enqueue([int]$p.ProcessId)
        }
    }
}

$working = 0L
$private = 0L
$counted = @()

foreach ($id in $seen) {
    $proc = Get-Process -Id $id -ErrorAction SilentlyContinue
    if ($null -eq $proc) { continue }   # exited between the snapshot and here
    $working += $proc.WorkingSet64
    $private += $proc.PrivateMemorySize64
    $counted += [pscustomobject]@{
        pid        = $id
        name       = $proc.ProcessName
        workingSet = $proc.WorkingSet64
    }
}

[pscustomobject]@{
    processes     = $counted.Count
    workingSet    = $working
    privateBytes  = $private
    breakdown     = $counted
} | ConvertTo-Json -Depth 4 -Compress
