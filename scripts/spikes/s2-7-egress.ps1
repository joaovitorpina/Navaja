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
#   DNS client for, with that process's ID;
# - the BITS client's Operational log records each download job, with the
#   process that created it and its URLs: BITS downloads for that process
#   from its own service.
# The process tree is every navaja.exe started in the phase and every process
# it starts, at any depth: WebView2's msedgewebview2.exe processes included.
# A connection from the tree to an address other than loopback is a finding,
# and so are a DNS query from it and a BITS job it created. So is a DNS query
# for one of the egress canary's hosts, whichever process asks.
#
# Two services seem to act for the tree without any log naming it: WAM's
# account broker and its sign-in service ($WokenServices). Their connections
# in the 15 s after a start of navaja.exe or of its WebView2 browser process
# count as the app's; those an entry of s2-7-disclosed.tsv covers are
# reported as disclosed and fail nothing (docs/adr/0003-webview-network.md).
# An entry covers a connection only through the names its own process asked
# the DNS client for, before it, in lookups that gave its address
# (Find-DisclosedEntry).
#
# Other services may act for the tree too, so a baseline, 5 min without the
# app, gives each process outside the tree an identity (its image, plus the
# services it hosts or the COM server it is), and each later phase lists the
# connections from identities that made none in the baseline, and those in
# the 15 s after each of those starts from any identity that does not poll
# all through the baseline. That list is for a person to review, and
# fails nothing: the runner's own scheduled tasks and services come and go.
#
# pktmon, built into Windows, also captures the NICs' packets for each phase
# (cut to 512 bytes). It names no process, but its DNS packets
# (s2-7-dns.mjs) name the hosts behind the findings: WebView2's network
# service sends DNS queries itself, which the DNS client's log never sees.
# The copy kept for the artifact holds only the headers of other packets.
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
# The images of the tree, and the one it starts from.
$TreeImages = @('navaja.exe', 'msedgewebview2.exe')
$RootImages = @('navaja.exe')
# How long after each start of navaja.exe, and of the tree's WebView2
# browser process, the review list also takes connections from identities
# the baseline has, and connections from the services in $WokenServices
# count as the app's (see Get-Report).
$StartupSeconds = 15
# Services that the webview wakes outside its tree, by identity (see
# Format-Identity): WAM's Microsoft-account provider and the sign-in service,
# which looked login.live.com up 1.2 to 2.3 s after WebView2's browser
# process started, in every S2.7 run checked, though no log names them as
# acting for it. Their connections in the $StartupSeconds after those
# starts count as the app's.
$WokenServices = @('svchost.exe [wlidsvc]', 'backgroundtaskhost.exe [BackgroundTaskHost.WebAccountProvider]')
# What the check reports as disclosed instead of failing on: traffic the app
# sets off that nothing it controls can stop (docs/adr/0003-webview-network.md).
$DisclosedList = Join-Path $Repo 'scripts/spikes/s2-7-disclosed.tsv'
# Audit subcategories, by GUID so that a localized Windows reads them too.
$AuditConnection = '{0CCE9226-69AE-11D9-BED3-505054503030}' # Filtering Platform Connection
$AuditProcess = '{0CCE922B-69AE-11D9-BED3-505054503030}'    # Process Creation
$DnsLog = 'Microsoft-Windows-DNS-Client/Operational'
$BitsLog = 'Microsoft-Windows-Bits-Client/Operational'
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

# The port of an "address:port" pair (IPv6 addresses hold colons too).
function Get-Port([string]$Endpoint) {
  return $Endpoint.Substring($Endpoint.LastIndexOf(':') + 1)
}

function ConvertTo-Time($Time) {
  if ($Time -is [datetime]) { return $Time }
  return [datetime]::Parse([string]$Time, $null, [Globalization.DateTimeStyles]::RoundtripKind)
}

# Seconds from the phase's start, as "+12.3 s". ConvertFrom-Json may have
# turned the ISO time into a DateTime already.
function Format-Offset($Time, [datetime]$Start) {
  $at = ConvertTo-Time $Time
  return ('+{0:0.0} s' -f ($at.ToUniversalTime() - $Start.ToUniversalTime()).TotalSeconds)
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
  wevtutil sl $BitsLog /e:true /ms:67108864
  Assert-Exit 'wevtutil (BITS client log)'
  auditpol /get "/subcategory:$AuditConnection,$AuditProcess"
  Write-Host "DNS client log: $((Get-WinEvent -ListLog $DnsLog).IsEnabled); BITS client log: $((Get-WinEvent -ListLog $BitsLog).IsEnabled)"
  $webview = Get-ItemProperty -ErrorAction SilentlyContinue `
    'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
  if ($webview) { Write-Host "WebView2 Runtime: $($webview.pv)" }
  Write-Host "Windows: $([Environment]::OSVersion.VersionString)"
}

function Invoke-Teardown {
  pktmon stop 2>&1 | Out-Null
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
  Hide-CommandLines
  # A native command's exit status must not become this script's.
  exit 0
}

# The exported 4688 events keep a command line only for the tree's images,
# and the service or COM server name of the others: the rest belongs to the
# runner, and the artifact can be downloaded by anyone who can see the run.
function Hide-CommandLines {
  foreach ($file in @(Get-ChildItem -Path $Out -Filter '*-security.json' -ErrorAction SilentlyContinue)) {
    $events = @(Read-Json $file.FullName)
    $hidden = 0
    foreach ($entry in $events) {
      if ($entry.Id -ne 4688) { continue }
      $image = (Split-Path -Leaf (Get-Field $entry 'NewProcessName')).ToLowerInvariant()
      if ($TreeImages -contains $image) { continue }
      $line = Get-Field $entry 'CommandLine'
      $kept = ''
      if ($line -match '\s-s\s+(\S+)') { $kept = " -s $($Matches[1])" }
      elseif ($line -match '-ServerName:(\S+)') { $kept = " -ServerName:$($Matches[1])" }
      $entry | Add-Member -Force -NotePropertyName CommandLine -NotePropertyValue "(hidden)$kept"
      $hidden++
    }
    Save-Json $file.FullName $events
    Write-Host "$($file.Name): command lines of $hidden processes outside the tree hidden for the artifact."
  }
}

# --- Exporting a phase's events ---------------------------------------------------

# Events as flat objects: time (UTC), ID, the process that logged it, and the
# named EventData fields. A log that can't be read fails the run; only "no
# events" is empty.
function Read-Events([hashtable]$Filter) {
  try {
    $records = @(Get-WinEvent -FilterHashtable $Filter -ErrorAction Stop)
  } catch {
    if ($_.FullyQualifiedErrorId -like 'NoMatchingEventsFound*') { return }
    Fail "reading the $($Filter.LogName) log failed: $_"
  }
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
      # A field named like one above keeps a "Data" prefix: BITS events 59
      # to 61 name their job's GUID "Id".
      $field = $node.GetAttribute('Name')
      if ($row.Contains($field)) { $field = "Data$field" }
      $row[$field] = $node.InnerText
    }
    [pscustomobject]$row
  }
}

# The record number of a log's newest or oldest event, 0 when it is empty.
# Record numbers only grow, so they show whether a log lost events.
function Get-RecordId([string]$Log, [switch]$Oldest) {
  try {
    $event = Get-WinEvent -LogName $Log -MaxEvents 1 -Oldest:$Oldest -ErrorAction Stop
  } catch {
    if ($_.FullyQualifiedErrorId -like 'NoMatchingEventsFound*') { return [int64]0 }
    Fail "reading the $Log log failed: $_"
  }
  return [int64]$event.RecordId
}

# The newest record of each log, just before a phase starts.
function Get-LogMarks {
  return @{
    Security = Get-RecordId 'Security'
    $DnsLog = Get-RecordId $DnsLog
    $BitsLog = Get-RecordId $BitsLog
  }
}

# Fails if the log no longer holds the first record written after the phase
# started: it wrapped, so events of the phase are missing.
function Assert-LogCovers([string]$Name, [string]$Log, $Marks) {
  $oldest = Get-RecordId $Log -Oldest
  if ($oldest -gt $Marks[$Log] + 1) {
    Fail "${Name}: the $Log log no longer holds record $($Marks[$Log] + 1), the first one after the phase started (its oldest is $oldest); it wrapped, so events are missing"
  }
}

# A lookup of a name of this script's own at the end of each phase. The DNS
# client's log must hold it, or it stopped recording during the phase, and
# the tree's queries could be missing for that reason alone.
function Send-DnsMarker([string]$Name) {
  $marker = "s27-end-of-$Name.example.com"
  try { [System.Net.Dns]::GetHostAddresses($marker) | Out-Null } catch { }
  return $marker
}

# The processes running now, with the services each one hosts, into
# <name>-processes-<when>.json: the identities of processes outside the tree.
function Save-Snapshot([string]$Name, [string]$When) {
  $services = @{}
  foreach ($service in @(Get-CimInstance Win32_Service | Where-Object { $_.ProcessId -gt 0 })) {
    $key = [int64]$service.ProcessId
    if (-not $services.ContainsKey($key)) { $services[$key] = @() }
    $services[$key] += $service.Name
  }
  $list = @(Get-CimInstance Win32_Process | ForEach-Object {
      $key = [int64]$_.ProcessId
      $hosted = ''
      if ($services.ContainsKey($key)) { $hosted = (@($services[$key] | Sort-Object) -join ',') }
      [pscustomobject]@{ ProcessId = $key; Name = $_.Name; Services = $hosted }
    })
  Save-Json "$Out\$Name-processes-$When.json" $list
}

# pktmon on every NIC, for one phase. A capture left by an earlier step is
# stopped first; packet filters are cleared, so every packet is kept.
function Start-Capture([string]$Name) {
  pktmon stop 2>&1 | Out-Null
  pktmon filter remove 2>&1 | Out-Null
  pktmon start --capture --comp nics --pkt-size 512 --file-size 1024 --file-name "$Out\$Name.etl" | Out-Host
  Assert-Exit 'pktmon start'
}

# Stops pktmon, converts its log to a pcapng, and writes the DNS messages in
# it to <name>-dns-packets.json. <name>.pcapng keeps only the headers of the
# other packets.
function Stop-Capture([string]$Name) {
  pktmon stop | Out-Host
  Assert-Exit 'pktmon stop'
  pktmon etl2pcap "$Out\$Name.etl" --out "$Out\$Name.full.pcapng" | Out-Host
  Assert-Exit 'pktmon etl2pcap'
  Remove-Item "$Out\$Name.etl" -ErrorAction SilentlyContinue
  & node scripts/spikes/s2-7-dns.mjs "$Out\$Name.full.pcapng" --strip "$Out\$Name.pcapng" |
    Set-Content -Path "$Out\$Name-dns-packets.json" -Encoding utf8
  Assert-Exit 's2-7-dns.mjs'
  $full = (Get-Item "$Out\$Name.full.pcapng").Length
  Remove-Item "$Out\$Name.full.pcapng"
  $messages = @(Read-Json "$Out\$Name-dns-packets.json")
  Write-Host ("${Name}: pktmon captured $full bytes; $($messages.Count) DNS messages over UDP in them. " +
    "The copy kept, headers only but for DNS: $((Get-Item "$Out\$Name.pcapng").Length) bytes.")
}

# Writes <name>-security.json, -dns.json and -bits.json: every event of the
# phase, from any process, so that a person can check what the analysis kept.
function Export-Phase([string]$Name, $Marks, [datetime]$Start, [datetime]$End, [string]$Marker) {
  Assert-LogCovers $Name 'Security' $Marks
  Assert-LogCovers $Name $DnsLog $Marks
  Assert-LogCovers $Name $BitsLog $Marks
  $security = @(Read-Events @{ LogName = 'Security'; Id = 4688, 5156, 5157, 5158, 5159; StartTime = $Start; EndTime = $End })
  $dns = @(Read-Events @{ LogName = $DnsLog; StartTime = $Start; EndTime = $End })
  $bits = @(Read-Events @{ LogName = $BitsLog; StartTime = $Start; EndTime = $End })
  $window = [ordered]@{
    Start = $Start.ToUniversalTime().ToString('o')
    End = $End.ToUniversalTime().ToString('o')
    Marker = $Marker
  }
  Save-Json "$Out\$Name-window.json" $window
  Save-Json "$Out\$Name-security.json" $security
  Save-Json "$Out\$Name-dns.json" $dns
  Save-Json "$Out\$Name-bits.json" $bits
  Write-Host ("${Name}: exported $($security.Count) security events (4688, 5156-5159), " +
    "$($dns.Count) DNS client events and $($bits.Count) BITS client events, $($window.Start) to $($window.End).")
  if ($dns.Count -eq 0) {
    Fail "${Name}: the DNS client log holds no event from the phase, though the runner looks names up all the time; it is not recording"
  }
  if (@($dns | Where-Object { (Get-Field $_ 'QueryName') -like "$Marker*" }).Count -eq 0) {
    Fail "${Name}: the DNS client log does not hold this script's lookup of $Marker at the phase's end; it stopped recording, so the tree's queries could be missing"
  }
}

# --- Analysis ---------------------------------------------------------------------

# The processes started in the phase, from 4688.
function Get-Created($Security) {
  return @(foreach ($entry in $Security) {
      if ($entry.Id -ne 4688) { continue }
      [pscustomobject]@{
        ProcessId = ConvertTo-ProcessId (Get-Field $entry 'NewProcessId')
        Parent = ConvertTo-ProcessId (Get-Field $entry 'ProcessId')
        ParentImage = Split-Path -Leaf (Get-Field $entry 'ParentProcessName')
        Image = Get-Field $entry 'NewProcessName'
        CommandLine = Get-Field $entry 'CommandLine'
        Time = $entry.Time
      }
    })
}

# The tree, from 4688: each process whose image is one of $Roots (and,
# if $RootPids is given, whose PID is in it), and each process started by one
# already in the tree, at any depth, and not before it: Windows reuses PIDs
# (see Get-LiveTree). Keyed by PID.
function Get-Tree($Created, [string[]]$Roots, [int64[]]$RootPids) {
  $tree = @{}
  foreach ($process in $Created) {
    $image = Split-Path -Leaf $process.Image
    if ($Roots -notcontains $image.ToLowerInvariant()) { continue }
    if ($RootPids -and $RootPids -notcontains $process.ProcessId) { continue }
    $tree[$process.ProcessId] = $process
  }
  do {
    $added = 0
    foreach ($process in $Created) {
      if ($tree.ContainsKey($process.ProcessId) -or -not $tree.ContainsKey($process.Parent)) { continue }
      if ((ConvertTo-Time $process.Time) -lt (ConvertTo-Time $tree[$process.Parent].Time)) { continue }
      $tree[$process.ProcessId] = $process
      $added++
    }
  } while ($added -gt 0)
  return $tree
}

# The name a process outside the tree keeps across phases: its image, plus
# the services it hosts or the COM server it is ("svchost.exe [BITS]",
# "backgroundtaskhost.exe [BackgroundTaskHost.WebAccountProvider]").
function Format-Identity([string]$Image, [string]$Services, [string]$CommandLine) {
  $name = [IO.Path]::GetFileName($Image).ToLowerInvariant()
  if ($Services) { return "$name [$Services]" }
  if ($CommandLine -match '\s-s\s+(\S+)') { return "$name [$($Matches[1])]" }
  if ($CommandLine -match '-ServerName:(\S+)') { return "$name [$($Matches[1])]" }
  return $name
}

# PID -> identity, from the phase's snapshots and its 4688 events: the
# snapshot at the start, the processes started since, the snapshot at the
# end, each overriding the one before (a PID can be reused).
function Get-Identities([string]$Name, $Created) {
  $identity = @{}
  foreach ($when in @('start', 'end')) {
    $path = "$Out\$Name-processes-$when.json"
    if (Test-Path $path) {
      foreach ($process in (Read-Json $path)) {
        $identity[[int64]$process.ProcessId] = Format-Identity $process.Name $process.Services ''
      }
    }
    if ($when -eq 'start') {
      foreach ($process in $Created) {
        $identity[$process.ProcessId] = Format-Identity $process.Image '' $process.CommandLine
      }
    }
  }
  return $identity
}

# The BITS jobs that the tree created (event 3, or 16403 for a file added
# to a job), plus $KnownJobs, the tree's jobs from earlier phases, with what
# BITS did for each: 59 started, 60 stopped and 61 failed a transfer (their
# job's GUID is "DataId", see Read-Events), 4 completed the job, 5
# cancelled it.
function Get-BitsJobs($Bits, $Tree, [string[]]$KnownJobs) {
  $jobs = [ordered]@{}
  foreach ($entry in $Bits) {
    if ($entry.Id -ne 3 -and $entry.Id -ne 16403) { continue }
    $creator = ConvertTo-ProcessId (Get-Field $entry 'processId')
    if (-not $Tree.ContainsKey($creator)) { continue }
    $job = (Get-Field $entry 'jobId').ToLowerInvariant()
    if (-not $jobs.Contains($job)) {
      $jobs[$job] = [pscustomobject]@{
        Job = $job
        Title = Get-Field $entry 'jobTitle'
        Creator = "$((Split-Path -Leaf $Tree[$creator].Image).ToLowerInvariant()) ($creator)"
        Time = $entry.Time
        Urls = @()
        Transferred = [int64]0
        Events = 0
      }
    }
    $remote = Get-Field $entry 'RemoteName'
    if ($remote -and $jobs[$job].Urls -notcontains $remote) { $jobs[$job].Urls += $remote }
  }
  foreach ($job in $KnownJobs) {
    if (-not $job -or $jobs.Contains($job)) { continue }
    $jobs[$job] = [pscustomobject]@{
      Job = $job; Title = ''; Creator = 'the tree, in an earlier phase'; Time = $null
      Urls = @(); Transferred = [int64]0; Events = 0
    }
  }
  foreach ($entry in $Bits) {
    if (@(4, 5, 59, 60, 61) -notcontains $entry.Id) { continue }
    $job = Get-Field $entry 'DataId'
    if (-not $job) { $job = Get-Field $entry 'jobId' }
    $job = $job.ToLowerInvariant()
    if (-not $jobs.Contains($job)) { continue }
    $record = $jobs[$job]
    $record.Events++
    if (-not $record.Title) { $record.Title = Get-Field $entry 'name' }
    if (-not $record.Title) { $record.Title = Get-Field $entry 'jobTitle' }
    if ($null -eq $record.Time) { $record.Time = $entry.Time }
    $url = Get-Field $entry 'url'
    if ($url -and $record.Urls -notcontains $url) { $record.Urls += $url }
    $bytes = Get-Field $entry 'bytesTransferred'
    if ($bytes -match '^\d+$' -and [int64]$bytes -gt $record.Transferred) { $record.Transferred = [int64]$bytes }
  }
  # A job only known from an earlier phase counts here if BITS worked on it.
  return @($jobs.Values | Where-Object { $_.Creator -ne 'the tree, in an earlier phase' -or $_.Events -gt 0 })
}

# The disclosed list's entries for Windows and the phase: service, hosts and
# purpose, from scripts/spikes/s2-7-disclosed.tsv (its header says how).
function Get-DisclosedEntries([string]$Phase) {
  if (-not (Test-Path $DisclosedList)) { Fail "no disclosed list at $DisclosedList" }
  $number = 0
  return @(foreach ($line in (Get-Content -Path $DisclosedList -Encoding utf8)) {
      $number++
      if ($line -match '^\s*(#|$)') { continue }
      $fields = $line -split "`t"
      if ($fields.Count -ne 5) { Fail "s2-7-disclosed.tsv line ${number}: $($fields.Count) fields, not 5" }
      if ($fields[0] -ne 'windows' -or @($fields[1] -split ' ') -notcontains $Phase) { continue }
      [pscustomobject]@{
        Service = $fields[2]
        Hosts = @($fields[3] -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ })
        Label = "$($fields[2]) -> $($fields[3])"
      }
    })
}

# The entry that covers a connection: its service is the connection's
# identity, and every name the connection is tied to (Asked: what its process
# asked the DNS client for, before it, in lookups that gave its address) is
# one of the entry's hosts. A connection tied to no name is covered by none.
# Not the names pktmon's DNS packets give for the address: those come from
# any process, and one address can answer for many names, as Apple's edge
# servers do in the macOS captures.
function Find-DisclosedEntry($Entries, $Connection) {
  $asked = @($Connection.Asked -split ', ' | Where-Object { $_ })
  if ($asked.Count -eq 0) { return $null }
  foreach ($entry in $Entries) {
    if ($entry.Service -ne $Connection.Identity) { continue }
    if (@($asked | Where-Object { $entry.Hosts -notcontains $_ }).Count -eq 0) { return $entry }
  }
  return $null
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

# What a phase's logs hold from the tree, and, given the baseline's
# identities, what they hold from other processes that the baseline lacks.
function Get-Report([string]$Name, [string[]]$Roots, [string[]]$Images, [int64[]]$RootPids,
  [string[]]$KnownJobs = @(), $BaselineIdentities = $null, [string[]]$Steady = @()) {
  $security = @(Read-Json "$Out\$Name-security.json")
  $dns = @(Read-Json "$Out\$Name-dns.json")
  $bits = @(Read-Json "$Out\$Name-bits.json")
  $created = Get-Created $security
  $tree = Get-Tree $created $Roots $RootPids
  $identities = Get-Identities $Name $created

  $connections = @()
  $others = @()
  foreach ($entry in $security) {
    if ($entry.Id -ne 5156 -and $entry.Id -ne 5157) { continue }
    $processId = ConvertTo-ProcessId (Get-Field $entry 'ProcessID')
    $image = (Split-Path -Leaf (Get-Field $entry 'Application')).ToLowerInvariant()
    $connection = [pscustomobject]@{
      Time = $entry.Time
      Event = $entry.Id
      ProcessId = $processId
      Process = "$image ($processId)"
      Identity = $image
      Direction = Get-Direction (Get-Field $entry 'Direction')
      Local = "$(Get-Field $entry 'SourceAddress'):$(Get-Field $entry 'SourcePort')"
      Remote = Get-Field $entry 'DestAddress'
      Port = Get-Field $entry 'DestPort'
      Protocol = Get-Protocol (Get-Field $entry 'Protocol')
      Names = ''
      Asked = ''
    }
    if ($tree.ContainsKey($processId) -or $Images -contains $image) {
      $connections += $connection
    } elseif (-not (Test-Loopback $connection.Remote)) {
      if ($identities.ContainsKey($processId)) { $connection.Identity = $identities[$processId] }
      $others += $connection
    }
  }
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

  $jobs = @(Get-BitsJobs $bits $tree $KnownJobs)

  # Names from the capture: what each local port asked (a query's source
  # port), and what each address answered for (a response's A or AAAA).
  $asked = @{}
  $answered = @{}
  $packetsPath = "$Out\$Name-dns-packets.json"
  if (Test-Path $packetsPath) {
    foreach ($message in (Read-Json $packetsPath)) {
      foreach ($question in @($message.questions)) {
        if (-not $message.response) {
          $key = "$($message.src):$($message.sport)"
          if (-not $asked.ContainsKey($key)) { $asked[$key] = @() }
          $asked[$key] += "$($question.name) $($question.type)"
        }
      }
      if ($message.response) {
        foreach ($answer in @($message.answers)) {
          if ($answer.type -ne 'A' -and $answer.type -ne 'AAAA') { continue }
          if (-not $answered.ContainsKey($answer.data)) { $answered[$answer.data] = @() }
          $answered[$answer.data] += @($message.questions | ForEach-Object { $_.name })
        }
      }
    }
  }
  foreach ($connection in @($outside + $others)) {
    $names = @()
    if ($connection.Port -eq '53') {
      $key = $connection.Local
      if ($asked.ContainsKey($key)) { $names = $asked[$key] }
    } elseif ($answered.ContainsKey($connection.Remote)) {
      $names = $answered[$connection.Remote]
    }
    $connection.Names = @($names | Sort-Object -Unique) -join ', '
  }

  # The services the webview wakes ($WokenServices): their connections in
  # the $StartupSeconds after a start of navaja.exe or of the tree's WebView2
  # browser process (msedgewebview2.exe without --type=) count as the app's,
  # unless an entry of the disclosed list covers them. The browser process
  # counts because WAM follows it: in run 37168559527 it started 14.4 s after
  # navaja.exe, and the sign-in service looked login.live.com up 2.1 s later,
  # outside a window that opened with navaja.exe alone.
  $appStarts = @($created | Where-Object {
      $image = [IO.Path]::GetFileName($_.Image).ToLowerInvariant()
      $image -eq 'navaja.exe' -or
      ($image -eq 'msedgewebview2.exe' -and $tree.ContainsKey($_.ProcessId) -and $_.CommandLine -notmatch '--type=')
    } | ForEach-Object { (ConvertTo-Time $_.Time).ToUniversalTime() })
  $nearStart = {
    param($Connection)
    $at = (ConvertTo-Time $Connection.Time).ToUniversalTime()
    return @($appStarts | Where-Object { ($at - $_).TotalSeconds -ge 0 -and ($at - $_).TotalSeconds -le $StartupSeconds }).Count -gt 0
  }
  $woken = @($others | Where-Object { $WokenServices -contains $_.Identity -and (& $nearStart $_) })
  # What each process asked the DNS client for, and the addresses it got
  # back: event 3008, which the asking process logs. A woken service's
  # connection is tied to the names of its own process's lookups, made
  # before it, that gave its address; with no such lookup it is tied to
  # none, and fails.
  $lookups = @(foreach ($entry in $dns) {
      if ($entry.Id -ne 3008) { continue }
      $results = Get-Field $entry 'QueryResults'
      if (-not $results) { continue }
      [pscustomobject]@{
        Time = (ConvertTo-Time $entry.Time).ToUniversalTime()
        ProcessId = [int64]$entry.LoggedBy
        Name = Get-Field $entry 'QueryName'
        # "type: 5 <CNAME target>;" records, then addresses; IPv4 ones may
        # come mapped to IPv6 (::ffff:40.126.29.15).
        Addresses = @($results -split ';' | ForEach-Object { $_.Trim() } |
            Where-Object { $_ -and $_ -notlike 'type:*' } | ForEach-Object { $_ -replace '^::ffff:', '' })
      }
    })
  $entries = @(Get-DisclosedEntries $Name)
  $disclosed = @()
  $delegated = @()
  foreach ($connection in $woken) {
    $at = (ConvertTo-Time $connection.Time).ToUniversalTime()
    $connection.Asked = @($lookups | Where-Object {
        $_.ProcessId -eq $connection.ProcessId -and $_.Time -le $at -and $_.Addresses -contains $connection.Remote
      } | ForEach-Object { $_.Name } | Sort-Object -Unique) -join ', '
    $entry = Find-DisclosedEntry $entries $connection
    if ($null -ne $entry) {
      $disclosed += $connection | Select-Object *, @{ n = 'Entry'; e = { $entry.Label } }
    } else {
      $delegated += $connection
    }
  }

  # Outside the tree, for review: connections from identities the baseline
  # lacks, and connections in the $StartupSeconds after an app start (above)
  # from identities that do not poll all the time in the baseline (such as
  # the VM agents), but those of the services the webview wakes, counted
  # above. Then what started on demand in the phase.
  $suspects = @()
  if ($null -ne $BaselineIdentities) {
    foreach ($connection in $others) {
      if ($woken -contains $connection) { continue }
      $reasons = @()
      if ($BaselineIdentities -notcontains $connection.Identity) { $reasons += 'not in the baseline' }
      if ((& $nearStart $connection) -and $Steady -notcontains $connection.Identity) { $reasons += "within $StartupSeconds s of an app start" }
      if ($reasons.Count -eq 0) { continue }
      $suspects += $connection | Select-Object *, @{ n = 'Label'; e = { "$($_.Identity) ($($reasons -join '; '))" } }
    }
  }
  $onDemand = @($created | Where-Object {
      -not $tree.ContainsKey($_.ProcessId) -and
      ($_.ParentImage -eq 'services.exe' -or $_.CommandLine -match '-Embedding|-ServerName:|/Processid:')
    } | ForEach-Object {
      [pscustomobject]@{ Time = $_.Time; Identity = Format-Identity $_.Image '' $_.CommandLine }
    })

  return [pscustomobject]@{
    Name = $Name
    Start = (Read-Json "$Out\$Name-window.json")[0].Start
    Captured = (Test-Path $packetsPath)
    Tree = $tree
    SecurityEvents = $security.Count
    ConnectionEvents = @($security | Where-Object { $_.Id -eq 5156 -or $_.Id -eq 5157 }).Count
    DnsEvents = $dns.Count
    BitsEvents = $bits.Count
    Connections = $connections
    Outside = $outside
    Loopback = $loopback
    Binds = $binds
    Queries = $queries
    Jobs = $jobs
    Others = $others
    Identities = @($others | ForEach-Object { $_.Identity } | Sort-Object -Unique)
    Baseline = ($null -ne $BaselineIdentities)
    Suspects = $suspects
    OnDemand = $onDemand
    Woken = $woken
    Delegated = $delegated
    Disclosed = $disclosed
    Findings = $outside.Count + $queries.Count + $jobs.Count + $delegated.Count
  }
}

# Rows of connections, grouped, for a Markdown table.
# With -Asked, a column more: the names the connection is tied to through its
# process's own lookups (see Find-DisclosedEntry).
function Add-ConnectionRows($Lines, $Connections, [string]$Key, [datetime]$Start, [switch]$Asked) {
  $askedHeader = ''
  $askedRule = ''
  if ($Asked) {
    $askedHeader = ' Its process looked up, before it (DNS client) |'
    $askedRule = '---|'
  }
  $Lines.Add("| Process | Event | Direction | Destination | Port | Protocol | Name, from the capture's DNS packets |$askedHeader Events | First, from the phase's start |")
  $Lines.Add("|---|---|---|---|---|---|---|$askedRule---|---|")
  $rows = @($Connections | Group-Object $Key, Event, Direction, Remote, Port, Protocol | ForEach-Object {
      $first = $_.Group[0]
      $names = @($_.Group | ForEach-Object { $_.Names -split ', ' } | Where-Object { $_ } | Sort-Object -Unique) -join ', '
      if (-not $names) { $names = '-' }
      $askedCell = ''
      if ($Asked) {
        $tied = @($_.Group | ForEach-Object { $_.Asked -split ', ' } | Where-Object { $_ } | Sort-Object -Unique) -join ', '
        if (-not $tied) { $tied = '-' }
        $askedCell = " $tied |"
      }
      $earliest = ConvertTo-Time ($_.Group | Sort-Object Time | Select-Object -First 1).Time
      [pscustomobject]@{
        At = $earliest
        Text = "| $($first.$Key) | $($first.Event) | $($first.Direction) | $($first.Remote) | $($first.Port) | $($first.Protocol) | $names |$askedCell $($_.Count) | $(Format-Offset $earliest $Start) |"
      }
    })
  $rows | Sort-Object At | ForEach-Object { $Lines.Add($_.Text) }
  $Lines.Add('')
}

# The phase's table as Markdown, written to <name>-findings.md and printed.
function Write-Report($Report) {
  $start = ConvertTo-Time $Report.Start
  $label = $env:MATRIX_OS
  if (-not $label) { $label = 'windows' }
  $lines = New-Object System.Collections.Generic.List[string]
  $lines.Add("#### $($Report.Name): $label")
  $lines.Add('')
  $lines.Add('| Log | Events in the phase, all processes | Counted as the app''s | Of those, to loopback | Findings |')
  $lines.Add('|---|---|---|---|---|')
  $lines.Add("| WFP connections (5156 allowed, 5157 blocked; counted: the process tree's) | $($Report.ConnectionEvents) | $($Report.Connections.Count) | $($Report.Loopback.Count) | $($Report.Outside.Count) |")
  $lines.Add("| DNS client (the tree's queries, and the canary's names from any process) | $($Report.DnsEvents) | $($Report.Queries.Count) | - | $($Report.Queries.Count) |")
  $lines.Add("| BITS client (download jobs the tree created) | $($Report.BitsEvents) | $($Report.Jobs.Count) | - | $($Report.Jobs.Count) |")
  $lines.Add("| WFP connections of the services the webview wakes ($($WokenServices -join ', ')), in the $StartupSeconds s after a start of navaja.exe or its WebView2 browser process | - | $($Report.Woken.Count) | - | $($Report.Delegated.Count) ($($Report.Disclosed.Count) disclosed) |")
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
  if ($Report.Disclosed.Count -gt 0) {
    $lines.Add('Disclosed, not findings: connections that an entry of `scripts/spikes/s2-7-disclosed.tsv` covers in this phase (docs/adr/0003-webview-network.md).')
    $lines.Add('')
    Add-ConnectionRows $lines $Report.Disclosed 'Entry' $start -Asked
  }
  if ($Report.Findings -eq 0) {
    if ($Report.Disclosed.Count -gt 0) {
      $lines.Add('No other connection, DNS query or BITS job from the process tree or the services it wakes.')
    } else {
      $lines.Add('No connection, DNS query or BITS job from the process tree or the services it wakes.')
    }
    $lines.Add('')
  } else {
    if ($Report.Outside.Count -gt 0) { Add-ConnectionRows $lines $Report.Outside 'Process' $start }
    if ($Report.Delegated.Count -gt 0) { Add-ConnectionRows $lines $Report.Delegated 'Identity' $start -Asked }
    if ($Report.Queries.Count -gt 0) {
      $lines.Add('| Process | DNS client event | Name | Type | Events | First, from the phase''s start |')
      $lines.Add('|---|---|---|---|---|---|')
      $Report.Queries | Group-Object Process, Event, Name, Type | ForEach-Object {
        $first = $_.Group[0]
        $earliest = ($_.Group | Sort-Object Time | Select-Object -First 1).Time
        $lines.Add("| $($first.Process) | $($first.Event) | $($first.Name) | $($first.Type) | $($_.Count) | $(Format-Offset $earliest $start) |")
      }
      $lines.Add('')
    }
    if ($Report.Jobs.Count -gt 0) {
      $lines.Add('| BITS job | Created by | Hosts of its URLs | Bytes transferred | BITS events on it | First, from the phase''s start |')
      $lines.Add('|---|---|---|---|---|---|')
      foreach ($job in $Report.Jobs) {
        $hosts = @($job.Urls | ForEach-Object { try { ([uri]$_).Host } catch { $_ } } | Sort-Object -Unique) -join ', '
        if (-not $hosts) { $hosts = '-' }
        $when = '-'
        if ($null -ne $job.Time) { $when = Format-Offset $job.Time $start }
        $lines.Add("| $($job.Title) | $($job.Creator) | $hosts | $($job.Transferred) | $($job.Events) | $when |")
      }
      $lines.Add('')
    }
  }
  if ($Report.Name -eq 'idle' -or $Report.Name -eq 'in-use') {
    $lines.Add('##### Outside the process tree, for review (not findings)')
    $lines.Add('')
    $lines.Add("Connections outside loopback from processes outside the tree: from identities (image, plus the services it hosts or the COM server it is) that made none in the 5 min baseline, and from any identity but the baseline's steady pollers in the $StartupSeconds s after a start of navaja.exe or its WebView2 browser process, except the services the webview wakes, counted above. Another service that works for the tree without a log naming it would show here; so do the runner's own scheduled tasks.")
    $lines.Add('')
    if (-not $Report.Baseline) {
      $lines.Add('No baseline was captured, so there is no such list.')
    } elseif ($Report.Suspects.Count -eq 0) {
      $lines.Add('None.')
    } else {
      Add-ConnectionRows $lines $Report.Suspects 'Label' $start
    }
    $lines.Add('')
    $started = @($Report.OnDemand | Sort-Object Time | ForEach-Object { "``$($_.Identity)`` $(Format-Offset $_.Time $start)" })
    if ($started.Count -eq 0) { $started = @('none') }
    $lines.Add("Started on demand in the phase, outside the tree (services, and COM servers such as task hosts): $($started -join ', ').")
    $lines.Add('')
  }
  $lines.Add('')
  $text = $lines -join "`n"
  Set-Content -Path "$Out\$($Report.Name)-findings.md" -Value $text -Encoding utf8
  Write-Host $text
}

# --- Control ----------------------------------------------------------------------

# A request the logs must see and attribute, through the same analysis as the
# phases: if they don't, empty phase logs would prove nothing. Then a BITS
# download, which the BITS service makes for the process that asks: the BITS
# log must name that process, or a download the tree hands to BITS would go
# unattributed.
function Invoke-Control {
  # A cold cache, so that curl's lookup reaches the DNS client's network path.
  Clear-DnsClientCache
  Start-Capture 'control'
  $marks = Get-LogMarks
  $start = Get-Date
  Start-Sleep -Milliseconds 500
  $curl = Join-Path $env:SystemRoot 'System32\curl.exe'
  $proc = Start-Process -FilePath $curl -NoNewWindow -PassThru -Wait `
    -ArgumentList @('-sS', '-o', 'NUL', '-w', '%{http_code}', '--max-time', '30', 'https://github.com') `
    -RedirectStandardOutput "$Out\control-curl.txt"
  $code = (Get-Content -Raw "$Out\control-curl.txt").Trim()
  Write-Host "curl.exe (PID $($proc.Id)) https://github.com: HTTP $code, exit $($proc.ExitCode)"
  if ($proc.ExitCode -ne 0) { Fail 'control: curl https://github.com failed' }

  $powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
  $target = Join-Path $Out 'control-bits.txt'
  $bitsProc = Start-Process -FilePath $powershell -NoNewWindow -PassThru `
    -ArgumentList @('-NoProfile', '-NonInteractive', '-Command',
      "Start-BitsTransfer -Source 'https://github.com/robots.txt' -Destination '$target' -ErrorAction Stop")
  # Without the handle, ExitCode stays empty once the process is gone.
  $null = $bitsProc.Handle
  if (-not $bitsProc.WaitForExit(120000)) {
    Stop-Process -Id $bitsProc.Id -Force -ErrorAction SilentlyContinue
    Fail 'control: the BITS download of https://github.com/robots.txt did not finish within 120 s'
  }
  Write-Host "powershell.exe (PID $($bitsProc.Id)) Start-BitsTransfer https://github.com/robots.txt: exit $($bitsProc.ExitCode), $((Get-Item $target -ErrorAction SilentlyContinue).Length) bytes"
  if ($bitsProc.ExitCode -ne 0) { Fail 'control: the BITS download of https://github.com/robots.txt failed' }

  # Events reach the logs a moment after the fact.
  $marker = Send-DnsMarker 'control'
  Start-Sleep -Seconds 3
  $end = Get-Date
  Stop-Capture 'control'
  Export-Phase 'control' $marks $start $end $marker
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
  if (@($tcp | Where-Object { $_.Names -like '*github.com*' }).Count -eq 0) {
    Fail "control: pktmon's DNS packets do not name github.com for curl's connection; the names in the findings can't be trusted"
  }
  $bitsTree = @{ ([int64]$bitsProc.Id) = [pscustomobject]@{ Image = $powershell } }
  $jobs = @(Get-BitsJobs (Read-Json "$Out\control-bits.json") $bitsTree @())
  $jobs | Format-Table -AutoSize Title, Creator, Urls, Transferred, Events | Out-String -Width 200 | Write-Host
  if (@($jobs | Where-Object { @($_.Urls | Where-Object { $_ -like '*github.com*' }).Count -gt 0 -and $_.Events -gt 0 }).Count -eq 0) {
    Fail "control: the BITS log names no transfer of github.com in a job created by powershell.exe ($($bitsProc.Id)); a download the tree hands to BITS would go unattributed, so the logs can't be trusted"
  }
  Write-Host ('Control: the logs attributed curl''s connection and its DNS query to curl.exe, pktmon''s DNS packets named its host, ' +
    'and the BITS log attributed the download job to powershell.exe.')
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

# The live processes under $Root, from WMI. A process counts as a child only
# if it started after its parent: Windows reuses PIDs and keeps a process's
# parent PID after the parent exits. In run 37170350998 a WebView2 utility
# process got PID 756, the PID of the smss.exe that had started csrss.exe and
# wininit.exe at boot, so the tree took in every process on the machine, and
# Wait-Gone killed them, the runner's own worker too.
function Get-LiveTree([int64]$Root) {
  $all = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name, CommandLine, CreationDate)
  $started = @{}
  foreach ($process in $all) { $started[[int64]$process.ProcessId] = $process.CreationDate }
  $tree = @{ $Root = $true }
  do {
    $added = 0
    foreach ($process in $all) {
      $id = [int64]$process.ProcessId
      $parent = [int64]$process.ParentProcessId
      if ($tree.ContainsKey($id) -or -not $tree.ContainsKey($parent)) { continue }
      if ($null -eq $process.CreationDate) { continue }
      if ($null -ne $started[$parent] -and $process.CreationDate -lt $started[$parent]) { continue }
      $tree[$id] = $true
      $added++
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

# Waits up to 30 s for the given processes to end, then kills those left
# that run one of the tree's images ($TreeImages). Only those: a PID may have
# been reused by another process by then.
function Wait-Gone([int64[]]$ProcessIds) {
  for ($i = 0; $i -lt 60; $i++) {
    $left = @($ProcessIds | Where-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue })
    if ($left.Count -eq 0) { return }
    Start-Sleep -Milliseconds 500
  }
  $kill = @($left | Where-Object {
      $process = Get-Process -Id $_ -ErrorAction SilentlyContinue
      $null -ne $process -and $TreeImages -contains "$($process.ProcessName).exe".ToLowerInvariant()
    })
  $spared = @($left | Where-Object { $kill -notcontains $_ })
  Write-Host "::warning::S2.7 still running 30 s after the run: $($left -join ', '). Killed, as the tree's images: $($kill -join ', '); left alone: $($spared -join ', ')"
  $kill | ForEach-Object { Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue }
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

# The same capture and logs as idle, as long, before the app ever runs on
# this machine.
function Invoke-Baseline {
  $running = @(Get-Process -Name navaja -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Binary })
  if ($running.Count -gt 0) { Fail "baseline: the app already runs (PID $($running.Id -join ', ')), so this is no baseline" }
  Start-Capture 'baseline'
  $marks = Get-LogMarks
  $start = Get-Date
  Save-Snapshot 'baseline' 'start'
  Write-Host "Baseline: capturing $IdleSeconds s with nothing of Navaja running."
  Start-Sleep -Seconds $IdleSeconds
  Save-Snapshot 'baseline' 'end'
  $marker = Send-DnsMarker 'baseline'
  Start-Sleep -Seconds 3
  $end = Get-Date
  Stop-Capture 'baseline'
  Export-Phase 'baseline' $marks $start $end $marker
  Write-Host 'Baseline: done; the logs are checked in the next step.'
}

# The idle phase: the app starts as @wdio/tauri-service starts it, runs for
# $IdleSeconds with its window shown and nothing driving it, and is stopped.
function Invoke-Idle {
  if (-not (Test-Path $Binary)) { Fail "idle: no $Binary; build it first (see s2-7-egress.sh)" }
  if (Test-PortOpen) { Fail "idle: 127.0.0.1:$Port already accepts connections; stop whatever holds it" }
  $appDir = Join-Path ([IO.Path]::GetTempPath()) ('navaja-s2-7-' + [guid]::NewGuid())
  New-Item -ItemType Directory -Path $appDir | Out-Null
  Start-Capture 'idle'
  $marks = Get-LogMarks
  $start = Get-Date
  Save-Snapshot 'idle' 'start'
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
  Save-Snapshot 'idle' 'end'
  $marker = Send-DnsMarker 'idle'
  # A moment for anything still in flight, and for the logs.
  Start-Sleep -Seconds 3
  $end = Get-Date
  Stop-Capture 'idle'
  Export-Phase 'idle' $marks $start $end $marker
  if ($failure) { Fail "idle: $failure" }
  Write-Host 'Idle: done; the logs are checked in the next step.'
}

function Invoke-InUse {
  if (-not (Test-Path $Binary)) { Fail "in use: no $Binary; build it first (see s2-7-egress.sh)" }
  Start-Capture 'in-use'
  $marks = Get-LogMarks
  $start = Get-Date
  Save-Snapshot 'in-use' 'start'
  Start-Sleep -Milliseconds 500
  & pnpm e2e
  $status = $LASTEXITCODE
  # Whatever the suite started that is still running, the app or WebView2.
  $left = @(Get-CimInstance Win32_Process | Where-Object {
      ($_.Name -eq 'navaja.exe' -and $_.ExecutablePath -eq $Binary) -or
      ($_.Name -eq 'msedgewebview2.exe' -and $_.CreationDate -gt $start)
    } | ForEach-Object { [int64]$_.ProcessId })
  if ($left.Count -gt 0) { Wait-Gone $left }
  Save-Snapshot 'in-use' 'end'
  $marker = Send-DnsMarker 'in-use'
  Start-Sleep -Seconds 3
  $end = Get-Date
  Stop-Capture 'in-use'
  Export-Phase 'in-use' $marks $start $end $marker
  if ($status -ne 0) { Fail "in use: the end-to-end suite failed (exit $status)" }
  Write-Host 'In use: the suite passed; the logs are checked in the next step.'
}

function Invoke-Check([string]$Name) {
  # The tree's BITS jobs from earlier phases: a job outlives the process
  # that created it.
  $known = @()
  foreach ($file in @(Get-ChildItem -Path $Out -Filter '*-bits-jobs.json' -ErrorAction SilentlyContinue)) {
    if ($file.Name -ne "$Name-bits-jobs.json") { $known += @(Read-Json $file.FullName | ForEach-Object { [string]$_ }) }
  }
  $baseline = $null
  $steady = @()
  if ($Name -ne 'baseline' -and (Test-Path "$Out\baseline-identities.json")) {
    $baseline = @(Read-Json "$Out\baseline-identities.json" | ForEach-Object { [string]$_ })
    $steady = @(Read-Json "$Out\baseline-steady.json" | ForEach-Object { [string]$_ })
  }
  $report = Get-Report $Name $RootImages $TreeImages @() $known $baseline $steady
  Save-Json "$Out\$Name-bits-jobs.json" @($report.Jobs | ForEach-Object { $_.Job })
  Write-Report $report
  if ($Name -eq 'baseline') {
    Save-Json "$Out\baseline-identities.json" $report.Identities
    # The identities that connect out in at least half of the baseline's
    # 30 s slices: pollers such as the VM agents, which the start-up
    # window's list leaves out.
    $start = ConvertTo-Time $report.Start
    $end = ConvertTo-Time (Read-Json "$Out\baseline-window.json")[0].End
    $slices = [math]::Max(1, [math]::Ceiling(($end - $start).TotalSeconds / 30))
    $steadyIds = @($report.Others | Group-Object Identity | Where-Object {
        $seen = @($_.Group | ForEach-Object { [math]::Floor(((ConvertTo-Time $_.Time) - $start).TotalSeconds / 30) } | Sort-Object -Unique)
        $seen.Count * 2 -ge $slices
      } | ForEach-Object { $_.Name })
    Save-Json "$Out\baseline-steady.json" $steadyIds
    Write-Host "baseline: steady pollers, out in at least half of its $slices slices of 30 s: $($steadyIds -join ', ')."
    Write-Host "baseline: $($report.Identities.Count) identities outside the tree connected outside loopback."
    if ($report.Findings -gt 0) {
      Fail "baseline: $($report.Findings) event(s) counted as the app's while it was not running, so the attribution would blame it for the machine's own traffic; see the tables above"
    }
    Write-Host 'baseline: nothing counted as the app''s while it was not running.'
    return
  }
  # The app's own WebDriver connections show that the logs saw it.
  if (@($report.Loopback | Where-Object { $_.Process -like 'navaja.exe *' }).Count -eq 0) {
    Fail "${Name}: the logs hold no connection of navaja.exe, not even its WebDriver on loopback, so they prove nothing"
  }
  $suspects = @($report.Suspects | Group-Object Label, Event, Direction, Remote, Port, Protocol).Count
  Set-Content -Path "$Out\$Name-suspects.count" -Value $suspects -Encoding ascii
  if ($suspects -gt 0) {
    Write-Host "::warning::S2.7 ${Name}: $suspects connection(s) outside the process tree listed above for review (not findings)"
  }
  $disclosed = $report.Disclosed.Count
  Set-Content -Path "$Out\$Name-disclosed.count" -Value $disclosed -Encoding ascii
  if ($disclosed -gt 0) {
    Write-Host "::notice::S2.7 ${Name}: $disclosed connection(s) covered by the disclosed list (scripts/spikes/s2-7-disclosed.tsv), reported above, not findings"
  }
  if ($report.Findings -gt 0) {
    Fail "${Name}: $($report.Findings) connection, DNS or BITS event(s) from the process tree or the services it wakes; see the tables above"
  }
  if ($disclosed -gt 0) {
    Write-Host "${Name}: no connection, DNS query or BITS job from the process tree or the services it wakes but the $disclosed the disclosed list covers."
  } else {
    Write-Host "${Name}: no connection, DNS query or BITS job from the process tree or the services it wakes."
  }
}

switch ($Mode) {
  'setup' { Invoke-Setup }
  'control' { Invoke-Control }
  'baseline' { Invoke-Baseline }
  'idle' { Invoke-Idle }
  'in-use' { Invoke-InUse }
  'check' {
    if (@('baseline', 'idle', 'in-use') -notcontains $Phase) { Write-Host 'usage: s2-7-egress.ps1 check baseline|idle|in-use'; exit 2 }
    Invoke-Check $Phase
  }
  'teardown' { Invoke-Teardown }
  default {
    Write-Host 'usage: s2-7-egress.ps1 setup|control|baseline|check baseline|idle|check idle|in-use|check in-use|teardown'
    exit 2
  }
}
exit 0
