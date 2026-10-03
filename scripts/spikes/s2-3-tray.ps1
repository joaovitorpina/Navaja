# Spike S2.3 (docs/spikes.md): the Windows half of the tray-icon check in
# .github/workflows/spikes.yml, in Windows PowerShell 5.1. It runs the app as
# shipped, finds its icon in the notification area through UI Automation, and
# captures the taskbar at the display's own scale. It changes no display
# setting. The macOS half is s2-3-check.sh.
#
# Usage, from anywhere in the repository, after
# `pnpm tauri build --debug --no-bundle` (the shipped configuration):
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/spikes/s2-3-tray.ps1 <mode>
#   before   negative control: no notification-area button named Navaja yet
#   launch   starts target\debug\navaja.exe, promotes its icon out of the
#            overflow, and waits for its button in the notification area
#   capture  captures the screen, the notification area, and the icon x8
#   stop     quits the app
#
# The modes share a folder, $env:S23_DIR (default: navaja-s2-3 under
# $env:RUNNER_TEMP, or under $env:TEMP outside CI), as s2-3-check.sh does.
#
# Promoting: Windows 11 puts a new icon in the overflow, behind the chevron.
# Explorer records each icon under HKCU\Control Panel\NotifyIconSettings, and
# IsPromoted = 1 there shows it on the taskbar, as the Settings page does.
param(
  [Parameter(Mandatory = $true, Position = 0)]
  [ValidateSet('before', 'launch', 'capture', 'stop')]
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
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing, System.Windows.Forms

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

function Find-NavajaButton {
  foreach ($button in (Get-TaskbarButtons)) {
    if ($button.Current.Name -like 'Navaja*') { return $button }
  }
  return $null
}

function Show-Buttons {
  foreach ($button in (Get-TaskbarButtons)) {
    $r = $button.Current.BoundingRectangle
    Write-Output ("  {0,-40} {1},{2} {3}x{4}  {5}" -f $button.Current.Name, $r.X, $r.Y, $r.Width, $r.Height, $button.Current.AutomationId)
  }
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
    $rectFile = Join-Path $Dir 'icon.rect'
    if (-not (Test-Path $rectFile)) { Fail "no icon rectangle; run 'launch' first" }
    $x, $y, $w, $h = (Get-Content $rectFile).Split(' ') | ForEach-Object { [int]$_ }
    $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $screen = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
    $graphics = [System.Drawing.Graphics]::FromImage($screen)
    $graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
    $graphics.Dispose()
    $screen.Save((Join-Path $Dir 'windows-screen.png'), [System.Drawing.Imaging.ImageFormat]::Png)
    Write-Output "$(Join-Path $Dir 'windows-screen.png'): $($bounds.Width)x$($bounds.Height) pixels."
    # The taskbar's right end, from 240 pixels left of the icon, and the icon alone.
    $bar = New-Object System.Drawing.Rectangle ($x - 240), ($y - 8), ($bounds.Width - $x + 240), ($h + 16)
    Save-Crop $screen $bar (Join-Path $Dir 'windows-tray.png') 1
    Save-Crop $screen (New-Object System.Drawing.Rectangle $x, $y, $w, $h) (Join-Path $Dir 'windows-icon-x8.png') 8
    $screen.Dispose()
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
