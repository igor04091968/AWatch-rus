[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\ActivityWatch\deployment-config.json'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$modulePath = Join-Path $PSScriptRoot 'ActivityWatch.Windows.Common.psm1'
Import-Module $modulePath -Force

$config = Read-ActivityWatchDeploymentConfig -Path $ConfigPath
$installRoot = [string]$config.paths.installRoot
$stateRoot = [string]$config.paths.stateRoot
$collectorScript = [string]$config.paths.collectorScript
$rulesPath = [string]$config.paths.rulesPath
$launchScript = [string]$config.paths.launchScript
$recoveryScript = [string]$config.paths.recoveryScript

$requiredFiles = @(
    (Join-Path $installRoot 'aw-watcher-afk\aw-watcher-afk.exe'),
    (Join-Path $installRoot 'aw-watcher-window\aw-watcher-window.exe'),
    $collectorScript,
    $rulesPath,
    $launchScript,
    $recoveryScript,
    $ConfigPath
)

$missingFiles = @(
    $requiredFiles | Where-Object { -not (Test-Path -LiteralPath $_) }
)

$processNames = @('aw-watcher-afk', 'aw-watcher-window')
$runningProcesses = Get-Process -Name $processNames -ErrorAction SilentlyContinue | Select-Object Name, Id, SessionId

$taskNames = @()
if ($config.userTasks) {
    $taskNames += @($config.userTasks | ForEach-Object { [string]$_.launchTaskName })
}
$taskNames += [string]$config.recovery.taskName
$taskNames = $taskNames | Sort-Object -Unique

$tasks = foreach ($taskName in $taskNames) {
    $task = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
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
            state = 'Missing'
            present = $false
        }
    }
}

$serverUrl = '{0}://{1}:{2}' -f [string]$config.server.scheme, [string]$config.server.host, [int]$config.server.port
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
        list = @($runningProcesses)
        ok = [bool](($runningProcesses | Select-Object -ExpandProperty Name -Unique).Count -ge 2)
    }
}

$result.overallOk = [bool]($result.files.ok -and $result.tasks.ok -and $result.processes.ok)

$result
