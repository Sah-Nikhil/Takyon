<#
.SYNOPSIS
Drive the automatable half of docs/verify/v0.10.md sections A and B.

What is left after the Playwright suite is everything WebView2 decides and a
browser does not. Two things in particular:

  - **oklch and color-mix(in oklab) have to actually render.** Every theme value
    is authored in oklch and every derived token is an oklab mix; Chromium in the
    test harness supports both, and WebView2 on the operator's machine is a
    different build. The failure is silent - an unsupported colour resolves to
    nothing and the surface paints transparent - so this samples real pixels.
  - **Window mode resizes a native window.** `EXPANDED_HEIGHT` lives in Rust and
    nothing in the webview can see it, so the browser suite cannot tell Compact
    from Expanded at all.

Same two rules as scripts/verify-drive.ps1. It refuses to type unless the
foreground window belongs to takyon, because injected keys go wherever focus is.
And it launches the binary itself, because a GUI process started from a tool call
is reaped when that call returns.

Section E (the Windows key) stays manual and cannot be otherwise: the hook is
installed against a real desktop, and a script that pressed the Windows key would
be typing into whatever it opened.
#>
[CmdletBinding()]
param(
    [string]$Exe = "apps\desktop\src-tauri\target\release\takyon.exe",
    [string]$OutDir = "$env:TEMP\takyon-verify-v0.10",
    # Free chord. Alt+Space is contested by Raycast and PowerToys Run on most
    # machines this is developed on.
    [string]$Hotkey = "Ctrl+Alt+Shift+F9"
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type -Namespace Takyon -Name Drive10 -MemberDefinition @'
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(IntPtr h, out int pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll", SetLastError=true)] public static extern void keybd_event(byte vk, byte sc, uint f, System.UIntPtr x);
  public struct RECT { public int Left, Top, Right, Bottom; }
'@

# Without this the capture photographs the top-left fraction of a scaled display.
[void][Takyon.Drive10]::SetProcessDPIAware()
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$script:pass = 0
$script:fail = 0
function Step([string]$id, [bool]$ok, [string]$note = "") {
    if ($ok) { $script:pass++; Write-Host "  PASS  $id $note" -ForegroundColor Green }
    else     { $script:fail++; Write-Host "  FAIL  $id $note" -ForegroundColor Red }
}

if (-not (Test-Path $Exe)) {
    # Named rather than guessed at: a bare `cargo build --release` produces an exe
    # that launches with a completely dead frontend, which fails in the one way
    # that looks like a Rust bug (CLAUDE.md, Gotchas).
    throw "$Exe not found. Run `bun run build` - never a bare cargo build."
}

Write-Host "Launching $Exe" -ForegroundColor Cyan
$env:TAKYON_HOTKEY = $Hotkey
$proc = Start-Process -FilePath $Exe -PassThru
Start-Sleep -Seconds 6

function Show-Palette {
    # Ctrl+Alt+Shift+F9, down then up, in the order Windows expects.
    foreach ($vk in 0x11, 0x12, 0x10) { [Takyon.Drive10]::keybd_event($vk, 0, 0, [UIntPtr]::Zero) }
    [Takyon.Drive10]::keybd_event(0x78, 0, 0, [UIntPtr]::Zero)
    [Takyon.Drive10]::keybd_event(0x78, 0, 2, [UIntPtr]::Zero)
    foreach ($vk in 0x10, 0x12, 0x11) { [Takyon.Drive10]::keybd_event($vk, 0, 2, [UIntPtr]::Zero) }
    Start-Sleep -Milliseconds 700
}

function Get-PaletteRect {
    $h = [Takyon.Drive10]::GetForegroundWindow()
    # `$owner`, not `$pid`: PowerShell reserves `$PID` for the current process
    # and assigning it is a hard error, not a shadow.
    $owner = 0
    [void][Takyon.Drive10]::GetWindowThreadProcessId($h, [ref]$owner)
    if ($owner -ne $proc.Id) { return $null }
    $r = New-Object Takyon.Drive10+RECT
    if (-not [Takyon.Drive10]::GetWindowRect($h, [ref]$r)) { return $null }
    return $r
}

function Save-Shot([string]$name, $r) {
    $w = $r.Right - $r.Left
    $h = $r.Bottom - $r.Top
    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($r.Left, $r.Top, 0, 0, $bmp.Size)
    $path = Join-Path $OutDir "$name.png"
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    return $path
}

<#
  The plate, sampled from inside the panel.

  The window rect is bigger than the panel: the window is transparent and
  undecorated, and Windows draws its shadow inside the same rect. So a sample
  near the origin photographs the desktop through the shadow, which is what the
  first version of this script did - it read a green pixel off a wallpaper and
  still passed, because "not black and not white" is true of wallpaper too.

  The right-hand end of the input row is the one region that is panel in every
  mode, at every width, with no glyph in it.
#>
function Get-PlatePixel($r) {
    $x = $r.Right - 60
    $y = $r.Top + [int](($r.Bottom - $r.Top) * 0.25)
    $bmp = New-Object System.Drawing.Bitmap 1, 1
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($x, $y, 0, 0, $bmp.Size)
    $c = $bmp.GetPixel(0, 0)
    $g.Dispose(); $bmp.Dispose()
    return $c
}

try {
    Show-Palette
    $rect = Get-PaletteRect
    Step "launch" ($null -ne $rect) "the Palette took the foreground"
    if ($null -eq $rect) { throw "no Palette window; nothing below can be measured" }

    $height = $rect.Bottom - $rect.Top
    Write-Host "  Palette height: $height physical px" -ForegroundColor DarkGray

    <#
      A1/A2 in pixels rather than by eye.

      An unsupported colour function does not throw and does not warn: the
      declaration is dropped, the element paints its inherited background, and
      over a `transparent: true` window that is the desktop. So a plate pixel
      that is neither black nor the wallpaper is the evidence oklch resolved.
    #>
    $plate = Get-PlatePixel $rect
    Write-Host "  plate pixel: R$($plate.R) G$($plate.G) B$($plate.B)" -ForegroundColor DarkGray
    <#
      Graphite's dark plate is oklch(0.1905 0.0045 286), which resolves to about
      R20 G20 B22 - a near-neutral dark. Asserted as "dark and near-neutral"
      rather than as three exact numbers, because the panel is 95% opaque over a
      backdrop blur, so the wallpaper shifts it by a point or two.

      "Not black and not white" is not enough on its own: an unresolved colour
      function paints nothing, and nothing over an arbitrary wallpaper is any
      colour at all. The channel spread is what separates a real plate from one.
    #>
    $sum = $plate.R + $plate.G + $plate.B
    $spread = ([int[]]@($plate.R, $plate.G, $plate.B) | Measure-Object -Maximum).Maximum -
              ([int[]]@($plate.R, $plate.G, $plate.B) | Measure-Object -Minimum).Minimum
    $painted = ($sum -gt 10) -and ($sum -lt 180) -and ($spread -lt 24)
    Step "A-oklch" $painted "plate is dark and near-neutral (sum $sum, spread $spread), so oklch resolved"

    $shot = Save-Shot "palette-compact" $rect
    Write-Host "  $shot" -ForegroundColor DarkGray

    # B1: Compact is the default and is ~68 logical px. Compared loosely because
    # the operator's scaling is unknown and this is a shape check, not a budget.
    Step "B1" ($height -lt 200) "Compact opened short ($height px)"

    <#
      B3 without a person.

      The mode is a stored preference, so the way to drive it is a second
      instance against a scratch LOCALAPPDATA whose settings.db already says
      expanded. That is also the only way this script may touch the preference at
      all: writing into the operator's real settings.db to take a measurement
      would leave their launcher in whatever state the script died in.
    #>
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2

    $sqlite = (Get-Command sqlite3 -ErrorAction SilentlyContinue).Source
    if (-not $sqlite) {
        Step "B3" $false "sqlite3 is not on PATH; cannot seed the scratch profile"
    } else {
        $scratch = Join-Path $OutDir "profile"
        Remove-Item $scratch -Recurse -Force -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Force -Path (Join-Path $scratch "v3sper\takyon") | Out-Null
        $db = Join-Path $scratch "v3sper\takyon\settings.db"
        & $sqlite $db "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL); INSERT INTO settings (key,value) VALUES ('ui.window-mode','expanded');" | Out-Null

        $realLocal = $env:LOCALAPPDATA
        $env:LOCALAPPDATA = $scratch
        $proc = Start-Process -FilePath $Exe -PassThru
        # Longer than the first launch: this profile is empty, so it creates four
        # databases and walks the applications before the hotkey is answered.
        Start-Sleep -Seconds 12
        $rect2 = $null
        # Retried, because the previous instance's single-instance mutex can
        # outlive its process by a moment and the show is simply dropped.
        foreach ($try in 1..3) {
            Show-Palette
            $rect2 = Get-PaletteRect
            if ($null -ne $rect2) { break }
            Start-Sleep -Seconds 3
        }
        $env:LOCALAPPDATA = $realLocal

        if ($null -ne $rect2) {
            $h2 = $rect2.Bottom - $rect2.Top
            Write-Host "  Expanded height: $h2 physical px" -ForegroundColor DarkGray
            # Not compared against EXPANDED_HEIGHT: that constant is logical and
            # the operator's scaling is unknown. "Several times Compact" is the
            # claim the mode actually makes.
            Step "B3" ($h2 -gt $height * 2) "Expanded ($h2) is much taller than Compact ($height)"
            $shot2 = Save-Shot "palette-expanded" $rect2
            Write-Host "  $shot2" -ForegroundColor DarkGray
        } else {
            Step "B3" $false "the Palette did not come back to the foreground"
        }
        Remove-Item $scratch -Recurse -Force -ErrorAction SilentlyContinue
    }
} finally {
    if ($proc -and -not $proc.HasExited) {
        Write-Host ""
        Write-Host "Stopping takyon (pid $($proc.Id))" -ForegroundColor Cyan
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    }
    Remove-Item Env:\TAKYON_HOTKEY -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "$($script:pass) passed, $($script:fail) failed. Shots in $OutDir" -ForegroundColor Cyan
Write-Host "Sections A3-A9, B4-B9, C, D, E and F stay manual." -ForegroundColor DarkGray
if ($script:fail -gt 0) { exit 1 }
