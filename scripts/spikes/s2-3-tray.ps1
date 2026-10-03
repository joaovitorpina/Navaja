# Spike S2.3 (docs/spikes.md): the Windows half of the tray-icon check in
# .github/workflows/spikes.yml, in Windows PowerShell 5.1. It runs the app as
# shipped, finds its icon in the notification area through UI Automation,
# captures the taskbar at the display's own scale, and checks that the
# icon's button has an icon drawn in it. It changes no display setting. The
# macOS half is s2-3-check.sh.
#
# Usage, from anywhere in the repository, after
# `pnpm tauri build --debug --no-bundle` (the shipped configuration):
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/spikes/s2-3-tray.ps1 <mode>
#   before           negative control: no notification-area button named Navaja yet
#   launch           starts target\debug\navaja.exe, promotes its icon out of the
#                    overflow, and waits for its button in the notification area
#   capture          captures the screen, the notification area, and the icon x8;
#                    picks an empty stretch of the taskbar for present-refuses
#   present          an icon is drawn in the button's capture
#   present-refuses  negative control: the same check finds none on that empty stretch
#   stop             quits the app
#
# The modes share a folder, $env:S23_DIR (default: navaja-s2-3 under
# $env:RUNNER_TEMP, or under $env:TEMP outside CI), as s2-3-check.sh does.
# present and present-refuses read only the files there, so they can run on
# a downloaded artifact.
#
# Promoting: Windows 11 puts a new icon in the overflow, behind the chevron.
# Explorer records each icon under HKCU\Control Panel\NotifyIconSettings, and
# IsPromoted = 1 there shows it on the taskbar, as the Settings page does.
param(
  [Parameter(Mandatory = $true, Position = 0)]
  [ValidateSet('before', 'launch', 'capture', 'present', 'present-refuses', 'stop')]
  [string] $Mode
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$Root = (& git rev-parse --show-toplevel).Trim()
$Base = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { $env:TEMP }
$Dir = if ($env:S23_DIR) { $env:S23_DIR } else { Join-Path $Base 'navaja-s2-3' }
$App = Join-Path $Root 'target\debug\navaja.exe'
$null = New-Item -ItemType Directory -Force -Path $Dir

function Fail([string] $Message) {
  Write-Output "::error::S2.3 $Message"
  exit 1
}

# Pixels, not DPI-scaled coordinates, for the capture and UI Automation alike.
Add-Type -Namespace S23 -Name Dpi -MemberDefinition '[DllImport("user32.dll")] public static extern bool SetProcessDPIAware();'
$null = [S23.Dpi]::SetProcessDPIAware()
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, WindowsBase, System.Drawing, System.Windows.Forms

function Show-Display {
  $screen = [System.Windows.Forms.Screen]::PrimaryScreen
  $metrics = Get-ItemProperty 'HKCU:\Control Panel\Desktop\WindowMetrics' -ErrorAction SilentlyContinue
  $dpi = if ($metrics -and ($metrics.PSObject.Properties.Name -contains 'AppliedDPI')) { $metrics.AppliedDPI } else { 96 }
  $graphics = [System.Drawing.Graphics]::FromHwnd([IntPtr]::Zero)
  Write-Output ("Display: {0}x{1} pixels; AppliedDPI {2} ({3} %); the desktop's DPI {4}." -f `
      $screen.Bounds.Width, $screen.Bounds.Height, $dpi, [int]($dpi * 100 / 96), $graphics.DpiX)
  $graphics.Dispose()
  $session = [System.Diagnostics.Process]::GetCurrentProcess().SessionId
  $explorer = @(Get-Process explorer -ErrorAction SilentlyContinue | ForEach-Object { "pid $($_.Id), session $($_.SessionId)" })
  Write-Output ("This shell: session {0}, interactive {1}; explorer: {2}." -f `
      $session, [Environment]::UserInteractive, ($(if ($explorer) { $explorer -join '; ' } else { 'not running' })))
}

# The taskbar's buttons (the notification area's icons among them), as UI
# Automation lists them.
function Get-TaskbarButtons {
  $root = [System.Windows.Automation.AutomationElement]::RootElement
  $byClass = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ClassNameProperty, 'Shell_TrayWnd')
  $taskbar = $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $byClass)
  if (-not $taskbar) { return @() }
  $buttons = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    [System.Windows.Automation.ControlType]::Button)
  return @($taskbar.FindAll([System.Windows.Automation.TreeScope]::Descendants, $buttons))
}

# The notification-area icon, not the app's own taskbar button (the app
# shows its window once the front end is ready, window.rs): Explorer names
# the icon after its tooltip, "Navaja", and gives it this AutomationId.
function Find-NavajaButton {
  foreach ($button in (Get-TaskbarButtons)) {
    if ($button.Current.AutomationId -eq 'NotifyItemIcon' -and $button.Current.Name -like 'Navaja*') { return $button }
  }
  return $null
}

function Show-Buttons {
  foreach ($button in (Get-TaskbarButtons)) {
    $r = $button.Current.BoundingRectangle
    Write-Output ("  {0,-40} {1},{2} {3}x{4}  {5}" -f $button.Current.Name, $r.X, $r.Y, $r.Width, $r.Height, $button.Current.AutomationId)
  }
}

# A notification-area button is as tall as the taskbar, whose top row is a
# 1-pixel edge in another grey. The pixel checks look inside the button,
# this many pixels in from each side, where only the icon and the
# taskbar's background are.
$Inset = 4

# What one rectangle (x y w h, pixels) of a capture shows, measured as
# s2-3-menubar.swift does on macOS. The background is the median of the
# rectangle's outermost ring of pixels, which the icon leaves empty. Ink is
# every pixel at least 48 of 255 away from it in some channel; the core is
# the ink at 60 % or more of the strongest contrast.
function Measure-Area([System.Drawing.Bitmap] $Bitmap, [int[]] $Rect) {
  $x0 = $Rect[0] + $Inset
  $y0 = $Rect[1] + $Inset
  $x1 = $Rect[0] + $Rect[2] - $Inset
  $y1 = $Rect[1] + $Rect[3] - $Inset
  if ($x0 -lt 0 -or $y0 -lt 0 -or $x1 -gt $Bitmap.Width -or $y1 -gt $Bitmap.Height -or $x1 -le $x0 + 2 -or $y1 -le $y0 + 2) {
    Fail "rectangle $($Rect -join ' ') is outside the capture"
  }
  $ring = New-Object System.Collections.Generic.List[System.Drawing.Color]
  for ($x = $x0; $x -lt $x1; $x++) { $ring.Add($Bitmap.GetPixel($x, $y0)); $ring.Add($Bitmap.GetPixel($x, $y1 - 1)) }
  for ($y = $y0; $y -lt $y1; $y++) { $ring.Add($Bitmap.GetPixel($x0, $y)); $ring.Add($Bitmap.GetPixel($x1 - 1, $y)) }
  $middle = [int][math]::Floor($ring.Count / 2)
  $bg = @(
    @($ring | ForEach-Object { [int]$_.R } | Sort-Object)[$middle],
    @($ring | ForEach-Object { [int]$_.G } | Sort-Object)[$middle],
    @($ring | ForEach-Object { [int]$_.B } | Sort-Object)[$middle]
  )
  $pixels = 0
  $ink = New-Object System.Collections.Generic.List[object]
  $strongest = 0
  $box = @([int]::MaxValue, [int]::MaxValue, [int]::MinValue, [int]::MinValue)
  for ($y = $y0; $y -lt $y1; $y++) {
    for ($x = $x0; $x -lt $x1; $x++) {
      $c = $Bitmap.GetPixel($x, $y)
      $d = [math]::Max([math]::Abs($c.R - $bg[0]), [math]::Max([math]::Abs($c.G - $bg[1]), [math]::Abs($c.B - $bg[2])))
      $pixels++
      if ($d -gt $strongest) { $strongest = $d }
      if ($d -ge 48) {
        $ink.Add([pscustomobject]@{ Contrast = $d; R = [int]$c.R; G = [int]$c.G; B = [int]$c.B })
        $box = @([math]::Min($box[0], $x), [math]::Min($box[1], $y), [math]::Max($box[2], $x), [math]::Max($box[3], $y))
      }
    }
  }
  $core = @($ink | Where-Object { $_.Contrast -ge 0.6 * $strongest })
  $n = [math]::Max(1, $core.Count)
  $mean = foreach ($channel in 'R', 'G', 'B') {
    [int]((($core | ForEach-Object { $_.$channel }) | Measure-Object -Sum).Sum / $n)
  }
  return [pscustomobject]@{
    Area       = "{0} {1} {2} {3}" -f $x0, $y0, ($x1 - $x0), ($y1 - $y0)
    Pixels     = $pixels
    Background = '#{0:x2}{1:x2}{2:x2}' -f $bg[0], $bg[1], $bg[2]
    Ink        = $ink.Count
    InkShare   = $ink.Count / [math]::Max(1, $pixels)
    Core       = $core.Count
    CoreColor  = '#{0:x2}{1:x2}{2:x2}' -f $mean[0], $mean[1], $mean[2]
    Strongest  = $strongest
    InkBox     = $(if ($ink.Count) { "{0}x{1}" -f ($box[2] - $box[0] + 1), ($box[3] - $box[1] + 1) } else { '0x0' })
  }
}

function Show-Measure($Measure) {
  "area {0} (x y w h, inside the button): pixels={1} background={2} ink={3} ({4:N1}%) core={5} core_colour={6} max_contrast={7} ink_box={8}" -f `
    $Measure.Area, $Measure.Pixels, $Measure.Background, $Measure.Ink, ($Measure.InkShare * 100), $Measure.Core,
  $Measure.CoreColor, $Measure.Strongest, $Measure.InkBox
}

# The macOS rule: an icon is drawn when at least 3 % of the pixels are ink
# and the strongest contrast is at least 96 of 255.
function Test-Present($Measure) {
  return ($Measure.InkShare -ge 0.03 -and $Measure.Strongest -ge 96)
}

function Read-Rect([string] $Name) {
  $file = Join-Path $Dir $Name
  if (-not (Test-Path $file)) { return $null }
  return [int[]]((Get-Content $file).Trim().Split(' '))
}

function Read-Capture {
  $file = Join-Path $Dir 'windows-screen.png'
  if (-not (Test-Path $file)) { Fail "no $file; run 'capture' first" }
  return New-Object System.Drawing.Bitmap $file
}

function Save-Crop([System.Drawing.Bitmap] $Source, [System.Drawing.Rectangle] $Area, [string] $Path, [int] $Zoom) {
  $area = [System.Drawing.Rectangle]::Intersect($Area, (New-Object System.Drawing.Rectangle 0, 0, $Source.Width, $Source.Height))
  $out = New-Object System.Drawing.Bitmap ($area.Width * $Zoom), ($area.Height * $Zoom)
  $graphics = [System.Drawing.Graphics]::FromImage($out)
  $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::NearestNeighbor
  $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::Half
  $graphics.DrawImage($Source, (New-Object System.Drawing.Rectangle 0, 0, $out.Width, $out.Height), $area, [System.Drawing.GraphicsUnit]::Pixel)
  $graphics.Dispose()
  $out.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
  $out.Dispose()
  Write-Output ("{0}: {1}x{2} pixels ({3}x{4} from the capture, x{5})." -f $Path, ($area.Width * $Zoom), ($area.Height * $Zoom), $area.Width, $area.Height, $Zoom)
}

switch ($Mode) {
  'before' {
    Show-Display
    if (Get-Process navaja -ErrorAction SilentlyContinue) { Fail 'a navaja process is already running; quit it first' }
    Write-Output 'Taskbar buttons before the app starts:'
    Show-Buttons
    if (Find-NavajaButton) { Fail 'before: a notification-area button named Navaja exists before the app starts' }
    if (-not (Get-TaskbarButtons)) { Fail 'before: UI Automation finds no taskbar buttons at all, so the check would pass on nothing' }
    Write-Output 'No Navaja button in the notification area yet.'
  }

  'launch' {
    if (-not (Test-Path $App)) { Fail "no $App; build it first with: pnpm tauri build --debug --no-bundle" }
    $appDir = Join-Path $Dir 'app-dir'
    $null = New-Item -ItemType Directory -Force -Path $appDir
    $env:NAVAJA_APP_DIR = $appDir
    $process = Start-Process -FilePath $App -PassThru -RedirectStandardOutput (Join-Path $Dir 'navaja.out') -RedirectStandardError (Join-Path $Dir 'navaja.err')
    Set-Content -Path (Join-Path $Dir 'navaja.pid') -Value $process.Id
    Write-Output "Started $App, pid $($process.Id)."

    # Explorer's record of the icon, keyed by the executable's path.
    $settings = 'HKCU:\Control Panel\NotifyIconSettings'
    $entry = $null
    $deadline = (Get-Date).AddSeconds(60)
    while (-not $entry -and (Get-Date) -lt $deadline) {
      if (Test-Path $settings) {
        $entry = Get-ChildItem $settings | Where-Object {
          $path = $_.GetValue('ExecutablePath')
          $path -and ($path -like '*\navaja.exe')
        } | Select-Object -First 1
      }
      if (-not $entry) { Start-Sleep -Milliseconds 250 }
    }
    if ($entry) {
      Write-Output ("Explorer's record: {0}, IsPromoted {1}." -f $entry.Name, $entry.GetValue('IsPromoted'))
      Set-ItemProperty -Path $entry.PSPath -Name IsPromoted -Value 1 -Type DWord
      Write-Output 'Set IsPromoted = 1.'
    } else {
      Write-Output "::warning::S2.3 no NotifyIconSettings entry for navaja.exe within 60 s; the icon may stay in the overflow"
    }

    $button = $null
    $deadline = (Get-Date).AddSeconds(30)
    while (-not $button -and (Get-Date) -lt $deadline) {
      $button = Find-NavajaButton
      if (-not $button) { Start-Sleep -Milliseconds 250 }
    }
    Write-Output 'Taskbar buttons with the app running:'
    Show-Buttons
    if (-not $button) { Fail 'launch: no notification-area button named Navaja within 30 s' }
    $r = $button.Current.BoundingRectangle
    Set-Content -Path (Join-Path $Dir 'icon.rect') -Value ("{0} {1} {2} {3}" -f [int]$r.X, [int]$r.Y, [int]$r.Width, [int]$r.Height)
    Write-Output ("Navaja's notification-area button: '{0}' at {1},{2}, {3}x{4} pixels." -f $button.Current.Name, $r.X, $r.Y, $r.Width, $r.Height)
  }

  'capture' {
    Show-Display
    $icon = Read-Rect 'icon.rect'
    if (-not $icon) { Fail "no icon rectangle; run 'launch' first" }
    $x, $y, $w, $h = $icon

    # For present-refuses: a stretch of the taskbar, as large as the icon's
    # button and at its height, that overlaps nothing UI Automation lists
    # there. Elements half the taskbar's width or wider are containers, not
    # drawn items, so they do not count.
    $root = [System.Windows.Automation.AutomationElement]::RootElement
    $byClass = New-Object System.Windows.Automation.PropertyCondition(
      [System.Windows.Automation.AutomationElement]::ClassNameProperty, 'Shell_TrayWnd')
    $taskbar = $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $byClass)
    if (-not $taskbar) { Fail 'capture: UI Automation finds no taskbar' }
    $bar = $taskbar.Current.BoundingRectangle
    $items = @($taskbar.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition) |
        ForEach-Object { $_.Current.BoundingRectangle } |
        Where-Object { -not $_.IsEmpty -and $_.Width -gt 0 -and $_.Height -gt 0 -and $_.Width -lt $bar.Width / 2 })
    Write-Output ("Taskbar: {0},{1} {2}x{3}; {4} elements narrower than half of it." -f $bar.X, $bar.Y, $bar.Width, $bar.Height, $items.Count)
    $empty = $null
    for ($ex = [int]$bar.X + 8; $ex + $w -le $bar.X + $bar.Width - 8; $ex += 4) {
      $candidate = New-Object System.Windows.Rect $ex, $y, $w, $h
      if (-not ($items | Where-Object { $_.IntersectsWith($candidate) })) { $empty = $candidate; break }
    }
    if ($empty) {
      Set-Content -Path (Join-Path $Dir 'empty.rect') -Value ("{0} {1} {2} {3}" -f [int]$empty.X, [int]$empty.Y, [int]$empty.Width, [int]$empty.Height)
      Write-Output ("An empty stretch of the taskbar: {0},{1}, {2}x{3} pixels." -f $empty.X, $empty.Y, $empty.Width, $empty.Height)
    } else {
      Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $Dir 'empty.rect')
      Write-Output '::warning::S2.3 no stretch of the taskbar is free of UI Automation elements; present-refuses will fail'
    }

    $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $screen = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
    $graphics = [System.Drawing.Graphics]::FromImage($screen)
    $graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
    $graphics.Dispose()
    $screen.Save((Join-Path $Dir 'windows-screen.png'), [System.Drawing.Imaging.ImageFormat]::Png)
    Write-Output "$(Join-Path $Dir 'windows-screen.png'): $($bounds.Width)x$($bounds.Height) pixels."
    # The taskbar's right end, from 240 pixels left of the icon, the icon
    # alone, and the empty stretch.
    $right = New-Object System.Drawing.Rectangle ($x - 240), ($y - 8), ($bounds.Width - $x + 240), ($h + 16)
    Save-Crop $screen $right (Join-Path $Dir 'windows-tray.png') 1
    Save-Crop $screen (New-Object System.Drawing.Rectangle $x, $y, $w, $h) (Join-Path $Dir 'windows-icon-x8.png') 8
    if ($empty) {
      Save-Crop $screen (New-Object System.Drawing.Rectangle ([int]$empty.X), ([int]$empty.Y), $w, $h) (Join-Path $Dir 'windows-empty-x8.png') 8
    }
    $screen.Dispose()
  }

  'present' {
    $icon = Read-Rect 'icon.rect'
    if (-not $icon) { Fail "no icon rectangle; run 'launch' first" }
    $capture = Read-Capture
    $measure = Measure-Area $capture $icon
    $capture.Dispose()
    Write-Output "Navaja's button at $($icon -join ' '): $(Show-Measure $measure)"
    if (-not (Test-Present $measure)) {
      Fail ("present: no icon in Navaja's button: ink {0:N1}% (needs 3%), max contrast {1} (needs 96)" -f ($measure.InkShare * 100), $measure.Strongest)
    }
    Write-Output 'An icon is drawn there.'
  }

  # The negative control for `present`: the same check, on a stretch of the
  # same capture's taskbar where nothing is, must find no icon.
  'present-refuses' {
    $empty = Read-Rect 'empty.rect'
    if (-not $empty) { Fail "present-refuses: no empty stretch of the taskbar was found; see the 'capture' step" }
    $capture = Read-Capture
    $measure = Measure-Area $capture $empty
    $capture.Dispose()
    Write-Output "The empty stretch at $($empty -join ' '): $(Show-Measure $measure)"
    if (Test-Present $measure) { Fail 'present-refuses: the check found an icon on an empty stretch of the taskbar' }
    Write-Output ("The check finds no icon there: ink {0:N1}% (needs 3%), max contrast {1} (needs 96)." -f ($measure.InkShare * 100), $measure.Strongest)
  }

  'stop' {
    $pidFile = Join-Path $Dir 'navaja.pid'
    if (Test-Path $pidFile) {
      $id = [int](Get-Content $pidFile)
      Stop-Process -Id $id -Force -ErrorAction SilentlyContinue
      Write-Output 'Navaja stopped.'
    }
  }
}
