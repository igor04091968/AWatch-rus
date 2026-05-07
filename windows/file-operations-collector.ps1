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
$script:WalPath = $null
$script:HealthPath = $null
$script:WalFlushing = $false

# Настройка логирования
$script:LogPath = $LogPath
$script:LocalAgentLogsEnabled = [bool]$LogPath

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

function Add-WalEntry {
    param(
        [Parameter(Mandatory = $true)][string]$Uri,
        [Parameter(Mandatory = $true)][string]$Json
    )
    if ([string]::IsNullOrWhiteSpace($script:WalPath)) { return }
    try {
        $entry = @{ ts = (Get-Date).ToUniversalTime().ToString('o'); uri = $Uri; json = $Json } | ConvertTo-Json -Compress
        Add-Content -LiteralPath $script:WalPath -Value $entry -Encoding UTF8
    } catch {}
}

function Flush-Wal {
    if ([string]::IsNullOrWhiteSpace($script:WalPath) -or -not (Test-Path -LiteralPath $script:WalPath)) { return }
    $remaining = New-Object System.Collections.Generic.List[string]
    try {
        $script:WalFlushing = $true
        foreach ($line in (Get-Content -LiteralPath $script:WalPath -ErrorAction SilentlyContinue)) {
            if ([string]::IsNullOrWhiteSpace($line)) { continue }
            try {
                $entry = $line | ConvertFrom-Json
                if ($null -eq $entry -or -not $entry.uri -or -not $entry.json) { continue }
                if (-not (Invoke-AwJsonPost -Uri ([string]$entry.uri) -Json ([string]$entry.json))) { $remaining.Add($line) }
            } catch { $remaining.Add($line) }
        }
        if ($remaining.Count -eq 0) {
            Remove-Item -LiteralPath $script:WalPath -Force -ErrorAction SilentlyContinue
        } else {
            Set-Content -LiteralPath $script:WalPath -Value ($remaining -join [Environment]::NewLine) -Encoding UTF8
        }
    } finally {
        $script:WalFlushing = $false
    }
}

function Write-CollectorHealth {
    param([string]$Status = 'running')
    if ([string]::IsNullOrWhiteSpace($script:HealthPath)) { return }
    try {
        $walDepth = 0
        if ($script:WalPath -and (Test-Path -LiteralPath $script:WalPath)) { $walDepth = @((Get-Content -LiteralPath $script:WalPath)).Count }
        $health = @{
            collector = 'file-operations'; hostname = $script:Hostname; sessionId = $script:SessionId;
            status = $Status; apiBase = $script:ApiBase; walDepth = $walDepth; ts = (Get-Date).ToUniversalTime().ToString('o')
        } | ConvertTo-Json -Depth 5
        Set-Content -LiteralPath $script:HealthPath -Value $health -Encoding UTF8
    } catch {}
}

function Invoke-AwJsonPost {
    param(
        [Parameter(Mandatory = $true)][string]$Uri,
        [Parameter(Mandatory = $true)][string]$Json,
        [int]$MaxAttempts = 5,
        [int]$InitialBackoffMs = 500
    )
    $attempt = 1
    $backoff = [Math]::Max(100, $InitialBackoffMs)
    while ($attempt -le $MaxAttempts) {
        $httpClient = $null
        try {
            $httpClient = New-Object System.Net.Http.HttpClient
            $content = New-Object System.Net.Http.StringContent($Json, [System.Text.Encoding]::UTF8, "application/json")
            $response = $httpClient.PostAsync($Uri, $content).Result
            if ($response.IsSuccessStatusCode) { return $true }
            if ($attempt -ge $MaxAttempts) {
                if (-not $script:WalFlushing) { Add-WalEntry -Uri $Uri -Json $Json }
                return $false
            }
        } catch {
            if ($attempt -ge $MaxAttempts) {
                if (-not $script:WalFlushing) { Add-WalEntry -Uri $Uri -Json $Json }
                return $false
            }
        } finally {
            if ($null -ne $httpClient) {
                $httpClient.Dispose()
            }
        }
        Start-Sleep -Milliseconds $backoff
        $backoff = [Math]::Min($backoff * 2, 10000)
        $attempt++
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

    Invoke-AwJsonPost -Uri "$($script:ApiBase)/buckets/$bucketId/heartbeat?pulsetime=15" -Json $payload
}

$config = Get-DeploymentConfig -Path $ConfigPath
if (-not $config) { throw "Configuration file not found: $ConfigPath" }
$script:Hostname = if ($config.PSObject.Properties.Name -contains 'awHostname' -and -not [string]::IsNullOrWhiteSpace([string]$config.awHostname)) { [string]$config.awHostname } else { [string]$env:COMPUTERNAME }

$scheme = if ($ServerScheme) { $ServerScheme } elseif ($config.server.scheme) { $config.server.scheme } else { 'http' }
$hostName = if ($ServerHost) { $ServerHost } elseif ($config.server.host) { $config.server.host } else { 'localhost' }
$port = if ($ServerPort) { $ServerPort } elseif ($config.server.port) { $config.server.port } else { 5600 }
$script:ApiBase = "{0}://{1}:{2}/api/0" -f $scheme, $hostName, $port
$stateRoot = if ($config.paths -and $config.paths.stateRoot) { [string]$config.paths.stateRoot } else { 'C:\ProgramData\AWatch-rus' }
$script:WalPath = Join-Path $stateRoot 'wal-file-operations.ndjson'
$script:HealthPath = Join-Path $stateRoot 'health-file-operations.json'

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
    
    $watchers += $watcher
    $subscriptions += @($onChanged, $onDeleted, $onRenamed)
}

Write-FileCollectorLog "Collector started. Waiting for events..."

try {
    while ($true) {
        Flush-Wal
        Write-CollectorHealth -Status 'running'
        Start-Sleep -Seconds $PollSeconds
    }
}
finally {
    Write-FileCollectorLog "Stopping collector..."
    Write-CollectorHealth -Status 'stopped'
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
