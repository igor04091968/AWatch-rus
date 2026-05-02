[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\AWatch-rus\deployment-config.json'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$modulePath = Join-Path $PSScriptRoot 'ActivityWatch.Windows.Common.psm1'
Import-Module $modulePath -Force

$config = Read-ActivityWatchDeploymentConfig -Path $ConfigPath
$installRoot = [string]$config.paths.installRoot
$stateRoot = [string]$config.paths.stateRoot
$collectorScript = [string]$config.paths.collectorScript
$endpointCollectorScript = if ($config.paths.PSObject.Properties.Name -contains 'endpointCollectorScript') { [string]$config.paths.endpointCollectorScript } else { Join-Path $stateRoot 'dlp-endpoint-signals-collector.ps1' }
$fileCollectorScript = if ($config.paths.PSObject.Properties.Name -contains 'fileCollectorScript') { [string]$config.paths.fileCollectorScript } else { Join-Path $stateRoot 'file-operations-collector.ps1' }
$sessionCollectorScript = if ($config.paths.PSObject.Properties.Name -contains 'sessionCollectorScript') { [string]$config.paths.sessionCollectorScript } else { Join-Path $stateRoot 'worktime-session-collector.ps1' }
$rulesPath = [string]$config.paths.rulesPath
$policyPath = if ($config.paths.PSObject.Properties.Name -contains 'policyPath') { [string]$config.paths.policyPath } else { Join-Path $stateRoot 'dlp-policy.json' }
$launchScript = [string]$config.paths.launchScript
$recoveryScript = [string]$config.paths.recoveryScript

$afkExpected = if ($config.PSObject.Properties.Name -contains 'collectors' -and $config.collectors.PSObject.Properties.Name -contains 'afkEnabled') { [bool]$config.collectors.afkEnabled } else { $true }
$windowExpected = if ($config.PSObject.Properties.Name -contains 'collectors' -and $config.collectors.PSObject.Properties.Name -contains 'windowEnabled') { [bool]$config.collectors.windowEnabled } else { $true }
$fileOpsExpected = if ($config.PSObject.Properties.Name -contains 'collectors' -and $config.collectors.PSObject.Properties.Name -contains 'fileOpsEnabled') { [bool]$config.collectors.fileOpsEnabled } else { $true }
$requiredFiles = @(
    $collectorScript,
    $endpointCollectorScript,
    $sessionCollectorScript,
    $rulesPath,
    $policyPath,
    $launchScript,
    $recoveryScript,
    $ConfigPath
)
if ($fileOpsExpected) {
    $requiredFiles += $fileCollectorScript
}
if ($afkExpected) {
    $requiredFiles += (Join-Path $installRoot 'aw-watcher-afk\aw-watcher-afk.exe')
}
if ($windowExpected) {
    $requiredFiles += (Join-Path $installRoot 'aw-watcher-window\aw-watcher-window.exe')
}

$missingFiles = @(
    $requiredFiles | Where-Object { -not (Test-Path -LiteralPath $_) }
)

$processNames = @()
if ($afkExpected) { $processNames += 'aw-watcher-afk' }
if ($windowExpected) { $processNames += 'aw-watcher-window' }
$runningProcesses = @()
if ($processNames.Count -gt 0) {
    $runningProcesses = Get-Process -Name $processNames -ErrorAction SilentlyContinue | Select-Object Name, Id, SessionId
}
$sessionCollectorProcesses = @(
    Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
        Where-Object {
            ($_.Name -ieq 'powershell.exe' -or $_.Name -ieq 'pwsh.exe') -and
            $_.CommandLine -match [Regex]::Escape($sessionCollectorScript)
        } |
        Select-Object Name, ProcessId, SessionId, CommandLine
)

$taskNames = @()
if ($config.userTasks) {
    $taskNames += @($config.userTasks | ForEach-Object { [string]$_.launchTaskName })
}
$taskNames += [string]$config.recovery.taskName
$taskNames = $taskNames | Sort-Object -Unique

$tasks = @(
    foreach ($taskName in $taskNames) {
        $task = Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object { $_.TaskName -eq $taskName } | Select-Object -First 1
        if ($task) {
            [pscustomobject]@{
                taskName = $task.TaskName
                state = [string]$task.State
                present = $true
            }
        }
        else {
            [pscustomobject]@{
                taskName = $taskName
                state = 'Отсутствует'
                present = $false
            }
        }
    }
)

$serverUrl = '{0}://{1}:{2}' -f [string]$config.server.scheme, [string]$config.server.host, [int]$config.server.port
$uniqueRunningProcessNames = @($runningProcesses | Select-Object -ExpandProperty Name -Unique)
$result = [ordered]@{
    generatedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
    configPath = $ConfigPath
    serverUrl = $serverUrl
    installRoot = $installRoot
    stateRoot = $stateRoot
    files = [ordered]@{
        required = $requiredFiles
        missing = $missingFiles
        ok = ($missingFiles.Count -eq 0)
    }
    tasks = [ordered]@{
        list = $tasks
        ok = [bool]($tasks.Count -gt 0 -and -not ($tasks | Where-Object { -not $_.present }))
    }
    processes = [ordered]@{
        expected = $processNames
        list = @($runningProcesses)
        sessionCollectors = @($sessionCollectorProcesses)
        ok = [bool](
            (
                ($processNames.Count -eq 0) -or
                ($uniqueRunningProcessNames.Count -ge $processNames.Count)
            ) -and
            ($sessionCollectorProcesses.Count -ge 1)
        )
    }
}

$result.overallOk = [bool]($result.files.ok -and $result.tasks.ok -and $result.processes.ok)

$result
