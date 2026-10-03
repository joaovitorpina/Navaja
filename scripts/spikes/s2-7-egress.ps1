# Spike S2.7 (docs/spikes.md) on Windows: the modes of s2-7-egress.sh, which
# hands every mode to this script on Windows. See that script for the usage.
# Like it, this one is meant for a throwaway CI runner VM: it changes the
# audit policy and the event logs' settings, and needs an administrator.
#
# Windows has no built-in capture that names each packet's process, so the
# runner's own logs stand in for one:
# - "Filtering Platform Connection" auditing logs each connection the Windows
#   Filtering Platform allows (5156) or blocks (5157), TCP and UDP, loopback
#   included, and each bind (5158), with the process ID and the program;
# - "Process Creation" auditing (4688) logs each process started, with its
#   parent, so the process tree comes from the log, not from polling;
# - the DNS client's Operational log records each name a process asks the
#   DNS client for, with that process's ID.
# The process tree is every navaja.exe started in the phase and every process
# it starts, at any depth: WebView2's msedgewebview2.exe processes included.
# A connection from the tree to an address other than loopback is a finding,
# and so is a DNS query from it. So is a DNS query for one of the egress
# canary's hosts, whichever process asks.
param(
  [Parameter(Mandatory = $true, Position = 0)][string]$Mode,
  [Parameter(Position = 1)][string]$Phase = ''
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$Repo = (& git rev-parse --show-toplevel).Trim()
Set-Location $Repo
if ($env:S2_7_OUT) {
  $Out = $env:S2_7_OUT
} elseif ($env:RUNNER_TEMP) {
  $Out = Join-Path $env:RUNNER_TEMP 'navaja-s2-7'
} else {
  $Out = Join-Path ([IO.Path]::GetTempPath()) 'navaja-s2-7'
}
New-Item -ItemType Directory -Force -Path $Out | Out-Null

$Binary = Join-Path $Repo 'target\debug\navaja.exe'
# The harness's WebDriver port (wdio.conf.ts; @wdio/tauri-service's default).
$Port = 4445
$IdleSeconds = 300
if ($env:S2_7_IDLE_SECONDS) { $IdleSeconds = [int]$env:S2_7_IDLE_SECONDS }
# Part of every host the egress canary asks for (app/e2e/specs/egress.e2e.ts).
$Canary = 'navaja-canary'
# Audit subcategories, by GUID so that a localized Windows reads them too.
$AuditConnection = '{0CCE9226-69AE-11D9-BED3-505054503030}' # Filtering Platform Connection
$AuditProcess = '{0CCE922B-69AE-11D9-BED3-505054503030}'    # Process Creation
$DnsLog = 'Microsoft-Windows-DNS-Client/Operational'
$AuditKey = 'HKLM\Software\Microsoft\Windows\CurrentVersion\Policies\System\Audit'

function Fail([string]$Message) {
  Write-Host "::error::S2.7 $Message"
  exit 1
}

function Assert-Exit([string]$What) {
  if ($LASTEXITCODE -ne 0) { Fail "$What failed (exit $LASTEXITCODE)" }
}

# A property of an exported event, '' when the event has none.
function Get-Field($Object, [string]$Name) {
  $property = $Object.PSObject.Properties[$Name]
  if ($null -eq $property -or $null -eq $property.Value) { return '' }
  return [string]$property.Value
}

# A process ID as the logs write it: decimal, or hex with 0x (4688).
function ConvertTo-ProcessId([string]$Value) {
  if ($Value -match '^0x[0-9a-fA-F]+$') { return [Convert]::ToInt64($Value, 16) }
  if ($Value -match '^\d+$') { return [int64]$Value }
  return [int64]-1
}

function Test-Loopback([string]$Address) {
  return ($Address -eq '::1' -or $Address -like '127.*' -or $Address -like '::ffff:127.*')
}

function Save-Json([string]$Path, $Value) {
  ConvertTo-Json -InputObject @($Value) -Depth 4 | Set-Content -Path $Path -Encoding utf8
}

function Read-Json([string]$Path) {
  if (-not (Test-Path $Path)) { Fail "no $Path; run the phase first" }
  return @(Get-Content -Raw -Path $Path | ConvertFrom-Json)
}

function Test-PortOpen {
  $client = New-Object System.Net.Sockets.TcpClient
  try {
    $client.Connect('127.0.0.1', $Port)
    return $true
  } catch {
    return $false
  } finally {
    $client.Dispose()
  }
}

# --- Setup and teardown ----------------------------------------------------------

function Invoke-Setup {
  # Kept, so that teardown puts the policy back as it was.
  auditpol /backup "/file:$Out\auditpol-before.csv" | Out-Null
  Assert-Exit 'auditpol /backup'
  auditpol /set "/subcategory:$AuditConnection" /success:enable /failure:enable
  Assert-Exit 'auditpol (Filtering Platform Connection)'
  auditpol /set "/subcategory:$AuditProcess" /success:enable /failure:enable
  Assert-Exit 'auditpol (Process Creation)'
  # 4688 with the command line, which tells WebView2's process types apart.
  reg add $AuditKey /v ProcessCreationIncludeCmdLine_Enabled /t REG_DWORD /d 1 /f | Out-Null
  Assert-Exit 'reg add (command lines in 4688)'
  # Room for every event of a phase: a log that wraps loses the oldest ones,
  # and each phase checks that it did not.
  wevtutil sl Security /ms:1073741824
  Assert-Exit 'wevtutil (Security log size)'
  wevtutil sl $DnsLog /e:true /ms:268435456
  Assert-Exit 'wevtutil (DNS client log)'
  auditpol /get "/subcategory:$AuditConnection,$AuditProcess"
  Write-Host "DNS client log: $((Get-WinEvent -ListLog $DnsLog).IsEnabled)"
  $webview = Get-ItemProperty -ErrorAction SilentlyContinue `
    'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
  if ($webview) { Write-Host "WebView2 Runtime: $($webview.pv)" }
  Write-Host "Windows: $([Environment]::OSVersion.VersionString)"
}

function Invoke-Teardown {
  Get-Process -Name navaja -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -eq $Binary } |
    Stop-Process -Force -ErrorAction SilentlyContinue
  if (Test-Path "$Out\auditpol-before.csv") {
    auditpol /restore "/file:$Out\auditpol-before.csv" | Out-Null
  } else {
    auditpol /set "/subcategory:$AuditConnection" /success:disable /failure:disable | Out-Null
    auditpol /set "/subcategory:$AuditProcess" /success:disable /failure:disable | Out-Null
  }
  reg delete $AuditKey /v ProcessCreationIncludeCmdLine_Enabled /f 2>$null | Out-Null
  wevtutil sl $DnsLog /e:false
  auditpol /get "/subcategory:$AuditConnection,$AuditProcess"
  Write-Host 'Audit policy restored; DNS client log off.'
  # A native command's exit status must not become this script's.
  exit 0
}

# --- Exporting a phase's events ---------------------------------------------------

# Events as flat objects: time (UTC), ID, the process that logged it, and the
# named EventData fields.
function Read-Events([hashtable]$Filter) {
  $records = @(Get-WinEvent -FilterHashtable $Filter -ErrorAction SilentlyContinue)
  foreach ($record in $records) {
    $row = [ordered]@{
      Time = $record.TimeCreated.ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
      Id = $record.Id
      # The process that logged the event: for the DNS client, the one that
      # asked. Not "ProcessId": 4688 and 5156 have EventData fields of that
      # name (any case), with other meanings.
      LoggedBy = [int64]$record.ProcessId
    }
    $xml = [xml]$record.ToXml()
    foreach ($node in $xml.SelectNodes("//*[local-name()='EventData']/*[local-name()='Data']")) {
      $row[$node.GetAttribute('Name')] = $node.InnerText
    }
    [pscustomobject]$row
  }
}

# Writes <name>-security.json and <name>-dns.json: every event of the phase,
# from any process, so that a person can check what the analysis kept.
function Export-Phase([string]$Name, [datetime]$Start, [datetime]$End) {
  $oldest = Get-WinEvent -LogName Security -Oldest -MaxEvents 1
  if ($oldest.TimeCreated -gt $Start) {
    Fail "${Name}: the Security log starts after the phase did; it wrapped, so events are missing"
  }
  $security = @(Read-Events @{ LogName = 'Security'; Id = 4688, 5156, 5157, 5158, 5159; StartTime = $Start; EndTime = $End })
  $dns = @(Read-Events @{ LogName = $DnsLog; StartTime = $Start; EndTime = $End })
  $window = [ordered]@{
    Start = $Start.ToUniversalTime().ToString('o')
    End = $End.ToUniversalTime().ToString('o')
  }
  Save-Json "$Out\$Name-window.json" $window
  Save-Json "$Out\$Name-security.json" $security
  Save-Json "$Out\$Name-dns.json" $dns
  Write-Host ("${Name}: exported $($security.Count) security events (4688, 5156-5159) and " +
    "$($dns.Count) DNS client events, $($window.Start) to $($window.End).")
}

# --- Analysis ---------------------------------------------------------------------

# The tree, from 4688: each process whose image is one of $RootImages (and,
# if $RootPids is given, whose PID is in it), and each process started by one
# already in the tree, at any depth. Keyed by PID.
function Get-Tree($Security, [string[]]$RootImages, [int64[]]$RootPids) {
  $created = @(foreach ($entry in $Security) {
      if ($entry.Id -ne 4688) { continue }
      [pscustomobject]@{
        ProcessId = ConvertTo-ProcessId (Get-Field $entry 'NewProcessId')
        Parent = ConvertTo-ProcessId (Get-Field $entry 'ProcessId')
        Image = Get-Field $entry 'NewProcessName'
        CommandLine = Get-Field $entry 'CommandLine'
        Time = $entry.Time
      }
    })
  $tree = @{}
  foreach ($process in $created) {
    $image = Split-Path -Leaf $process.Image
    if ($RootImages -notcontains $image.ToLowerInvariant()) { continue }
    if ($RootPids -and $RootPids -notcontains $process.ProcessId) { continue }
    $tree[$process.ProcessId] = $process
  }
  do {
    $added = 0
    foreach ($process in $created) {
      if (-not $tree.ContainsKey($process.ProcessId) -and $tree.ContainsKey($process.Parent)) {
        $tree[$process.ProcessId] = $process
        $added++
      }
    }
  } while ($added -gt 0)
  return $tree
}

function Get-Protocol([string]$Number) {
  switch ($Number) {
    '6' { return 'TCP' }
    '17' { return 'UDP' }
    '1' { return 'ICMP' }
    '58' { return 'ICMPv6' }
    default { return "protocol $Number" }
  }
}

function Get-Direction([string]$Value) {
  switch ($Value) {
    '%%14592' { return 'in' }
    '%%14593' { return 'out' }
    default { return $Value }
  }
}

# What a phase's logs hold from the tree.
function Get-Report([string]$Name, [string[]]$RootImages, [string[]]$Images, [int64[]]$RootPids) {
  $security = Read-Json "$Out\$Name-security.json"
  $dns = Read-Json "$Out\$Name-dns.json"
  $tree = Get-Tree $security $RootImages $RootPids

  $connections = @(foreach ($entry in $security) {
      if ($entry.Id -ne 5156 -and $entry.Id -ne 5157) { continue }
      $processId = ConvertTo-ProcessId (Get-Field $entry 'ProcessID')
      $image = (Split-Path -Leaf (Get-Field $entry 'Application')).ToLowerInvariant()
      if (-not $tree.ContainsKey($processId) -and $Images -notcontains $image) { continue }
      [pscustomobject]@{
        Time = $entry.Time
        Event = $entry.Id
        Process = "$image ($processId)"
        Direction = Get-Direction (Get-Field $entry 'Direction')
        Local = "$(Get-Field $entry 'SourceAddress'):$(Get-Field $entry 'SourcePort')"
        Remote = Get-Field $entry 'DestAddress'
        Port = Get-Field $entry 'DestPort'
        Protocol = Get-Protocol (Get-Field $entry 'Protocol')
      }
    })
  $outside = @($connections | Where-Object { -not (Test-Loopback $_.Remote) })
  $loopback = @($connections | Where-Object { Test-Loopback $_.Remote })

  $binds = @(foreach ($entry in $security) {
      if ($entry.Id -ne 5158) { continue }
      $processId = ConvertTo-ProcessId (Get-Field $entry 'ProcessID')
      if (-not $tree.ContainsKey($processId)) { continue }
      $address = Get-Field $entry 'SourceAddress'
      if (Test-Loopback $address) { continue }
      "$((Split-Path -Leaf (Get-Field $entry 'Application')).ToLowerInvariant()) ($processId) " +
      "$(Get-Protocol (Get-Field $entry 'Protocol')) ${address}:$(Get-Field $entry 'SourcePort')"
    })

  $queries = @(foreach ($entry in $dns) {
      # Not $name: PowerShell's names ignore case, and $Name is the phase.
      $query = Get-Field $entry 'QueryName'
      $fromTree = $tree.ContainsKey([int64]$entry.LoggedBy)
      if (-not $fromTree -and $query -notlike "*$Canary*") { continue }
      $image = 'process'
      if ($fromTree) { $image = Split-Path -Leaf $tree[[int64]$entry.LoggedBy].Image }
      [pscustomobject]@{
        Time = $entry.Time
        Event = $entry.Id
        Process = "$image ($($entry.LoggedBy))"
        Name = $query
        Type = Get-Field $entry 'QueryType'
      }
    })

  return [pscustomobject]@{
    Name = $Name
    Tree = $tree
    SecurityEvents = $security.Count
    ConnectionEvents = @($security | Where-Object { $_.Id -eq 5156 -or $_.Id -eq 5157 }).Count
    DnsEvents = $dns.Count
    Connections = $connections
    Outside = $outside
    Loopback = $loopback
    Binds = $binds
    Queries = $queries
    Findings = $outside.Count + $queries.Count
  }
}

# The phase's table as Markdown, written to <name>-findings.md and printed.
function Write-Report($Report) {
  $label = $env:MATRIX_OS
  if (-not $label) { $label = 'windows' }
  $lines = New-Object System.Collections.Generic.List[string]
  $lines.Add("#### $($Report.Name): $label")
  $lines.Add('')
  $lines.Add('| Log | Events in the phase, all processes | From the process tree | Of those, to loopback | Findings |')
  $lines.Add('|---|---|---|---|---|')
  $lines.Add("| WFP connections (5156 allowed, 5157 blocked) | $($Report.ConnectionEvents) | $($Report.Connections.Count) | $($Report.Loopback.Count) | $($Report.Outside.Count) |")
  $lines.Add("| DNS client (the tree's queries, and the canary's names from any process) | $($Report.DnsEvents) | $($Report.Queries.Count) | - | $($Report.Queries.Count) |")
  $lines.Add('')
  $kinds = @($Report.Tree.Values | ForEach-Object {
      $image = Split-Path -Leaf $_.Image
      if ($_.CommandLine -match '--type=([a-z-]+)') { $image = "$image --type=$($Matches[1])" }
      if ($_.CommandLine -match '--utility-sub-type=([A-Za-z.]+)') { $image = "$image ($($Matches[1]))" }
      $image
    } | Group-Object | Sort-Object Name | ForEach-Object { "$($_.Name) x$($_.Count)" })
  $lines.Add("Processes in the tree, from 4688: $($Report.Tree.Count) ($($kinds -join '; ')).")
  $lines.Add('')
  $remotes = @($Report.Loopback | Group-Object { "$($_.Process) $($_.Direction) $($_.Protocol) $($_.Remote):$($_.Port)" } |
      Sort-Object Name | ForEach-Object { "``$($_.Name)`` x$($_.Count)" })
  if ($remotes.Count -eq 0) { $remotes = @('none') }
  $lines.Add("The tree's loopback connections, which never leave the machine: $($remotes -join ', ').")
  if ($Report.Binds.Count -gt 0) {
    $lines.Add('')
    $binds = @($Report.Binds | Group-Object | ForEach-Object { "``$($_.Name)`` x$($_.Count)" })
    $lines.Add("Binds by the tree outside loopback (5158; a bind sends nothing by itself): $($binds -join ', ').")
  }
  $lines.Add('')
  if ($Report.Findings -eq 0) {
    $lines.Add('No connection or DNS query from the process tree.')
  } else {
    if ($Report.Outside.Count -gt 0) {
      $lines.Add('| Process | Event | Direction | Destination | Port | Protocol | Events | First (UTC) |')
      $lines.Add('|---|---|---|---|---|---|---|---|')
      $Report.Outside | Group-Object Process, Event, Direction, Remote, Port, Protocol | ForEach-Object {
        $first = $_.Group[0]
        $lines.Add("| $($first.Process) | $($first.Event) | $($first.Direction) | $($first.Remote) | $($first.Port) | $($first.Protocol) | $($_.Count) | $($first.Time) |")
      }
      $lines.Add('')
    }
    if ($Report.Queries.Count -gt 0) {
      $lines.Add('| Process | DNS client event | Name | Type | Events | First (UTC) |')
      $lines.Add('|---|---|---|---|---|---|')
      $Report.Queries | Group-Object Process, Event, Name, Type | ForEach-Object {
        $first = $_.Group[0]
        $lines.Add("| $($first.Process) | $($first.Event) | $($first.Name) | $($first.Type) | $($_.Count) | $($first.Time) |")
      }
      $lines.Add('')
    }
  }
  $lines.Add('')
  $text = $lines -join "`n"
  Set-Content -Path "$Out\$($Report.Name)-findings.md" -Value $text -Encoding utf8
  Write-Host $text
}

# --- Control ----------------------------------------------------------------------

# A request the logs must see and attribute, through the same analysis as the
# phases: if they don't, empty phase logs would prove nothing.
function Invoke-Control {
  # A cold cache, so that curl's lookup reaches the DNS client's network path.
  Clear-DnsClientCache
  $start = Get-Date
  Start-Sleep -Milliseconds 500
  $curl = Join-Path $env:SystemRoot 'System32\curl.exe'
  $proc = Start-Process -FilePath $curl -NoNewWindow -PassThru -Wait `
    -ArgumentList @('-sS', '-o', 'NUL', '-w', '%{http_code}', '--max-time', '30', 'https://github.com') `
    -RedirectStandardOutput "$Out\control-curl.txt"
  $code = (Get-Content -Raw "$Out\control-curl.txt").Trim()
  Write-Host "curl.exe (PID $($proc.Id)) https://github.com: HTTP $code, exit $($proc.ExitCode)"
  if ($proc.ExitCode -ne 0) { Fail 'control: curl https://github.com failed' }
  # Events reach the logs a moment after the fact.
  Start-Sleep -Seconds 3
  Export-Phase 'control' $start (Get-Date)
  $report = Get-Report 'control' @('curl.exe') @() @([int64]$proc.Id)
  Write-Report $report
  $tcp = @($report.Outside | Where-Object { $_.Port -eq '443' -and $_.Protocol -eq 'TCP' -and $_.Process -like "curl.exe ($($proc.Id))" })
  if ($tcp.Count -eq 0) {
    Fail "control: no WFP connection event for curl.exe ($($proc.Id)) to port 443; the logs can't be trusted"
  }
  $lookups = @($report.Queries | Where-Object { $_.Name -like '*github.com*' -and $_.Process -like "* ($($proc.Id))" })
  if ($lookups.Count -eq 0) {
    Fail "control: no DNS client event from curl.exe ($($proc.Id)) for github.com; a lookup by the tree would go unseen, so the logs can't be trusted"
  }
  Write-Host 'Control: the logs attributed curl''s connection and its DNS query to curl.exe.'
}

# --- Phases -----------------------------------------------------------------------

Add-Type -Namespace S27 -Name Win32 -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
[DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hWnd);
'@

function Assert-WindowShown([int]$ProcessId, [string]$When) {
  $process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
  if (-not $process) { throw "the app is gone ($When)" }
  $process.Refresh()
  $handle = $process.MainWindowHandle
  if ($handle -eq [IntPtr]::Zero) { throw "the app has no visible top-level window ($When)" }
  $visible = [S27.Win32]::IsWindowVisible($handle)
  $minimized = [S27.Win32]::IsIconic($handle)
  Write-Host "Window ($When): handle $handle, title '$($process.MainWindowTitle)', visible $visible, minimized $minimized"
  if (-not $visible -or $minimized) { throw "the app's window is not shown ($When)" }
}

# The live processes under $Root, from WMI.
function Get-LiveTree([int64]$Root) {
  $all = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name, CommandLine)
  $tree = @{ $Root = $true }
  do {
    $added = 0
    foreach ($process in $all) {
      if (-not $tree.ContainsKey([int64]$process.ProcessId) -and $tree.ContainsKey([int64]$process.ParentProcessId)) {
        $tree[[int64]$process.ProcessId] = $true
        $added++
      }
    }
  } while ($added -gt 0)
  return @($all | Where-Object { $tree.ContainsKey([int64]$_.ProcessId) })
}

function Save-Screenshot([string]$Path) {
  try {
    Add-Type -AssemblyName System.Windows.Forms, System.Drawing
    $bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $bitmap = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
    $bitmap.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    $graphics.Dispose()
    $bitmap.Dispose()
  } catch {
    Write-Host "::warning::S2.7 no screenshot: $_"
  }
}

# Waits up to 30 s for the given processes to end.
function Wait-Gone([int64[]]$ProcessIds) {
  for ($i = 0; $i -lt 60; $i++) {
    $left = @($ProcessIds | Where-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue })
    if ($left.Count -eq 0) { return }
    Start-Sleep -Milliseconds 500
  }
  Write-Host "::warning::S2.7 still running 30 s after the run, now killed: $($left -join ', ')"
  $left | ForEach-Object { Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue }
}

# Whether the app's WebDriver server reports ready, as the harness asks it.
function Test-WebDriverReady {
  try {
    $status = Invoke-RestMethod -Uri "http://127.0.0.1:$Port/status" -TimeoutSec 5
    return ($status.value.ready -eq $true)
  } catch {
    return $false
  }
}

# The idle phase: the app starts as @wdio/tauri-service starts it, runs for
# $IdleSeconds with its window shown and nothing driving it, and is stopped.
function Invoke-Idle {
  if (-not (Test-Path $Binary)) { Fail "idle: no $Binary; build it first (see s2-7-egress.sh)" }
  if (Test-PortOpen) { Fail "idle: 127.0.0.1:$Port already accepts connections; stop whatever holds it" }
  $appDir = Join-Path ([IO.Path]::GetTempPath()) ('navaja-s2-7-' + [guid]::NewGuid())
  New-Item -ItemType Directory -Path $appDir | Out-Null
  $start = Get-Date
  Start-Sleep -Milliseconds 500
  # The harness's launch (wdio.conf.ts, @wdio/tauri-service's embedded
  # driver): --tool uuid, a fresh app directory, and its WebDriver server.
  $env:NAVAJA_APP_DIR = $appDir
  $env:TAURI_WEBDRIVER_PORT = "$Port"
  $env:WDIO_EMBEDDED_SERVER = 'true'
  try {
    $app = Start-Process -FilePath $Binary -ArgumentList '--tool', 'uuid' -NoNewWindow -PassThru `
      -RedirectStandardOutput "$Out\idle-app.out.log" -RedirectStandardError "$Out\idle-app.err.log"
  } finally {
    Remove-Item Env:NAVAJA_APP_DIR, Env:TAURI_WEBDRIVER_PORT, Env:WDIO_EMBEDDED_SERVER
  }
  Write-Host "The app started (PID $($app.Id))."
  $failure = $null
  $tree = @()
  try {
    # Like the harness, wait for the WebDriver server to report ready.
    $deadline = (Get-Date).AddSeconds(60)
    while (-not (Test-WebDriverReady)) {
      if ($app.HasExited) { throw 'the app exited during startup' }
      if ((Get-Date) -gt $deadline) { throw 'the WebDriver server did not report ready within 60 s' }
      Start-Sleep -Milliseconds 500
    }
    $ready = Get-Date
    Write-Host "WebDriver ready; idling for $IdleSeconds s with nothing driving the app."
    # The window shows once the front end has rendered (shell_ready).
    Start-Sleep -Seconds 5
    Assert-WindowShown $app.Id 'start'
    $tree = @(Get-LiveTree $app.Id)
    $tree | Format-Table -AutoSize ProcessId, ParentProcessId, Name, @{ n = 'Type'; e = { if ($_.CommandLine -match '--type=([a-z-]+)') { $Matches[1] } } } |
      Out-String -Width 200 | Tee-Object -FilePath "$Out\idle-tree.txt" | Write-Host
    while (((Get-Date) - $ready).TotalSeconds -lt $IdleSeconds) {
      if ($app.HasExited) { throw "the app exited after $([int]((Get-Date) - $ready).TotalSeconds) s" }
      Start-Sleep -Seconds 10
    }
    if ($app.HasExited) { throw 'the app exited' }
    Assert-WindowShown $app.Id 'end'
    Save-Screenshot "$Out\idle-screen.png"
    Write-Host "The app ran $([int]((Get-Date) - $ready).TotalSeconds) s after WebDriver was ready, its window shown."
  } catch {
    $failure = "$_"
  } finally {
    $ids = @($tree | ForEach-Object { [int64]$_.ProcessId })
    $ids += @(Get-LiveTree $app.Id | ForEach-Object { [int64]$_.ProcessId })
    & taskkill /PID $app.Id /T /F | Out-Host
    Wait-Gone ($ids | Sort-Object -Unique)
  }
  Write-Host 'Its output:'
  Get-Content "$Out\idle-app.out.log", "$Out\idle-app.err.log" -ErrorAction SilentlyContinue | ForEach-Object { "  | $_" } | Write-Host
  # A moment for anything still in flight, and for the logs.
  Start-Sleep -Seconds 3
  Export-Phase 'idle' $start (Get-Date)
  if ($failure) { Fail "idle: $failure" }
  Write-Host 'Idle: done; the logs are checked in the next step.'
}

function Invoke-InUse {
  if (-not (Test-Path $Binary)) { Fail "in use: no $Binary; build it first (see s2-7-egress.sh)" }
  $start = Get-Date
  Start-Sleep -Milliseconds 500
  & pnpm e2e
  $status = $LASTEXITCODE
  # Whatever the suite started that is still running, the app or WebView2.
  $left = @(Get-CimInstance Win32_Process | Where-Object {
      ($_.Name -eq 'navaja.exe' -and $_.ExecutablePath -eq $Binary) -or
      ($_.Name -eq 'msedgewebview2.exe' -and $_.CreationDate -gt $start)
    } | ForEach-Object { [int64]$_.ProcessId })
  if ($left.Count -gt 0) { Wait-Gone $left }
  Start-Sleep -Seconds 3
  Export-Phase 'in-use' $start (Get-Date)
  if ($status -ne 0) { Fail "in use: the end-to-end suite failed (exit $status)" }
  Write-Host 'In use: the suite passed; the logs are checked in the next step.'
}

function Invoke-Check([string]$Name) {
  $report = Get-Report $Name @('navaja.exe') @('navaja.exe', 'msedgewebview2.exe') @()
  Write-Report $report
  # The app's own WebDriver connections show that the logs saw it.
  if (@($report.Loopback | Where-Object { $_.Process -like 'navaja.exe *' }).Count -eq 0) {
    Fail "${Name}: the logs hold no connection of navaja.exe, not even its WebDriver on loopback, so they prove nothing"
  }
  if ($report.Findings -gt 0) {
    Fail "${Name}: $($report.Findings) connection or DNS event(s) from the process tree; see the tables above"
  }
  Write-Host "${Name}: no connection or DNS query from the process tree."
}

switch ($Mode) {
  'setup' { Invoke-Setup }
  'control' { Invoke-Control }
  'idle' { Invoke-Idle }
  'in-use' { Invoke-InUse }
  'check' {
    if ($Phase -ne 'idle' -and $Phase -ne 'in-use') { Write-Host 'usage: s2-7-egress.ps1 check idle|in-use'; exit 2 }
    Invoke-Check $Phase
  }
  'teardown' { Invoke-Teardown }
  default {
    Write-Host 'usage: s2-7-egress.ps1 setup|control|idle|check idle|in-use|check in-use|teardown'
    exit 2
  }
}
exit 0
