[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [string]$OldInstallRoot = 'C:\Program Files\ActivityWatch-Phase2',
    [string]$OldStateRoot = 'C:\ProgramData\ActivityWatch-Phase2',
    [string]$NewInstallRoot = 'C:\Program Files\AWatch-rus\bin',
    [string]$NewStateRoot = 'C:\ProgramData\AWatch-rus',
    [string]$ToolkitRoot = 'C:\Program Files\AWatch-rus\windows',
    [switch]$SkipValidation
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$modulePath = Join-Path $PSScriptRoot 'ActivityWatch.Windows.Common.psm1'
Import-Module $modulePath -Force

Assert-Administrator

function Copy-DirectoryContents {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Source,
        [Parameter(Mandatory = $true)]
        [string]$Destination
    )

    if (-not (Test-Path -LiteralPath $Source)) {
        return
    }

    New-ActivityWatchDirectory -Path $Destination
    Copy-Item -Path (Join-Path $Source '*') -Destination $Destination -Recurse -Force
}

function Copy-IfExists {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Source,
        [Parameter(Mandatory = $true)]
        [string]$Destination
    )

    if (Test-Path -LiteralPath $Source) {
        Copy-Item -LiteralPath $Source -Destination $Destination -Force
    }
}

function Convert-PathValue {
    param(
        [AllowNull()]
        [string]$Value
    )

    if ([string]::IsNullOrWhiteSpace($Value)) {
        return $Value
    }

    return $Value.Replace($OldInstallRoot, $NewInstallRoot).Replace($OldStateRoot, $NewStateRoot)
}

function Stop-AWatchTaskSet {
    foreach ($task in @(Get-ScheduledTask | Where-Object { $_.TaskName -eq 'ActivityWatch Recovery' -or $_.TaskName -like 'ActivityWatch Launch *' })) {
        Stop-ScheduledTask -TaskName $task.TaskName -ErrorAction SilentlyContinue
    }
}

function Get-ExistingAWatchConfig {
    $newConfigPath = Join-Path $NewStateRoot 'deployment-config.json'
    $oldConfigPath = Join-Path $OldStateRoot 'deployment-config.json'

    if (Test-Path -LiteralPath $oldConfigPath) {
        return [pscustomobject]@{
            Path   = $oldConfigPath
            Config = Read-ActivityWatchDeploymentConfig -Path $oldConfigPath
        }
    }

    if (Test-Path -LiteralPath $newConfigPath) {
        return [pscustomobject]@{
            Path   = $newConfigPath
            Config = Read-ActivityWatchDeploymentConfig -Path $newConfigPath
        }
    }

    throw "Не найден deployment-config.json ни в $OldStateRoot, ни в $NewStateRoot."
}

function Update-AWatchConfigPaths {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject]$Config
    )

    $logsRoot = Join-Path $NewStateRoot 'logs'
    $Config.paths.installRoot = $NewInstallRoot
    $Config.paths.stateRoot = $NewStateRoot
    $Config.paths.logsRoot = $logsRoot
    $Config.paths.collectorScript = Join-Path $NewStateRoot 'browser-domains-native-collector.ps1'
    $Config.paths.endpointCollectorScript = Join-Path $NewStateRoot 'dlp-endpoint-signals-collector.ps1'
    if ($Config.paths.PSObject.Properties.Name -contains 'sessionCollectorScript') {
        $Config.paths.sessionCollectorScript = Join-Path $NewStateRoot 'worktime-session-collector.ps1'
    }
    $Config.paths.rulesPath = Join-Path $NewStateRoot 'web-category-rules.json'
    if ($Config.paths.PSObject.Properties.Name -contains 'policyPath') {
        $Config.paths.policyPath = Join-Path $NewStateRoot 'dlp-policy.json'
    }
    $Config.paths.launchScript = Join-Path $NewStateRoot 'launch-watchers.ps1'
    $Config.paths.recoveryScript = Join-Path $NewStateRoot 'recovery-loop.ps1'

    if ($Config.PSObject.Properties.Name -contains 'incidentCapture' -and $Config.incidentCapture.PSObject.Properties.Name -contains 'artifactsRoot') {
        $Config.incidentCapture.artifactsRoot = Convert-PathValue -Value ([string]$Config.incidentCapture.artifactsRoot)
    }

    return $Config
}

$existing = Get-ExistingAWatchConfig
$backupRoot = Join-Path $NewStateRoot ('migration-backups\' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
$newConfigPath = Join-Path $NewStateRoot 'deployment-config.json'
$newLogsRoot = Join-Path $NewStateRoot 'logs'

$summary = [ordered]@{
    sourceConfig = $existing.Path
    oldInstallRoot = $OldInstallRoot
    oldStateRoot = $OldStateRoot
    newInstallRoot = $NewInstallRoot
    newStateRoot = $NewStateRoot
    backupRoot = $backupRoot
    actions = @(
        'stop ActivityWatch scheduled tasks',
        'backup old/new install and state directories',
        'copy old install/state contents to AWatch-rus paths',
        'rewrite deployment-config.json paths',
        'regenerate launcher/recovery scripts',
        're-register scheduled tasks',
        'run validate-deployment.ps1'
    )
}

if ($WhatIfPreference) {
    return [pscustomobject]$summary
}

if ($PSCmdlet.ShouldProcess($env:COMPUTERNAME, 'Миграция ActivityWatch Windows/RDP путей в AWatch-rus')) {
    New-ActivityWatchDirectory -Path $NewStateRoot
    New-ActivityWatchDirectory -Path $backupRoot

    Stop-AWatchTaskSet

    foreach ($item in @(
            @{ Source = $OldInstallRoot; Name = 'old-install' },
            @{ Source = $OldStateRoot; Name = 'old-state' },
            @{ Source = $NewInstallRoot; Name = 'new-install' },
            @{ Source = $NewStateRoot; Name = 'new-state' }
        )) {
        if (Test-Path -LiteralPath $item.Source) {
            $backupDest = Join-Path $backupRoot $item.Name
            New-ActivityWatchDirectory -Path $backupDest

            $excludeDirs = @()
            if ($item.Source -eq $NewStateRoot) {
                # Avoid infinite recursion: backupRoot is inside NewStateRoot by default.
                $excludeDirs += $backupRoot
            }

            $robocopyArgs = @(
                $item.Source,
                $backupDest,
                '/E',
                '/R:1',
                '/W:1',
                '/NFL',
                '/NDL',
                '/NJH',
                '/NJS',
                '/NP'
            )
            if ($excludeDirs.Count -gt 0) {
                $robocopyArgs += '/XD'
                $robocopyArgs += $excludeDirs
            }

            & robocopy @robocopyArgs | Out-Null
            if ($LASTEXITCODE -ge 8) {
                throw "Backup robocopy failed (exit=$LASTEXITCODE) for source '$($item.Source)' to '$backupDest'"
            }
        }
    }

    Copy-DirectoryContents -Source $OldInstallRoot -Destination $NewInstallRoot
    Copy-DirectoryContents -Source $OldStateRoot -Destination $NewStateRoot
    New-ActivityWatchDirectory -Path $newLogsRoot

    foreach ($file in @(
            'browser-domains-native-collector.ps1',
            'dlp-endpoint-signals-collector.ps1',
            'worktime-session-collector.ps1',
            'web-category-rules.example.json',
            'dlp-policy.example.json'
        )) {
        Copy-IfExists -Source (Join-Path $ToolkitRoot $file) -Destination (Join-Path $NewStateRoot $file)
    }

    Copy-IfExists -Source (Join-Path $OldStateRoot 'web-category-rules.json') -Destination (Join-Path $NewStateRoot 'web-category-rules.json')
    Copy-IfExists -Source (Join-Path $OldStateRoot 'dlp-policy.json') -Destination (Join-Path $NewStateRoot 'dlp-policy.json')
    if (-not (Test-Path -LiteralPath (Join-Path $NewStateRoot 'web-category-rules.json'))) {
        Copy-IfExists -Source (Join-Path $NewStateRoot 'web-category-rules.example.json') -Destination (Join-Path $NewStateRoot 'web-category-rules.json')
    }
    if (-not (Test-Path -LiteralPath (Join-Path $NewStateRoot 'dlp-policy.json'))) {
        Copy-IfExists -Source (Join-Path $NewStateRoot 'dlp-policy.example.json') -Destination (Join-Path $NewStateRoot 'dlp-policy.json')
    }

    $config = Update-AWatchConfigPaths -Config $existing.Config
    Write-ActivityWatchDeploymentConfig -Config $config -Path $newConfigPath
    Write-ActivityWatchLaunchScript -Path $config.paths.launchScript -ConfigPath $newConfigPath
    Write-ActivityWatchRecoveryScript -Path $config.paths.recoveryScript -ConfigPath $newConfigPath

    $taskDefinitions = @($config.userTasks)
    Set-ActivityWatchAcl -InstallRoot $NewInstallRoot -StateRoot $NewStateRoot -LogsRoot $newLogsRoot
    Register-ActivityWatchUserTasks -TaskDefinitions $taskDefinitions -LaunchScriptPath $config.paths.launchScript -ConfigPath $newConfigPath
    Register-ActivityWatchRecoveryTask -TaskName $config.recovery.taskName -RecoveryScriptPath $config.paths.recoveryScript -ConfigPath $newConfigPath
    Start-ActivityWatchTasks -TaskDefinitions $taskDefinitions -RecoveryTaskName $config.recovery.taskName
    Start-Sleep -Seconds 5

    if (-not $SkipValidation) {
        $validateScript = Join-Path $ToolkitRoot 'validate-deployment.ps1'
        if (-not (Test-Path -LiteralPath $validateScript)) {
            $validateScript = Join-Path $PSScriptRoot 'validate-deployment.ps1'
        }
        $report = & $validateScript -ConfigPath $newConfigPath
        if (-not [bool]$report.overallOk) {
            throw "Миграция выполнена, но validation завершился ошибкой. Backup: $backupRoot"
        }
    }

    [pscustomobject]@{
        migrated = $true
        backupRoot = $backupRoot
        configPath = $newConfigPath
        installRoot = $NewInstallRoot
        stateRoot = $NewStateRoot
    }
}
