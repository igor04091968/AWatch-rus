[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\AWatch-rus\deployment-config.json',
    [string]$OutputRoot,
    [int]$RetentionDays,
    [string[]]$Channels,
    [int]$DaysBack = 3,
    [switch]$NoZip
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function New-Directory {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -ItemType Directory -Path $Path -Force | Out-Null
    }
}

function Get-ConfigValue {
    param(
        [object]$Config,
        [string]$Section,
        [string]$Name,
        $DefaultValue
    )
    if ($null -eq $Config) { return $DefaultValue }
    if ($Config.PSObject.Properties.Name -notcontains $Section) { return $DefaultValue }
    $sectionValue = $Config.$Section
    if ($null -eq $sectionValue) { return $DefaultValue }
    if ($sectionValue.PSObject.Properties.Name -notcontains $Name) { return $DefaultValue }
    return $sectionValue.$Name
}

$config = $null
if (Test-Path -LiteralPath $ConfigPath) {
    $config = Get-Content -LiteralPath $ConfigPath -Raw | ConvertFrom-Json
}

$effectiveOutputRoot = if ($OutputRoot) {
    $OutputRoot
} else {
    [string](Get-ConfigValue -Config $config -Section 'forensics' -Name 'evtxExportRoot' -DefaultValue 'C:\ProgramData\AWatch-rus\forensics\evtx-exports')
}
$effectiveRetentionDays = if ($PSBoundParameters.ContainsKey('RetentionDays')) {
    $RetentionDays
} else {
    [int](Get-ConfigValue -Config $config -Section 'forensics' -Name 'retentionDays' -DefaultValue 14)
}
$effectiveChannels = if ($Channels -and $Channels.Count -gt 0) {
    @($Channels)
} else {
    @(Get-ConfigValue -Config $config -Section 'forensics' -Name 'evtxChannels' -DefaultValue @(
        'Security',
        'System',
        'Application',
        'Microsoft-Windows-PowerShell/Operational',
        'Microsoft-Windows-TerminalServices-LocalSessionManager/Operational',
        'Microsoft-Windows-TerminalServices-RemoteConnectionManager/Operational'
    ))
}

New-Directory -Path $effectiveOutputRoot

$hostName = if ($config -and $config.PSObject.Properties.Name -contains 'awHostname' -and -not [string]::IsNullOrWhiteSpace([string]$config.awHostname)) {
    [string]$config.awHostname
} else {
    [string]$env:COMPUTERNAME
}

$timestamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$batchRoot = Join-Path $effectiveOutputRoot "$hostName-$timestamp"
$evtxRoot = Join-Path $batchRoot 'evtx'
$metaPath = Join-Path $batchRoot 'manifest.json'
$zipPath = Join-Path $effectiveOutputRoot "$hostName-$timestamp.zip"

New-Directory -Path $batchRoot
New-Directory -Path $evtxRoot

$daysBackMs = [int64]$DaysBack * 24 * 60 * 60 * 1000
$query = "*[System[TimeCreated[timediff(@SystemTime) <= $daysBackMs]]]"
$results = @()

foreach ($channel in @($effectiveChannels | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) })) {
    $safeName = (($channel -replace '[\\/:*?""<>| ]', '_').Trim('_'))
    $targetPath = Join-Path $evtxRoot ($safeName + '.evtx')
    try {
        & wevtutil.exe epl $channel $targetPath /ow:true /q:$query | Out-Null
        $exists = Test-Path -LiteralPath $targetPath
        $size = if ($exists) { (Get-Item -LiteralPath $targetPath).Length } else { 0 }
        $results += [pscustomobject]@{
            channel = $channel
            path = $targetPath
            exported = $exists
            size = $size
            status = if ($exists) { 'ok' } else { 'empty' }
        }
    }
    catch {
        $results += [pscustomobject]@{
            channel = $channel
            path = $targetPath
            exported = $false
            size = 0
            status = 'error'
            error = $_.Exception.Message
        }
    }
}

$manifest = [ordered]@{
    generatedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
    hostname = $hostName
    configPath = $ConfigPath
    outputRoot = $effectiveOutputRoot
    batchRoot = $batchRoot
    zipPath = if ($NoZip) { $null } else { $zipPath }
    daysBack = $DaysBack
    retentionDays = $effectiveRetentionDays
    channels = @($effectiveChannels)
    exports = @($results)
}
$manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $metaPath -Encoding UTF8

if (-not $NoZip) {
    if (Test-Path -LiteralPath $zipPath) {
        Remove-Item -LiteralPath $zipPath -Force -ErrorAction SilentlyContinue
    }
    Compress-Archive -Path (Join-Path $batchRoot '*') -DestinationPath $zipPath -Force
}

$cutoff = (Get-Date).AddDays(-1 * [Math]::Max(1, $effectiveRetentionDays))
Get-ChildItem -LiteralPath $effectiveOutputRoot -Directory -ErrorAction SilentlyContinue |
    Where-Object { $_.LastWriteTime -lt $cutoff } |
    ForEach-Object { Remove-Item -LiteralPath $_.FullName -Recurse -Force -ErrorAction SilentlyContinue }
Get-ChildItem -LiteralPath $effectiveOutputRoot -File -Filter '*.zip' -ErrorAction SilentlyContinue |
    Where-Object { $_.LastWriteTime -lt $cutoff } |
    ForEach-Object { Remove-Item -LiteralPath $_.FullName -Force -ErrorAction SilentlyContinue }

$manifest
