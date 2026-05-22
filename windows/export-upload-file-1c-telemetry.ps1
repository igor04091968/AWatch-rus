[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\AWatch-rus\deployment-config.json',
    [string]$AnalyticsHost = '',
    [string]$AnalyticsUser = 'igor',
    [string]$RemoteRoot = '/opt/activitywatch/clickhouse-1c/landing',
    [string]$RemoteKeyPath = 'C:\ProgramData\AWatch-rus\ssh\awops_ed25519'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function New-TemporarySshKeyCopy {
    param(
        [Parameter(Mandatory = $true)]
        [string]$SourceKeyPath
    )

    $tempDir = Join-Path $env:TEMP 'aw-rus-1c-ssh'
    New-Item -ItemType Directory -Path $tempDir -Force | Out-Null
    $tempKeyPath = Join-Path $tempDir 'awops_ed25519'
    Copy-Item -LiteralPath $SourceKeyPath -Destination $tempKeyPath -Force

    & icacls.exe $tempKeyPath /inheritance:r | Out-Null
    & icacls.exe $tempKeyPath /grant:r "$($env:USERNAME):(F)" | Out-Null
    & icacls.exe $tempKeyPath /remove:g 'Users' 'Authenticated Users' 'Everyone' 'BUILTIN\Users' 'BUILTIN\Administrators' 'NT AUTHORITY\SYSTEM' 2>$null | Out-Null

    return $tempKeyPath
}

function Get-1CFileInfobases {
    $results = New-Object System.Collections.Generic.List[object]
    $launcherFiles = Get-ChildItem -Path 'C:\Users' -Directory -ErrorAction SilentlyContinue |
        ForEach-Object { Join-Path $_.FullName 'AppData\Roaming\1C\1CEStart\ibases.v8i' } |
        Where-Object { Test-Path -LiteralPath $_ }

    foreach ($file in $launcherFiles) {
        $userName = Split-Path -Leaf (Split-Path -Parent (Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $file))))
        $currentName = $null
        $currentId = $null
        foreach ($lineRaw in Get-Content -LiteralPath $file -Encoding UTF8) {
            $line = [string]$lineRaw
            if ($line -match '^\[(.+)\]$') {
                $currentName = $Matches[1]
                $currentId = $null
                continue
            }
            if ($line -match '^ID=(.+)$') {
                $currentId = $Matches[1].Trim()
                continue
            }
            if ($line -match '^Connect=File="(.+)";$' -and $currentName) {
                $results.Add([pscustomobject]@{
                    userName = $userName
                    infobase = $currentName
                    baseId = $currentId
                    path = $Matches[1]
                    launcherFile = $file
                })
            }
        }
    }

    return $results |
        Group-Object infobase, path |
        ForEach-Object { $_.Group | Select-Object -First 1 }
}

function Get-HostSample {
    $cpu = (Get-Counter '\Processor(_Total)\% Processor Time').CounterSamples.CookedValue
    $os = Get-CimInstance Win32_OperatingSystem
    $disk = Get-PSDrive -Name E -ErrorAction SilentlyContinue
    $rdp = (quser 2>$null | Select-Object -Skip 1 | Measure-Object).Count

    return [ordered]@{
        ts = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
        host = $env:COMPUTERNAME
        cpu_pct = [math]::Round($cpu, 2)
        ram_pct = [math]::Round((($os.TotalVisibleMemorySize - $os.FreePhysicalMemory) / $os.TotalVisibleMemorySize) * 100, 2)
        disk_free_gb = if ($disk) { [math]::Round($disk.Free / 1GB, 2) } else { 0 }
        disk_latency_ms = 0
        smb_errors = 0
        rdp_sessions = $rdp
        backup_ok = 1
    }
}

function Write-JsonLines {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [Parameter(Mandatory = $true)]
        [object]$Rows
    )

    $directory = Split-Path -Parent $Path
    if ($directory) {
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }
    $normalizedRows = @()
    if ($null -ne $Rows) {
        $normalizedRows = @($Rows)
    }
    $normalizedRows |
        ForEach-Object { $_ | ConvertTo-Json -Depth 8 -Compress } |
        Set-Content -LiteralPath $Path -Encoding UTF8
}

if (-not (Test-Path -LiteralPath $RemoteKeyPath)) {
    throw "SSH private key not found: $RemoteKeyPath"
}

$config = Get-Content -Raw -LiteralPath $ConfigPath | ConvertFrom-Json
if ([string]::IsNullOrWhiteSpace($AnalyticsHost)) {
    if ($config.PSObject.Properties.Name -contains 'analytics' -and
        $config.analytics.PSObject.Properties.Name -contains 'file1cAutomation' -and
        $config.analytics.file1cAutomation.PSObject.Properties.Name -contains 'targetHost') {
        $AnalyticsHost = [string]$config.analytics.file1cAutomation.targetHost
    }
}
if ([string]::IsNullOrWhiteSpace($AnalyticsHost)) {
    throw "AnalyticsHost is empty and deployment-config has no analytics.file1cAutomation.targetHost"
}
if ($config.PSObject.Properties.Name -contains 'analytics' -and
    $config.analytics.PSObject.Properties.Name -contains 'file1cAutomation' -and
    $config.analytics.file1cAutomation.PSObject.Properties.Name -contains 'targetUser' -and
    -not [string]::IsNullOrWhiteSpace([string]$config.analytics.file1cAutomation.targetUser)) {
    $AnalyticsUser = [string]$config.analytics.file1cAutomation.targetUser
}
if ($config.PSObject.Properties.Name -contains 'analytics' -and
    $config.analytics.PSObject.Properties.Name -contains 'file1cAutomation' -and
    $config.analytics.file1cAutomation.PSObject.Properties.Name -contains 'remoteRoot' -and
    -not [string]::IsNullOrWhiteSpace([string]$config.analytics.file1cAutomation.remoteRoot)) {
    $RemoteRoot = [string]$config.analytics.file1cAutomation.remoteRoot
}

$infobases = @(Get-1CFileInfobases)
$nowUtc = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')

$documents = New-Object System.Collections.Generic.List[object]
$reglog = New-Object System.Collections.Generic.List[object]
$audit = New-Object System.Collections.Generic.List[object]

foreach ($base in $infobases) {
    $dbFile = Join-Path $base.path '1Cv8.1CD'
    $dbItem = Get-Item -LiteralPath $dbFile -ErrorAction SilentlyContinue
    $logDir = Join-Path $base.path '1Cv8Log'
    $logItems = @(Get-ChildItem -LiteralPath $logDir -File -ErrorAction SilentlyContinue)
    $mainLog = $logItems | Where-Object { $_.Extension -ieq '.lgp' } | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    $activeLocks = @(Get-ChildItem -LiteralPath $base.path -File -Filter '1Cv8*.1CL*' -ErrorAction SilentlyContinue)
    $tempDb = Get-Item -LiteralPath (Join-Path $base.path '1Cv8tmp.1CD') -ErrorAction SilentlyContinue
    $schedulerDir = Get-Item -LiteralPath (Join-Path $base.path '1Cv8JobScheduler') -ErrorAction SilentlyContinue
    $owner = if ($base.userName) { [string]$base.userName } else { 'unknown' }
    $status = if ($activeLocks.Count -gt 0 -or $tempDb) { 'busy' } else { 'online' }
    $docId = if ($base.baseId) { [string]$base.baseId } else { ([Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes([string]$base.path)).TrimEnd('=').Replace('/','_').Replace('+','-')) }

    $documents.Add([ordered]@{
        ts = $nowUtc
        infobase = [string]$base.infobase
        organization = ''
        department = 'FileBase'
        doc_type = 'InfobaseSnapshot'
        doc_id = $docId
        doc_number = ''
        author = $owner
        counterparty = ''
        operation_type = 'inventory'
        amount = 0
        status = $status
        posted = 1
    })

    $audit.Add([ordered]@{
        ts = $nowUtc
        infobase = [string]$base.infobase
        user = $owner
        object_type = 'infobase'
        object_id = $docId
        action = 'inventory_snapshot'
        before_hash = ''
        after_hash = ''
        risk_tag = if ($status -eq 'busy') { 'busy' } else { '' }
    })

    if ($mainLog) {
        $reglog.Add([ordered]@{
            ts = ([datetime]$mainLog.LastWriteTimeUtc).ToString('yyyy-MM-ddTHH:mm:ssZ')
            infobase = [string]$base.infobase
            user = $owner
            host = $env:COMPUTERNAME
            app = '1cv8-file'
            event_name = 'RegLogInventory'
            level = if ($mainLog.Length -gt 536870912) { 'warn' } else { 'info' }
            duration_ms = 0
            message = "Registration log file $($mainLog.Name) size=$([math]::Round($mainLog.Length / 1MB, 2))MB path=$($mainLog.FullName)"
        })
    }

    if ($activeLocks.Count -gt 0 -or $tempDb) {
        $reglog.Add([ordered]@{
            ts = $nowUtc
            infobase = [string]$base.infobase
            user = $owner
            host = $env:COMPUTERNAME
            app = '1cv8-file'
            event_name = 'FileBaseBusy'
            level = 'warn'
            duration_ms = 0
            message = "Detected active file-base markers: locks=$($activeLocks.Count) tempDb=$([bool]$tempDb)"
        })
    }

    if ($schedulerDir) {
        $reglog.Add([ordered]@{
            ts = ([datetime]$schedulerDir.LastWriteTimeUtc).ToString('yyyy-MM-ddTHH:mm:ssZ')
            infobase = [string]$base.infobase
            user = $owner
            host = $env:COMPUTERNAME
            app = '1cv8-file'
            event_name = 'JobSchedulerActivity'
            level = 'info'
            duration_ms = 0
            message = "1Cv8JobScheduler touched at $([datetime]$schedulerDir.LastWriteTimeUtc)"
        })
    }
}

$hostRows = @((Get-HostSample))

$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$outRoot = Join-Path $env:TEMP "aw-rus-1c-outbox-$stamp"
New-Item -ItemType Directory -Path $outRoot -Force | Out-Null

$files = @{
    documents = Join-Path $outRoot "documents-$stamp.jsonl"
    reglog = Join-Path $outRoot "reglog-$stamp.jsonl"
    audit = Join-Path $outRoot "audit-$stamp.jsonl"
    host = Join-Path $outRoot "host-$stamp.jsonl"
}

$documentRows = @($documents | ForEach-Object { $_ })
$reglogRows = @($reglog | ForEach-Object { $_ })
$auditRows = @($audit | ForEach-Object { $_ })
$hostRowsNormalized = @($hostRows | ForEach-Object { $_ })

Write-JsonLines -Path ([string]$files['documents']) -Rows $documentRows
Write-JsonLines -Path ([string]$files['reglog']) -Rows $reglogRows
Write-JsonLines -Path ([string]$files['audit']) -Rows $auditRows
Write-JsonLines -Path ([string]$files['host']) -Rows $hostRowsNormalized

$effectiveKeyPath = New-TemporarySshKeyCopy -SourceKeyPath $RemoteKeyPath

try {
    foreach ($dataset in 'documents', 'reglog', 'audit', 'host') {
        & scp.exe -q -i $effectiveKeyPath -o LogLevel=ERROR -o StrictHostKeyChecking=no -o UserKnownHostsFile=NUL $files[$dataset] "$AnalyticsUser@$AnalyticsHost`:$RemoteRoot/$dataset/"
        if ($LASTEXITCODE -ne 0) {
            throw "scp upload failed for dataset $dataset with rc=$LASTEXITCODE"
        }
    }
}
finally {
    Remove-Item -LiteralPath $effectiveKeyPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $outRoot -Recurse -Force -ErrorAction SilentlyContinue
}

[ordered]@{
    analyticsHost = $AnalyticsHost
    analyticsUser = $AnalyticsUser
    remoteRoot = $RemoteRoot
    infobases = @($infobases | ForEach-Object { $_.infobase })
    datasets = [ordered]@{
        documents = $documents.Count
        reglog = $reglog.Count
        audit = $audit.Count
        host = $hostRows.Count
    }
} | ConvertTo-Json -Depth 8
