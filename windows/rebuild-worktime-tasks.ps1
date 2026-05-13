[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\AWatch-rus\deployment-config.json',
    [string]$ModulePath = 'C:\Program Files\AWatch-rus\windows\ActivityWatch.Windows.Common.psm1'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module $ModulePath -Force

$config = Read-ActivityWatchDeploymentConfig -Path $ConfigPath
$launchScript = [string]$config.paths.launchScript
$recoveryScript = [string]$config.paths.recoveryScript
$sessionCollectorScript = if ($config.paths.PSObject.Properties.Name -contains 'sessionCollectorScript') {
    [string]$config.paths.sessionCollectorScript
} else {
    Join-Path ([string]$config.paths.stateRoot) 'worktime-session-collector.ps1'
}
$taskDefinitions = @($config.userTasks)
$recoveryTaskName = [string]$config.recovery.taskName

$collectorProcs = @(
    Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
        Where-Object {
            ($_.Name -ieq 'powershell.exe' -or $_.Name -ieq 'pwsh.exe') -and
            $_.CommandLine -and
            $_.CommandLine -match [Regex]::Escape($sessionCollectorScript)
        }
)

foreach ($proc in $collectorProcs) {
    try {
        Stop-Process -Id ([int]$proc.ProcessId) -Force -ErrorAction Stop
    }
    catch {
    }
}

Start-Sleep -Seconds 2

Write-ActivityWatchLaunchScript -Path $launchScript -ConfigPath $ConfigPath
Write-ActivityWatchRecoveryScript -Path $recoveryScript -ConfigPath $ConfigPath
Register-ActivityWatchUserTasks -TaskDefinitions $taskDefinitions -LaunchScriptPath $launchScript -ConfigPath $ConfigPath
Register-ActivityWatchRecoveryTask -TaskName $recoveryTaskName -RecoveryScriptPath $recoveryScript -ConfigPath $ConfigPath
Start-ActivityWatchTasks -TaskDefinitions $taskDefinitions -RecoveryTaskName $recoveryTaskName

Start-Sleep -Seconds 3

$running = @(
    Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
        Where-Object {
            ($_.Name -ieq 'powershell.exe' -or $_.Name -ieq 'pwsh.exe') -and
            $_.CommandLine -and
            $_.CommandLine -match [Regex]::Escape($sessionCollectorScript)
        } |
        Select-Object Name, ProcessId, SessionId, CommandLine
)

[pscustomobject]@{
    rebuiltAtUtc = (Get-Date).ToUniversalTime().ToString('o')
    launchScript = $launchScript
    recoveryScript = $recoveryScript
    sessionCollectorScript = $sessionCollectorScript
    runningSessionCollectors = @($running)
}
