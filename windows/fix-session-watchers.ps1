[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\AWatch-rus\deployment-config.json',
    [string]$ModulePath = 'C:\Program Files\AWatch-rus\windows\ActivityWatch.Windows.Common.psm1'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module $ModulePath -Force

$config = Read-ActivityWatchDeploymentConfig -Path $ConfigPath
$stateRoot = [string]$config.paths.stateRoot
$launchScriptPath = if ($config.paths.PSObject.Properties.Name -contains 'launchScript') { [string]$config.paths.launchScript } else { Join-Path $stateRoot 'launch-watchers.ps1' }
$recoveryScriptPath = if ($config.paths.PSObject.Properties.Name -contains 'recoveryScript') { [string]$config.paths.recoveryScript } else { Join-Path $stateRoot 'recovery-loop.ps1' }

Write-ActivityWatchLaunchScript -Path $launchScriptPath -ConfigPath $ConfigPath
Write-ActivityWatchRecoveryScript -Path $recoveryScriptPath -ConfigPath $ConfigPath
Write-ActivityWatchHiddenPowerShellWrapper -Path (Get-ActivityWatchHiddenLauncherPath -ScriptPath $launchScriptPath) -ScriptPath $launchScriptPath -ConfigPath $ConfigPath
Write-ActivityWatchHiddenPowerShellWrapper -Path (Get-ActivityWatchHiddenLauncherPath -ScriptPath $recoveryScriptPath) -ScriptPath $recoveryScriptPath -ConfigPath $ConfigPath

$sessionRecords = Get-ActivityWatchSessionRecords
Stop-ActivityWatchProcessesInNonLiveSessions -SessionRecords $sessionRecords -Config $config

Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
    Where-Object {
        ($_.Name -ieq 'powershell.exe' -or $_.Name -ieq 'pwsh.exe') -and
        $_.CommandLine -match 'recovery-loop\.ps1'
    } |
    ForEach-Object {
        Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue
    }

Start-Sleep -Seconds 2
schtasks /Run /TN 'ActivityWatch Recovery' | Out-Null
Start-Sleep -Seconds 3

Write-Host 'QUSER'
quser
Write-Host '---'
Write-Host 'WATCHERS'
Get-Process aw-watcher-afk,aw-watcher-window -ErrorAction SilentlyContinue |
    Select-Object Name, Id, SessionId, StartTime |
    Sort-Object SessionId, Name |
    Format-Table -AutoSize
