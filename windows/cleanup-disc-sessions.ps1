[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\AWatch-rus\deployment-config.json',
    [string]$ModulePath = 'C:\Program Files\AWatch-rus\windows\ActivityWatch.Windows.Common.psm1'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module $ModulePath -Force

$config = Read-ActivityWatchDeploymentConfig -Path $ConfigPath
$sessionRecords = Get-ActivityWatchSessionRecords
Stop-ActivityWatchProcessesInNonLiveSessions -SessionRecords $sessionRecords -Config $config

Write-Host 'QUSER'
quser
Write-Host '---'
Write-Host 'WATCHERS'
Get-Process aw-watcher-afk,aw-watcher-window -ErrorAction SilentlyContinue |
    Select-Object Name, Id, SessionId, StartTime |
    Sort-Object SessionId, Name |
    Format-Table -AutoSize
