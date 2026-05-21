[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\AWatch-rus\deployment-config.json',
    [Nullable[int]]$HoursBack,
    [int]$DaysBack = 1,
    [string]$ServerHost = '',
    [string]$ServerUser = 'awops',
    [string]$RemoteDropDir = '/opt/activitywatch/aw-rus-ops/drop',
    [string]$RemoteKeyPath = 'C:\ProgramData\AWatch-rus\ssh\awops_ed25519',
    [string]$Mode = 'incident',
    [Nullable[int]]$CaseId,
    [switch]$RunRemote,
    [switch]$NoLink
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function New-TemporarySshKeyCopy {
    param(
        [Parameter(Mandatory = $true)]
        [string]$SourceKeyPath
    )

    $tempDir = Join-Path $env:TEMP 'aw-rus-hayabusa-ssh'
    New-Item -ItemType Directory -Path $tempDir -Force | Out-Null
    $tempKeyPath = Join-Path $tempDir 'awops_ed25519'
    Copy-Item -LiteralPath $SourceKeyPath -Destination $tempKeyPath -Force

    & icacls.exe $tempKeyPath /inheritance:r | Out-Null
    & icacls.exe $tempKeyPath /grant:r "$($env:USERNAME):(F)" | Out-Null
    & icacls.exe $tempKeyPath /remove:g 'Users' 'Authenticated Users' 'Everyone' 'BUILTIN\Users' 'BUILTIN\Administrators' 'NT AUTHORITY\SYSTEM' 2>$null | Out-Null

    return $tempKeyPath
}

$exportScript = 'C:\ProgramData\AWatch-rus\export-evtx-for-hayabusa.ps1'
if (-not (Test-Path -LiteralPath $exportScript)) {
    throw "export script not found: $exportScript"
}
if (-not (Test-Path -LiteralPath $RemoteKeyPath)) {
    throw "SSH private key not found: $RemoteKeyPath"
}

$config = Get-Content -Raw -LiteralPath $ConfigPath | ConvertFrom-Json
if ([string]::IsNullOrWhiteSpace($ServerHost)) {
    $ServerHost = [string]$config.server.host
}
if ([string]::IsNullOrWhiteSpace($ServerHost)) {
    throw "ServerHost is empty and deployment-config has no server.host: $ConfigPath"
}

$exportArgs = @{
    ConfigPath = $ConfigPath
}
if ($null -ne $HoursBack) {
    $exportArgs.HoursBack = [int]$HoursBack
} else {
    $exportArgs.DaysBack = $DaysBack
}
$export = & $exportScript @exportArgs
$zipPath = [string]$export.zipPath
$hostName = [string]$export.hostname
if ([string]::IsNullOrWhiteSpace($zipPath) -or -not (Test-Path -LiteralPath $zipPath)) {
    throw "zipPath missing or not found after export: $zipPath"
}

$zipName = Split-Path -Leaf $zipPath
$remoteTarget = "$ServerUser@$ServerHost`:$RemoteDropDir/"
$baseName = [System.IO.Path]::GetFileNameWithoutExtension($zipPath)
$caseIdPath = Join-Path ([System.IO.Path]::GetDirectoryName($zipPath)) ($baseName + '.caseid')
$metaPath = Join-Path ([System.IO.Path]::GetDirectoryName($zipPath)) ($baseName + '.meta.json')
$effectiveKeyPath = New-TemporarySshKeyCopy -SourceKeyPath $RemoteKeyPath

try {
    $meta = [ordered]@{
        host = $hostName
        mode = $Mode
        link_source = 'windows-drop-upload'
    }
    if ($null -ne $CaseId) {
        $meta.case_id = [int]$CaseId
    }
    $meta | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $metaPath -Encoding UTF8
    & scp.exe -i $effectiveKeyPath -o StrictHostKeyChecking=no -o UserKnownHostsFile=NUL $metaPath $remoteTarget
    if ($LASTEXITCODE -ne 0) {
        throw "scp meta upload failed with rc=$LASTEXITCODE"
    }
    if ($null -ne $CaseId) {
        Set-Content -LiteralPath $caseIdPath -Value ([string]$CaseId) -Encoding ASCII
        & scp.exe -i $effectiveKeyPath -o StrictHostKeyChecking=no -o UserKnownHostsFile=NUL $caseIdPath $remoteTarget
        if ($LASTEXITCODE -ne 0) {
            throw "scp caseid upload failed with rc=$LASTEXITCODE"
        }
    }
    & scp.exe -i $effectiveKeyPath -o StrictHostKeyChecking=no -o UserKnownHostsFile=NUL $zipPath $remoteTarget
    if ($LASTEXITCODE -ne 0) {
        throw "scp upload failed with rc=$LASTEXITCODE"
    }
}
finally {
    Remove-Item -LiteralPath $effectiveKeyPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $metaPath -Force -ErrorAction SilentlyContinue
}

$result = [ordered]@{
    exportedZip = $zipPath
    uploadedTo = "$RemoteDropDir/$zipName"
    caseIdSidecar = if ($null -ne $CaseId) { "$RemoteDropDir/$baseName.caseid" } else { $null }
    metaSidecar = "$RemoteDropDir/$baseName.meta.json"
    hostname = $hostName
    mode = $Mode
    runRemote = [bool]$RunRemote
}

if ($RunRemote) {
    $accept = "sudo /usr/local/bin/aw-hayabusa accept --package $RemoteDropDir/$zipName --host $hostName"
    $process = "sudo /usr/local/bin/aw-hayabusa process-inbox --mode $Mode --limit 1"
    $link = if (($null -ne $CaseId) -and -not $NoLink) {
        " && sudo /usr/local/bin/aw-hayabusa-link-case --case-id $CaseId --mode $Mode --link-source windows-direct-upload"
    } else {
        ''
    }
    $remoteCmd = "$accept && $process$link && sudo cat /opt/hayabusa/state/latest-intake.json"
    $effectiveKeyPath = New-TemporarySshKeyCopy -SourceKeyPath $RemoteKeyPath
    try {
        $remoteOut = & ssh.exe -i $effectiveKeyPath -o StrictHostKeyChecking=no -o UserKnownHostsFile=NUL "$ServerUser@$ServerHost" $remoteCmd
        if ($LASTEXITCODE -ne 0) {
            throw "remote Hayabusa run failed with rc=$LASTEXITCODE"
        }
    }
    finally {
        Remove-Item -LiteralPath $effectiveKeyPath -Force -ErrorAction SilentlyContinue
    }
    $result.remoteOutput = $remoteOut
}

$result | ConvertTo-Json -Depth 8
