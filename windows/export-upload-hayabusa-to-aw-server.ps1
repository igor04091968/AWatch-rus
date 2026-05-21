[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\AWatch-rus\deployment-config.json',
    [int]$DaysBack = 1,
    [string]$ServerHost = '10.10.10.13',
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

$exportScript = 'C:\ProgramData\AWatch-rus\export-evtx-for-hayabusa.ps1'
if (-not (Test-Path -LiteralPath $exportScript)) {
    throw "export script not found: $exportScript"
}
if (-not (Test-Path -LiteralPath $RemoteKeyPath)) {
    throw "SSH private key not found: $RemoteKeyPath"
}

$export = powershell.exe -ExecutionPolicy Bypass -File $exportScript -ConfigPath $ConfigPath -DaysBack $DaysBack | ConvertFrom-Json
$zipPath = [string]$export.zipPath
$hostName = [string]$export.hostname
if ([string]::IsNullOrWhiteSpace($zipPath) -or -not (Test-Path -LiteralPath $zipPath)) {
    throw "zipPath missing or not found after export: $zipPath"
}

$zipName = Split-Path -Leaf $zipPath
$remoteTarget = "$ServerUser@$ServerHost`:$RemoteDropDir/"
$baseName = [System.IO.Path]::GetFileNameWithoutExtension($zipPath)
$caseIdPath = Join-Path ([System.IO.Path]::GetDirectoryName($zipPath)) ($baseName + '.caseid')

& scp.exe -i $RemoteKeyPath -o StrictHostKeyChecking=no -o UserKnownHostsFile=NUL $zipPath $remoteTarget
if ($LASTEXITCODE -ne 0) {
    throw "scp upload failed with rc=$LASTEXITCODE"
}
if ($CaseId.HasValue) {
    Set-Content -LiteralPath $caseIdPath -Value ([string]$CaseId.Value) -Encoding ASCII
    & scp.exe -i $RemoteKeyPath -o StrictHostKeyChecking=no -o UserKnownHostsFile=NUL $caseIdPath $remoteTarget
    if ($LASTEXITCODE -ne 0) {
        throw "scp caseid upload failed with rc=$LASTEXITCODE"
    }
}

$result = [ordered]@{
    exportedZip = $zipPath
    uploadedTo = "$RemoteDropDir/$zipName"
    caseIdSidecar = if ($CaseId.HasValue) { "$RemoteDropDir/$baseName.caseid" } else { $null }
    hostname = $hostName
    mode = $Mode
    runRemote = [bool]$RunRemote
}

if ($RunRemote) {
    $accept = "sudo /usr/local/bin/aw-hayabusa accept --package $RemoteDropDir/$zipName --host $hostName"
    $process = "sudo /usr/local/bin/aw-hayabusa process-inbox --mode $Mode --limit 1"
    $link = if ($CaseId.HasValue -and -not $NoLink) {
        " && sudo /usr/local/bin/aw-hayabusa-link-case --case-id $($CaseId.Value) --mode $Mode --link-source windows-direct-upload"
    } else {
        ''
    }
    $remoteCmd = "$accept && $process$link && sudo cat /opt/hayabusa/state/latest-intake.json"
    $remoteOut = & ssh.exe -i $RemoteKeyPath -o StrictHostKeyChecking=no -o UserKnownHostsFile=NUL "$ServerUser@$ServerHost" $remoteCmd
    if ($LASTEXITCODE -ne 0) {
        throw "remote Hayabusa run failed with rc=$LASTEXITCODE"
    }
    $result.remoteOutput = $remoteOut
}

$result | ConvertTo-Json -Depth 8
