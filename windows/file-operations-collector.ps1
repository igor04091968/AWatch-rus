[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\AWatch-rus\deployment-config.json',
    [string]$ServerHost,
    [int]$ServerPort,
    [ValidateSet('http', 'https')]
    [string]$ServerScheme,
    [string]$PolicyPath,
    [string]$LogPath,
    [int]$PollSeconds = 10,
    [string[]]$WatchPaths = @('Desktop', 'Documents', 'Downloads')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Force TLS 1.2 and load networking types
[System.Net.ServicePointManager]::SecurityProtocol = [System.Net.SecurityProtocolType]::Tls12
Add-Type -AssemblyName System.Net.Http

# Bucket registry
$script:KnownBuckets = @{}
$script:Hostname = $env:COMPUTERNAME
$script:SessionId = [System.Diagnostics.Process]::GetCurrentProcess().SessionId

# Настройка логирования
$script:LogPath = $LogPath
$script:LocalAgentLogsEnabled = [bool]$LogPath
$script:TransportStats = @{ eventsEnqueued = 0; eventsSent = 0; sendFailures = 0; lastSendStatus = 'init' }
$script:QueuePath = $null

function Get-DeploymentConfig {
    param([string]$Path)
    if ($Path -and (Test-Path -LiteralPath $Path)) {
        return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    }
    return $null
}

function Write-FileCollectorLog {
    param([string]$Message)
    if (-not $script:LocalAgentLogsEnabled) { return }
    try {
        Add-Content -LiteralPath $script:LogPath -Value ('{0} [FileCollector] {1}' -f (Get-Date -Format s), $Message)
    } catch {}
}


function Get-NewEventId { return ([guid]::NewGuid().ToString()) }

function Initialize-TransportQueue {
    param([Parameter(Mandatory = $true)][string]$QueuePath)
    $script:QueuePath = $QueuePath
    try {
        $dir = Split-Path -Path $QueuePath -Parent
        if ($dir -and -not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
        if (-not (Test-Path -LiteralPath $QueuePath)) { New-Item -ItemType File -Path $QueuePath -Force | Out-Null }
    } catch {
        Write-FileCollectorLog "Queue init error: $($_.Exception.Message)"
    }
}

function Add-TransportQueueRecord {
    param([string]$Uri,[string]$Json)
    if (-not $script:QueuePath) { return }
    $record = @{ id = (Get-NewEventId); createdAt = (Get-Date).ToUniversalTime().ToString('o'); uri = $Uri; payload = $Json }
    Add-Content -LiteralPath $script:QueuePath -Value ($record | ConvertTo-Json -Compress)
    $script:TransportStats.eventsEnqueued++
}

function Get-QueueDepth {
    if (-not $script:QueuePath -or -not (Test-Path -LiteralPath $script:QueuePath)) { return 0 }
    return @((Get-Content -LiteralPath $script:QueuePath)).Count
}

function Try-FlushTransportQueue {
    param([int]$MaxItems = 20)
    if (-not $script:QueuePath -or -not (Test-Path -LiteralPath $script:QueuePath)) { return }
    $lines = @(Get-Content -LiteralPath $script:QueuePath)
    if ($lines.Count -eq 0) { return }
    $remaining = New-Object System.Collections.Generic.List[string]
    $sentInRun = 0
    foreach ($line in $lines) {
        if ($sentInRun -ge $MaxItems) { $remaining.Add($line); continue }
        try { $rec = $line | ConvertFrom-Json } catch { $remaining.Add($line); continue }
        if (Invoke-AwJsonPost -Uri ([string]$rec.uri) -Json ([string]$rec.payload)) {
            $script:TransportStats.eventsSent++
            $script:TransportStats.lastSendStatus = 'ok'
            $sentInRun++
        } else {
            $script:TransportStats.sendFailures++
            $script:TransportStats.lastSendStatus = 'failed'
            $remaining.Add($line)
            break
        }
    }
    if ($sentInRun -lt $lines.Count) {
        for ($i=$sentInRun+($lines.Count-$remaining.Count); $i -lt $lines.Count; $i++) { }
    }
    Set-Content -LiteralPath $script:QueuePath -Value $remaining
}

function Send-WithQueue {
    param([string]$Uri,[string]$Json)
    Add-TransportQueueRecord -Uri $Uri -Json $Json
    Try-FlushTransportQueue -MaxItems 10
}

function Invoke-AwJsonPost {
    param(
        [Parameter(Mandatory = $true)][string]$Uri,
        [Parameter(Mandatory = $true)][string]$Json
    )
    $httpClient = New-Object System.Net.Http.HttpClient
    try {
        $content = New-Object System.Net.Http.StringContent($Json, [System.Text.Encoding]::UTF8, "application/json")
        $response = $httpClient.PostAsync($Uri, $content).Result
        if (-not $response.IsSuccessStatusCode) {
            $statusCode = [int]$response.StatusCode
            $reason = [string]$response.ReasonPhrase
            $responseBody = $response.Content.ReadAsStringAsync().Result
            Write-FileCollectorLog ("POST failed uri={0} status={1} reason={2} body={3}" -f $Uri, $statusCode, $reason, $responseBody)
        }
    } catch {
        Write-FileCollectorLog "POST Error: $($_.Exception.Message)"
        return $false
    } finally {
        $httpClient.Dispose()
    }
    return $response.IsSuccessStatusCode
}

function Ensure-Bucket {
    param(
        [string]$BucketId,
        [string]$ClientName,
        [string]$BucketType
    )
    if ($script:KnownBuckets.ContainsKey($BucketId)) { return }
    $body = @{
        client   = $ClientName
        type     = $BucketType
        hostname = $script:Hostname
    } | ConvertTo-Json -Compress
    Send-WithQueue -Uri "$($script:ApiBase)/buckets/$BucketId" -Json $body
    $script:KnownBuckets[$BucketId] = $true
}

function Send-FileOperationEvent {
    param(
        [string]$Operation,
        [string]$FilePath,
        [string]$OldFilePath = $null,
        [long]$Size = 0
    )

    $bucketId = 'aw-file-operations_' + $script:Hostname
    Ensure-Bucket -BucketId $bucketId -ClientName 'aw-file-operations' -BucketType 'aw.file.operation'

    $data = @{
        eventId   = (Get-NewEventId)
        eventCreatedAt = (Get-Date).ToUniversalTime().ToString('o')

        operation = $Operation
        path      = $FilePath
        extension = [System.IO.Path]::GetExtension($FilePath)
        username  = $env:USERNAME
        hostname  = $script:Hostname
    }
    if ($OldFilePath) { $data.oldPath = $OldFilePath }
    if ($Size -gt 0) { $data.size = $Size }

    # Детекция архивации (упрощенная)
    if ($Operation -eq 'Created' -and $data.extension -match '\.(zip|7z|rar|tar|gz)$') {
        $data.archiveHint = $true
    }

    $payload = @{
        timestamp = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
        duration  = 0
        data      = $data
    } | ConvertTo-Json -Depth 5 -Compress

    Send-WithQueue -Uri "$($script:ApiBase)/buckets/$bucketId/heartbeat?pulsetime=15" -Json $payload
}

$config = Get-DeploymentConfig -Path $ConfigPath
if (-not $config) { throw "Configuration file not found: $ConfigPath" }

$scheme = if ($ServerScheme) { $ServerScheme } elseif ($config.server.scheme) { $config.server.scheme } else { 'http' }
$hostName = if ($ServerHost) { $ServerHost } elseif ($config.server.host) { $config.server.host } else { 'localhost' }
$port = if ($ServerPort) { $ServerPort } elseif ($config.server.port) { $config.server.port } else { 5600 }
$script:ApiBase = "{0}://{1}:{2}/api/0" -f $scheme, $hostName, $port

$queueFile = Join-Path ([System.IO.Path]::GetDirectoryName($script:LogPath)) ("file-collector-queue-{0}.jsonl" -f $env:USERNAME)
Initialize-TransportQueue -QueuePath $queueFile

$bucketId = 'aw-file-operations_' + $script:Hostname
Ensure-Bucket -BucketId $bucketId -ClientName 'aw-file-operations' -BucketType 'aw.file.operation'

# Resolve paths for monitoring
$resolvedPaths = @()
foreach ($p in $WatchPaths) {
    $fullPath = $p
    if (-not [System.IO.Path]::IsPathRooted($p)) {
        try {
            if ($p -eq 'Desktop') { $fullPath = [Environment]::GetFolderPath('Desktop') }
            elseif ($p -eq 'Documents') { $fullPath = [Environment]::GetFolderPath('MyDocuments') }
            elseif ($p -eq 'Downloads') { $fullPath = Join-Path $env:USERPROFILE 'Downloads' }
        } catch {}
    }
    if ($fullPath -and (Test-Path -LiteralPath $fullPath)) {
        $resolvedPaths += $fullPath
    }
}

if ($resolvedPaths.Count -eq 0) {
    Write-FileCollectorLog "No valid watch paths found. Exiting."
    exit 0
}

Write-FileCollectorLog "Starting watch on paths: $($resolvedPaths -join ', ')"

$watchers = @()
$subscriptions = @()
foreach ($path in $resolvedPaths) {
    $watcher = New-Object System.IO.FileSystemWatcher
    $watcher.Path = $path
    $watcher.IncludeSubdirectories = $true
    $watcher.EnableRaisingEvents = $true
    
    $onChanged = Register-ObjectEvent $watcher "Created" -Action {
        $path = $Event.SourceEventArgs.FullPath
        $size = 0
        try { if (Test-Path -LiteralPath $path) { $size = (Get-Item -LiteralPath $path).Length } } catch {}
        Send-FileOperationEvent -Operation 'Created' -FilePath $path -Size $size
    }
    $onDeleted = Register-ObjectEvent $watcher "Deleted" -Action {
        Send-FileOperationEvent -Operation 'Deleted' -FilePath $Event.SourceEventArgs.FullPath
    }
    $onRenamed = Register-ObjectEvent $watcher "Renamed" -Action {
        Send-FileOperationEvent -Operation 'Renamed' -FilePath $Event.SourceEventArgs.FullPath -OldFilePath $Event.SourceEventArgs.OldFullPath
    }
    $subscriptions += @($onChanged, $onDeleted, $onRenamed)
    $watchers += $watcher
}

Write-FileCollectorLog "Collector started. Waiting for events..."

try {
    while ($true) {
        Try-FlushTransportQueue -MaxItems 50
        Write-FileCollectorLog ("transport metrics queueDepth={0} enqueued={1} sent={2} failures={3} lastStatus={4}" -f (Get-QueueDepth), $script:TransportStats.eventsEnqueued, $script:TransportStats.eventsSent, $script:TransportStats.sendFailures, $script:TransportStats.lastSendStatus)
        Start-Sleep -Seconds $PollSeconds
    }
}
finally {
    Write-FileCollectorLog "Stopping collector..."
    foreach ($sub in @($subscriptions)) {
        if ($null -ne $sub) {
            try { Unregister-Event -SubscriptionId $sub.Id -ErrorAction SilentlyContinue } catch {}
            try { Remove-Job -Id $sub.Id -Force -ErrorAction SilentlyContinue } catch {}
        }
    }
    foreach ($w in $watchers) {
        $w.EnableRaisingEvents = $false
        $w.Dispose()
    }
}
