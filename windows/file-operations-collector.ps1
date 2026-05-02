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

# Реестр известных бакетов
$script:KnownBuckets = @{}
$script:Hostname = $env:COMPUTERNAME
$script:SessionId = [System.Diagnostics.Process]::GetCurrentProcess().SessionId

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

function Invoke-AwJsonPost {
    param(
        [Parameter(Mandatory = $true)][string]$Uri,
        [Parameter(Mandatory = $true)][string]$Json
    )
    $bytes = [Text.Encoding]::UTF8.GetBytes($Json)
    Invoke-RestMethod -Method Post -Uri $Uri -ContentType 'application/json; charset=utf-8' -Body $bytes | Out-Null
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
    Invoke-AwJsonPost -Uri "$($script:ApiBase)/buckets/$BucketId" -Json $body
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
if (-not $config) { throw "Не найден конфигурационный файл: $ConfigPath" }

$scheme = if ($ServerScheme) { $ServerScheme } elseif ($config.server.scheme) { $config.server.scheme } else { 'http' }
$hostName = if ($ServerHost) { $ServerHost } elseif ($config.server.host) { $config.server.host } else { 'localhost' }
$port = if ($ServerPort) { $ServerPort } elseif ($config.server.port) { $config.server.port } else { 5600 }
$script:ApiBase = "{0}://{1}:{2}/api/0" -f $scheme, $hostName, $port

# Разрешение путей для мониторинга
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
    Write-FileCollectorLog "Нет доступных путей для мониторинга. Завершение."
    exit 0
}

$bucketId = 'aw-file-operations_' + $script:Hostname
Ensure-Bucket -BucketId $bucketId -ClientName 'aw-file-operations' -BucketType 'aw.file.operation'
Write-FileCollectorLog "Запуск мониторинга путей: $($resolvedPaths -join ', ')"

$watchers = @()
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
}

Write-FileCollectorLog "Коллектор запущен. Ожидание событий..."

try {
    while ($true) {
        Start-Sleep -Seconds $PollSeconds
    }
}
finally {
    Write-FileCollectorLog "Остановка коллектора..."
    foreach ($w in $watchers) {
        $w.EnableRaisingEvents = $false
        $w.Dispose()
    }
}
