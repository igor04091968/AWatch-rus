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
$script:TransportQueuePath = $null
$script:TransportQueueLockPath = $null
$script:StartupTracePath = if ($LogPath) { $LogPath } else { $null }
$script:TransportMetrics = @{
    eventsEnqueued = 0
    eventsFlushed  = 0
    sendFailures   = 0
    queueDepth     = 0
}

# Настройка логирования
$script:LogPath = $null
$script:LocalAgentLogsEnabled = $false

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

function Write-StartupTrace {
    param([string]$Message)
    if ([string]::IsNullOrWhiteSpace($script:StartupTracePath)) { return }
    try {
        $dir = Split-Path -Parent $script:StartupTracePath
        if ($dir -and -not (Test-Path -LiteralPath $dir)) {
            New-Item -Path $dir -ItemType Directory -Force | Out-Null
        }
        [System.IO.File]::AppendAllText($script:StartupTracePath, ('{0} [startup] {1}{2}' -f (Get-Date -Format s), $Message, [Environment]::NewLine), [System.Text.Encoding]::UTF8)
    }
    catch {
    }
}

function Invoke-AwJsonPost {
    param(
        [Parameter(Mandatory = $true)][string]$Uri,
        [Parameter(Mandatory = $true)][string]$Json
    )
    $httpClient = $null
    try {
        $httpClient = New-Object System.Net.Http.HttpClient
        $content = New-Object System.Net.Http.StringContent($Json, [System.Text.Encoding]::UTF8, "application/json")
        $response = $httpClient.PostAsync($Uri, $content).Result
        if (-not $response.IsSuccessStatusCode) {
            $status = [int]$response.StatusCode
            $reason = [string]$response.ReasonPhrase
            $body = $response.Content.ReadAsStringAsync().Result
            Write-FileCollectorLog ("POST failed: uri={0} status={1} reason={2} body={3}" -f $Uri, $status, $reason, $body)
            throw "HTTP POST failed status=$status"
        }
    } catch {
        Write-FileCollectorLog "POST Error: $($_.Exception.Message)"
        throw
    } finally {
        if ($null -ne $httpClient) {
            $httpClient.Dispose()
        }
    }
}

function Initialize-TransportQueue {
    param(
        [Parameter(Mandatory = $true)][string]$StateRoot
    )
    $script:TransportQueuePath = Join-Path $StateRoot 'file-operations-queue.jsonl'
    $script:TransportQueueLockPath = Join-Path $StateRoot 'file-operations-queue.lock'
    if (-not (Test-Path -LiteralPath $script:TransportQueuePath)) {
        New-Item -Path $script:TransportQueuePath -ItemType File -Force | Out-Null
    }
}

function Get-TransportQueueLock {
    $tries = 0
    while ($tries -lt 50) {
        try {
            $fs = [System.IO.File]::Open($script:TransportQueueLockPath, [System.IO.FileMode]::OpenOrCreate, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None)
            return $fs
        }
        catch {
            Start-Sleep -Milliseconds 50
            $tries++
        }
    }
    throw "Failed to acquire transport queue lock: $script:TransportQueueLockPath"
}

function Add-TransportQueueItem {
    param(
        [Parameter(Mandatory = $true)][string]$Uri,
        [Parameter(Mandatory = $true)][string]$Payload,
        [string]$Kind = 'file_op'
    )
    $lock = Get-TransportQueueLock
    try {
        $line = @{
            ts      = (Get-Date).ToUniversalTime().ToString('o')
            uri     = $Uri
            payload = $Payload
            kind    = $Kind
        } | ConvertTo-Json -Compress
        Add-Content -LiteralPath $script:TransportQueuePath -Value $line -Encoding UTF8
        $script:TransportMetrics.eventsEnqueued++
    }
    finally {
        $lock.Dispose()
    }
}

function Read-TransportQueueItems {
    if (-not (Test-Path -LiteralPath $script:TransportQueuePath)) { return @() }
    $items = @()
    foreach ($line in @(Get-Content -LiteralPath $script:TransportQueuePath -ErrorAction SilentlyContinue)) {
        if ([string]::IsNullOrWhiteSpace($line)) { continue }
        try { $items += ($line | ConvertFrom-Json) } catch {}
    }
    return $items
}

function Flush-TransportQueue {
    param(
        [int]$MaxItems = 100
    )
    if (-not (Test-Path -LiteralPath $script:TransportQueuePath)) { return }
    $lock = Get-TransportQueueLock
    try {
        $items = @(Read-TransportQueueItems)
        $itemCount = @($items).Count
        $script:TransportMetrics.queueDepth = $itemCount
        if ($itemCount -eq 0) { return }

        $left = New-Object System.Collections.Generic.List[object]
        $sent = 0
        foreach ($item in $items) {
            if ($sent -ge $MaxItems) {
                $left.Add($item)
                continue
            }
            try {
                Invoke-AwJsonPost -Uri ([string]$item.uri) -Json ([string]$item.payload)
                $sent++
                $script:TransportMetrics.eventsFlushed++
            }
            catch {
                $script:TransportMetrics.sendFailures++
                $left.Add($item)
            }
        }
        foreach ($item in $items | Select-Object -Skip ($sent + $left.Count)) {
            $left.Add($item)
        }

        $lines = @($left | ForEach-Object { $_ | ConvertTo-Json -Compress })
        Set-Content -LiteralPath $script:TransportQueuePath -Value $lines -Encoding UTF8
        $script:TransportMetrics.queueDepth = $left.Count
    }
    finally {
        $lock.Dispose()
    }
}

function Ensure-Bucket {
    param(
        [string]$BucketId,
        [string]$ClientName,
        [string]$BucketType
    )

    if ($script:KnownBuckets.ContainsKey($BucketId)) {
        return
    }

    try {
        Invoke-RestMethod -Method Get -Uri "$($script:ApiBase)/buckets/$BucketId" | Out-Null
        $script:KnownBuckets[$BucketId] = $true
        return
    }
    catch {
    }

    $body = @{
        client   = $ClientName
        type     = $BucketType
        hostname = $script:Hostname
    } | ConvertTo-Json -Compress

    try {
        Invoke-AwJsonPost -Uri "$($script:ApiBase)/buckets/$BucketId" -Json $body
    }
    catch {
        try {
            Invoke-RestMethod -Method Get -Uri "$($script:ApiBase)/buckets/$BucketId" | Out-Null
        }
        catch {
            Write-FileCollectorLog "Bucket create/check failed for ${BucketId}: $($_.Exception.Message)"
            throw
        }
    }
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

    Write-FileCollectorLog ("Queue file event: op={0} path={1}" -f $Operation, $FilePath)
    Add-TransportQueueItem -Uri "$($script:ApiBase)/buckets/$bucketId/heartbeat?pulsetime=15" -Payload $payload -Kind 'file_op'
    Flush-TransportQueue -MaxItems 20
}

function Send-CollectorHealthEvent {
    $bucketId = 'aw-file-operations_' + $script:Hostname
    Ensure-Bucket -BucketId $bucketId -ClientName 'aw-file-operations' -BucketType 'aw.file.operation'
    $payload = @{
        timestamp = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
        duration  = 0
        data      = @{
            signalType     = 'collector_health'
            username       = $env:USERNAME
            hostname       = $script:Hostname
            sessionId      = $script:SessionId
            queueDepth     = [int]$script:TransportMetrics.queueDepth
            eventsEnqueued = [int]$script:TransportMetrics.eventsEnqueued
            eventsFlushed  = [int]$script:TransportMetrics.eventsFlushed
            sendFailures   = [int]$script:TransportMetrics.sendFailures
        }
    } | ConvertTo-Json -Depth 5 -Compress
    Add-TransportQueueItem -Uri "$($script:ApiBase)/buckets/$bucketId/heartbeat?pulsetime=30" -Payload $payload -Kind 'health'
    Flush-TransportQueue -MaxItems 50
}

function Handle-FileWatcherEvent {
    param(
        [Parameter(Mandatory = $true)]$EventRecord
    )

    try {
        $args = $EventRecord.SourceEventArgs
        $eventName = $null
        if ($args -and $args.PSObject.Properties.Name -contains 'ChangeType' -and $args.ChangeType) {
            $eventName = [string]$args.ChangeType
        }
        elseif ($EventRecord.SourceIdentifier -match '-created$') {
            $eventName = 'Created'
        }
        elseif ($EventRecord.SourceIdentifier -match '-deleted$') {
            $eventName = 'Deleted'
        }
        elseif ($EventRecord.SourceIdentifier -match '-renamed$') {
            $eventName = 'Renamed'
        }

        Write-FileCollectorLog ("Received event: src={0} type={1}" -f $EventRecord.SourceIdentifier, $eventName)

        switch ($eventName) {
            'Created' {
                $path = [string]$args.FullPath
                $size = 0
                try {
                    if (Test-Path -LiteralPath $path) {
                        $size = (Get-Item -LiteralPath $path).Length
                    }
                }
                catch {
                }
                Send-FileOperationEvent -Operation 'Created' -FilePath $path -Size $size
            }
            'Deleted' {
                Send-FileOperationEvent -Operation 'Deleted' -FilePath ([string]$args.FullPath)
            }
            'Renamed' {
                Send-FileOperationEvent -Operation 'Renamed' -FilePath ([string]$args.FullPath) -OldFilePath ([string]$args.OldFullPath)
            }
        }
    }
    catch {
        Write-FileCollectorLog ("File watcher event failed: {0}" -f $_.Exception.Message)
    }
    finally {
        if ($EventRecord.EventIdentifier) {
            Remove-Event -EventIdentifier $EventRecord.EventIdentifier -ErrorAction SilentlyContinue
        }
    }
}

Write-StartupTrace "boot"
$config = Get-DeploymentConfig -Path $ConfigPath
if (-not $config) { throw "Configuration file not found: $ConfigPath" }
Write-StartupTrace "config-loaded"
$script:Hostname = if ($config.PSObject.Properties.Name -contains 'awHostname' -and -not [string]::IsNullOrWhiteSpace([string]$config.awHostname)) { [string]$config.awHostname } else { [string]$env:COMPUTERNAME }

$scheme = if ($ServerScheme) { $ServerScheme } elseif ($config.server.scheme) { $config.server.scheme } else { 'http' }
$serverHostName = if ($ServerHost) { $ServerHost } elseif ($config.server.host) { $config.server.host } else { 'localhost' }
$port = if ($ServerPort) { $ServerPort } elseif ($config.server.port) { $config.server.port } else { 5600 }
$script:ApiBase = "{0}://{1}:{2}/api/0" -f $scheme, $serverHostName, $port
$stateRoot = if ($config.paths -and $config.paths.stateRoot) { [string]$config.paths.stateRoot } else { 'C:\ProgramData\AWatch-rus' }
Write-StartupTrace ("api={0}" -f $script:ApiBase)
$resolvedLogsRoot = if ($config.paths -and $config.paths.logsRoot) { [string]$config.paths.logsRoot } else { Join-Path $stateRoot 'logs' }
$resolvedLocalAgentLogsEnabled = if ($config.PSObject.Properties.Name -contains 'logging' -and $config.logging -and $config.logging.PSObject.Properties.Name -contains 'localAgentLogsEnabled') { [bool]$config.logging.localAgentLogsEnabled } else { $true }
$script:LogPath = if ($LogPath) { $LogPath } else { Join-Path $resolvedLogsRoot ("file-operations-{0}.log" -f $env:USERNAME) }
$explicitLogPath = -not [string]::IsNullOrWhiteSpace($LogPath)
$script:LocalAgentLogsEnabled = ($explicitLogPath -or $resolvedLocalAgentLogsEnabled) -and -not [string]::IsNullOrWhiteSpace($script:LogPath)
if ($script:LocalAgentLogsEnabled -and -not (Test-Path -LiteralPath $resolvedLogsRoot)) {
    New-Item -Path $resolvedLogsRoot -ItemType Directory -Force | Out-Null
}
Initialize-TransportQueue -StateRoot $stateRoot
Write-StartupTrace "queue-ready"

$bucketId = 'aw-file-operations_' + $script:Hostname
Ensure-Bucket -BucketId $bucketId -ClientName 'aw-file-operations' -BucketType 'aw.file.operation'
Write-StartupTrace ("bucket-ready={0}" -f $bucketId)

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
Write-StartupTrace ("watch-paths={0}" -f ($resolvedPaths -join ';'))

if ($resolvedPaths.Count -eq 0) {
    Write-FileCollectorLog "No valid watch paths found. Exiting."
    exit 0
}

Write-FileCollectorLog "Starting watch on paths: $($resolvedPaths -join ', ')"

$watchers = @()
$subscriptions = @()
$eventPrefix = "aw-fileops-$PID"
$watchIndex = 0
foreach ($path in $resolvedPaths) {
    $watcher = New-Object System.IO.FileSystemWatcher
    $watcher.Path = $path
    $watcher.IncludeSubdirectories = $true
    $watcher.EnableRaisingEvents = $true

    $onChanged = Register-ObjectEvent -InputObject $watcher -EventName "Created" -SourceIdentifier ("{0}-{1}-created" -f $eventPrefix, $watchIndex)
    $onDeleted = Register-ObjectEvent -InputObject $watcher -EventName "Deleted" -SourceIdentifier ("{0}-{1}-deleted" -f $eventPrefix, $watchIndex)
    $onRenamed = Register-ObjectEvent -InputObject $watcher -EventName "Renamed" -SourceIdentifier ("{0}-{1}-renamed" -f $eventPrefix, $watchIndex)

    $watchers += $watcher
    $subscriptions += @($onChanged, $onDeleted, $onRenamed)
    $watchIndex++
}

Write-FileCollectorLog "Collector started. Waiting for events..."

try {
    try {
        Send-CollectorHealthEvent
        Write-FileCollectorLog "Initial collector_health heartbeat sent."
    }
    catch {
        $script:TransportMetrics.sendFailures++
        Write-FileCollectorLog ("Initial collector_health failed: {0}" -f $_.Exception.Message)
    }

    $lastHealth = [datetime]::UtcNow
    $backoffSeconds = 1
    while ($true) {
        $eventRecord = Wait-Event -Timeout $PollSeconds -ErrorAction SilentlyContinue
        if ($eventRecord) {
            if ([string]$eventRecord.SourceIdentifier -like "$eventPrefix-*") {
                Handle-FileWatcherEvent -EventRecord $eventRecord
            }
            else {
                Remove-Event -EventIdentifier $eventRecord.EventIdentifier -ErrorAction SilentlyContinue
            }

            foreach ($pendingEvent in @(Get-Event | Where-Object { [string]$_.SourceIdentifier -like "$eventPrefix-*" })) {
                Handle-FileWatcherEvent -EventRecord $pendingEvent
            }
        }

        try {
            Flush-TransportQueue -MaxItems 100
            $backoffSeconds = 1
        }
        catch {
            $script:TransportMetrics.sendFailures++
            $backoffSeconds = [Math]::Min($backoffSeconds * 2, 60)
            Write-FileCollectorLog ("Queue flush failed, backoff={0}s err={1}" -f $backoffSeconds, $_.Exception.Message)
        }

        if ((New-TimeSpan -Start $lastHealth -End ([datetime]::UtcNow)).TotalSeconds -ge ([Math]::Max($PollSeconds * 3, 30))) {
            try {
                Send-CollectorHealthEvent
                Write-FileCollectorLog "Periodic collector_health heartbeat sent."
            }
            catch {
                $script:TransportMetrics.sendFailures++
                Write-FileCollectorLog ("Periodic collector_health failed: {0}" -f $_.Exception.Message)
            }
            $lastHealth = [datetime]::UtcNow
        }
        elseif ($backoffSeconds -gt $PollSeconds) {
            Start-Sleep -Seconds $backoffSeconds
        }
    }
}
finally {
    Write-FileCollectorLog "Stopping collector..."
    foreach ($pendingEvent in @(Get-Event | Where-Object { [string]$_.SourceIdentifier -like "$eventPrefix-*" })) {
        try {
            Remove-Event -EventIdentifier $pendingEvent.EventIdentifier -ErrorAction SilentlyContinue
        }
        catch {
        }
    }
    foreach ($sub in @($subscriptions)) {
        try {
            if ($sub -and $sub.Id) {
                Unregister-Event -SubscriptionId $sub.Id -ErrorAction SilentlyContinue
                Remove-Job -Id $sub.Id -Force -ErrorAction SilentlyContinue
            }
        } catch {}
    }
    foreach ($w in $watchers) {
        $w.EnableRaisingEvents = $false
        $w.Dispose()
    }
}
