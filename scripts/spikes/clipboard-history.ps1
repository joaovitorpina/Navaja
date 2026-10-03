# M2a exit check "Copy stays out of Windows clipboard history"
# (docs/spikes.md): the clipboard and clipboard-history access that
# exit-clipboard-check.sh needs, in Windows PowerShell 5.1, which reaches the
# WinRT clipboard API (Windows.ApplicationModel.DataTransfer.Clipboard)
# without extra packages.
#
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/spikes/clipboard-history.ps1 <mode> [-Text <text>] [-Seconds <n>]
#   enable         turns clipboard history on for this user (EnableClipboardHistory = 1),
#                  restarts the per-user clipboard service, and waits until the API says it is on
#   status         whether history is on, and what it holds
#   put            puts -Text on the clipboard the ordinary way, with no exclusion formats
#   clipboard-is   fails unless the clipboard holds -Text
#   history-has    waits up to -Seconds for -Text to appear in clipboard history
#   history-lacks  waits -Seconds, then fails if -Text is in clipboard history
#
# `enable` changes this user's settings; it is meant for CI runners.
param(
  [Parameter(Mandatory = $true, Position = 0)]
  [ValidateSet('enable', 'status', 'put', 'clipboard-is', 'history-has', 'history-lacks')]
  [string] $Mode,
  [string] $Text = '',
  [int] $Seconds = 30
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Fail([string] $Message) {
  Write-Output "::error::clipboard-history $Message"
  exit 1
}

Add-Type -AssemblyName System.Runtime.WindowsRuntime
$null = [Windows.ApplicationModel.DataTransfer.Clipboard, Windows.ApplicationModel.DataTransfer, ContentType = WindowsRuntime]
$null = [Windows.ApplicationModel.DataTransfer.ClipboardHistoryItemsResult, Windows.ApplicationModel.DataTransfer, ContentType = WindowsRuntime]
$null = [Windows.ApplicationModel.DataTransfer.StandardDataFormats, Windows.ApplicationModel.DataTransfer, ContentType = WindowsRuntime]

# WinRT's IAsyncOperation<T>, awaited through .NET's AsTask<T>.
$AsTask = [System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
  $_.Name -eq 'AsTask' -and $_.IsGenericMethod -and $_.GetParameters().Count -eq 1 -and
  $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1'
} | Select-Object -First 1

function Await($Operation, [type] $Type) {
  $task = $AsTask.MakeGenericMethod($Type).Invoke($null, @($Operation))
  if (-not $task.Wait(15000)) { Fail 'a WinRT call did not finish within 15 s' }
  return $task.Result
}

$Clipboard = [Windows.ApplicationModel.DataTransfer.Clipboard]

# The history's texts, newest first, and the API's status for the call.
function Get-History {
  $result = Await ($Clipboard::GetHistoryItemsAsync()) ([Windows.ApplicationModel.DataTransfer.ClipboardHistoryItemsResult])
  $texts = @()
  if ("$($result.Status)" -eq 'Success') {
    foreach ($item in $result.Items) {
      if ($item.Content.Contains([Windows.ApplicationModel.DataTransfer.StandardDataFormats]::Text)) {
        $texts += [string](Await ($item.Content.GetTextAsync()) ([string]))
      } else {
        $texts += '(no text)'
      }
    }
  }
  return [pscustomobject]@{ Status = "$($result.Status)"; Texts = $texts }
}

function Show-History($History) {
  Write-Output ("History: status {0}, {1} item(s), newest first:" -f $History.Status, $History.Texts.Count)
  foreach ($text in $History.Texts) { Write-Output "  | $text" }
}

function Show-Session {
  $session = [System.Diagnostics.Process]::GetCurrentProcess().SessionId
  $explorer = @(Get-Process explorer -ErrorAction SilentlyContinue | ForEach-Object { "pid $($_.Id), session $($_.SessionId)" })
  Write-Output ("This shell: {0}, session {1}, interactive {2}, apartment {3}; explorer: {4}." -f `
      [Security.Principal.WindowsIdentity]::GetCurrent().Name, $session, [Environment]::UserInteractive,
    [Threading.Thread]::CurrentThread.GetApartmentState(),
    ($(if ($explorer) { $explorer -join '; ' } else { 'not running' })))
  $os = Get-CimInstance Win32_OperatingSystem
  Write-Output ("Windows: {0}, build {1}." -f $os.Caption, $os.BuildNumber)
}

function Show-Services {
  foreach ($service in @(Get-Service -Name 'cbdhsvc*' -ErrorAction SilentlyContinue)) {
    Write-Output ("Service {0}: {1}, start {2}." -f $service.Name, $service.Status, $service.StartType)
  }
}

function Show-Keys {
  foreach ($path in 'HKCU:\Software\Microsoft\Clipboard', 'HKLM:\SOFTWARE\Microsoft\Clipboard',
    'HKLM:\SOFTWARE\Policies\Microsoft\Windows\System') {
    $item = Get-ItemProperty $path -ErrorAction SilentlyContinue
    $values = if ($item) {
      @($item.PSObject.Properties | Where-Object { $_.Name -notlike 'PS*' } | ForEach-Object { "$($_.Name) = $($_.Value)" }) -join ', '
    } else { 'no key' }
    Write-Output "${path}: $(if ($values) { $values } else { 'no values' })"
  }
}

# The per-user instance (cbdhsvc_<id>) reads the setting when it starts.
function Restart-ClipboardService {
  foreach ($service in @(Get-Service -Name 'cbdhsvc_*' -ErrorAction SilentlyContinue)) {
    try {
      if ($service.Status -eq 'Running') {
        Restart-Service -Name $service.Name -Force
        Write-Output "Restarted $($service.Name)."
      } else {
        Start-Service -Name $service.Name
        Write-Output "Started $($service.Name)."
      }
    } catch {
      Write-Output "::warning::clipboard-history could not (re)start $($service.Name): $($_.Exception.Message)"
    }
  }
  Show-Services
}

# Whether history is on, as this process and a fresh one each see it. It
# returns only the answer; the line it prints goes to the host.
function Test-Enabled {
  $here = $Clipboard::IsHistoryEnabled()
  $fresh = & powershell.exe -NoProfile -Command '[Windows.ApplicationModel.DataTransfer.Clipboard, Windows.ApplicationModel.DataTransfer, ContentType = WindowsRuntime]::IsHistoryEnabled()'
  Write-Host "Clipboard.IsHistoryEnabled(): $here here, $("$fresh".Trim()) in a fresh process."
  return [bool]($here -and "$fresh".Trim() -eq 'True')
}

function Wait-Enabled([int] $For) {
  $deadline = (Get-Date).AddSeconds($For)
  while (-not (Test-Enabled)) {
    if ((Get-Date) -gt $deadline) { return $false }
    Start-Sleep -Seconds 1
  }
  return $true
}

switch ($Mode) {
  'enable' {
    Show-Session
    Show-Keys
    $key = 'HKCU:\Software\Microsoft\Clipboard'
    if (-not (Test-Path $key)) { $null = New-Item -Path $key -Force }
    Set-ItemProperty -Path $key -Name EnableClipboardHistory -Value 1 -Type DWord
    Write-Output 'Set HKCU\Software\Microsoft\Clipboard EnableClipboardHistory = 1.'
    Restart-ClipboardService
    if (-not (Wait-Enabled 15)) {
      # Windows Server may keep history off unless the policy allows it.
      $policy = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\System'
      if (-not (Test-Path $policy)) { $null = New-Item -Path $policy -Force }
      Set-ItemProperty -Path $policy -Name AllowClipboardHistory -Value 1 -Type DWord
      Write-Output 'Still off; set the policy AllowClipboardHistory = 1 too.'
      Restart-ClipboardService
      $null = Wait-Enabled $Seconds
    }
    Show-Keys
    Show-History (Get-History)
    if (-not (Test-Enabled)) { Fail 'enable: clipboard history is still off' }
  }

  'status' {
    Show-Session
    Show-Services
    Write-Output "Clipboard.IsHistoryEnabled(): $($Clipboard::IsHistoryEnabled())."
    Show-History (Get-History)
  }

  'put' {
    if (-not $Text) { Fail 'put: no -Text' }
    Set-Clipboard -Value $Text
    Write-Output "Put on the clipboard the ordinary way: $Text"
  }

  'clipboard-is' {
    if (-not $Text) { Fail 'clipboard-is: no -Text' }
    $now = Get-Clipboard -Raw
    if ($now -cne $Text) { Fail "clipboard-is: the clipboard holds '$now', not '$Text'" }
    Write-Output "The clipboard holds $Text."
  }

  'history-has' {
    if (-not $Text) { Fail 'history-has: no -Text' }
    $start = Get-Date
    $deadline = $start.AddSeconds($Seconds)
    while ($true) {
      $history = Get-History
      if ($history.Texts -ccontains $Text) {
        Write-Output ("In clipboard history after {0:N1} s: {1}" -f ((Get-Date) - $start).TotalSeconds, $Text)
        Show-History $history
        break
      }
      if ((Get-Date) -gt $deadline) {
        Show-History $history
        Fail "history-has: '$Text' is not in clipboard history after $Seconds s"
      }
      Start-Sleep -Milliseconds 250
    }
  }

  'history-lacks' {
    if (-not $Text) { Fail 'history-lacks: no -Text' }
    Start-Sleep -Seconds $Seconds
    $history = Get-History
    Show-History $history
    if ($history.Status -ne 'Success') { Fail "history-lacks: the history cannot be read (status $($history.Status)), so its absence proves nothing" }
    if ($history.Texts -ccontains $Text) { Fail "history-lacks: '$Text' is in clipboard history" }
    Write-Output "Not in clipboard history after $Seconds s: $Text"
  }
}
