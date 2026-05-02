[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\ActivityWatch-Phase2\deployment-config.json',
    [string]$ServerHost,
    [int]$ServerPort,
    [ValidateSet('http', 'https')]
    [string]$ServerScheme,
    [string[]]$Users,
    [string]$UserListPath,
    [string]$Domain,
    [string]$InstallRoot,
    [string]$StateRoot,
    [int]$PollSeconds,
    [int]$PulseSeconds,
    [int]$RecoveryIntervalSeconds,
    [bool]$AfkEnabled,
    [bool]$WindowEnabled,
    [bool]$LocalAgentLogsEnabled,
    [bool]$IncidentCaptureEnabled,
    [bool]$IncidentScreenshotEnabled,
    [string]$IncidentArtifactsRoot,
    [bool]$LogonMarkerEnabled,
    [string]$CustomRulesPath,
    [string]$CustomPolicyPath,
    [switch]$RepairPackage,
    [string]$Version,
    [string]$PackageUrl,
    [string]$PackageZipPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$modulePath = Join-Path $PSScriptRoot 'ActivityWatch.Windows.Common.psm1'
Import-Module $modulePath -Force

Assert-Administrator

$existingConfig = $null
if (Test-Path -LiteralPath $ConfigPath) {
    $existingConfig = Read-ActivityWatchDeploymentConfig -Path $ConfigPath
}

if (-not $existingConfig -and (-not $ServerHost)) {
    throw 'deployment-config.json отсутствует. Укажите -ServerHost и параметры пользователей либо сначала выполните скрипт развёртывания.'
}

$effectiveStateRoot = if ($StateRoot) { $StateRoot } elseif ($existingConfig) { [string]$existingConfig.paths.stateRoot } else { 'C:\ProgramData\ActivityWatch-Phase2' }
$effectiveInstallRoot = if ($InstallRoot) { $InstallRoot } elseif ($existingConfig) { [string]$existingConfig.paths.installRoot } else { 'C:\Program Files\ActivityWatch-Phase2' }
$effectiveLogsRoot = if ($existingConfig) { [string]$existingConfig.paths.logsRoot } else { Join-Path $effectiveStateRoot 'logs' }
$effectiveConfigPath = if ($ConfigPath) { $ConfigPath } else { Join-Path $effectiveStateRoot 'deployment-config.json' }
$effectiveLaunchScript = Join-Path $effectiveStateRoot 'launch-watchers.ps1'
$effectiveRecoveryScript = Join-Path $effectiveStateRoot 'recovery-loop.ps1'
$effectiveCollector = Join-Path $effectiveStateRoot 'browser-domains-native-collector.ps1'
$effectiveEndpointCollector = if ($existingConfig -and $existingConfig.paths.PSObject.Properties.Name -contains 'endpointCollectorScript') { [string]$existingConfig.paths.endpointCollectorScript } else { Join-Path $effectiveStateRoot 'dlp-endpoint-signals-collector.ps1' }
$effectiveRules = Join-Path $effectiveStateRoot 'web-category-rules.json'
$effectivePolicy = if ($existingConfig -and $existingConfig.paths.PSObject.Properties.Name -contains 'policyPath') { [string]$existingConfig.paths.policyPath } else { Join-Path $effectiveStateRoot 'dlp-policy.json' }

$effectiveServerHost = if ($ServerHost) { $ServerHost } elseif ($existingConfig) { [string]$existingConfig.server.host } else { $null }
$effectiveServerPort = if ($PSBoundParameters.ContainsKey('ServerPort')) { $ServerPort } elseif ($existingConfig) { [int]$existingConfig.server.port } else { 5600 }
$effectiveServerScheme = if ($ServerScheme) { $ServerScheme } elseif ($existingConfig) { [string]$existingConfig.server.scheme } else { 'http' }
$effectivePollSeconds = if ($PSBoundParameters.ContainsKey('PollSeconds')) { $PollSeconds } elseif ($existingConfig) { [int]$existingConfig.collector.pollSeconds } else { 5 }
$effectivePulseSeconds = if ($PSBoundParameters.ContainsKey('PulseSeconds')) { $PulseSeconds } elseif ($existingConfig) { [int]$existingConfig.collector.pulseSeconds } else { 30 }
$effectiveRecoveryInterval = if ($PSBoundParameters.ContainsKey('RecoveryIntervalSeconds')) { $RecoveryIntervalSeconds } elseif ($existingConfig) { [int]$existingConfig.recovery.intervalSeconds } else { 180 }
$effectiveAfkEnabled = if ($PSBoundParameters.ContainsKey('AfkEnabled')) { [bool]$AfkEnabled } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'collectors' -and $existingConfig.collectors.PSObject.Properties.Name -contains 'afkEnabled') { [bool]$existingConfig.collectors.afkEnabled } else { $true }
$effectiveWindowEnabled = if ($PSBoundParameters.ContainsKey('WindowEnabled')) { [bool]$WindowEnabled } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'collectors' -and $existingConfig.collectors.PSObject.Properties.Name -contains 'windowEnabled') { [bool]$existingConfig.collectors.windowEnabled } else { $true }
$effectiveLocalAgentLogsEnabled = if ($PSBoundParameters.ContainsKey('LocalAgentLogsEnabled')) { [bool]$LocalAgentLogsEnabled } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'logging' -and $existingConfig.logging.PSObject.Properties.Name -contains 'localAgentLogsEnabled') { [bool]$existingConfig.logging.localAgentLogsEnabled } else { $false }
$effectiveIncidentCaptureEnabled = if ($PSBoundParameters.ContainsKey('IncidentCaptureEnabled')) { [bool]$IncidentCaptureEnabled } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'incidentCapture' -and $existingConfig.incidentCapture.PSObject.Properties.Name -contains 'enabled') { [bool]$existingConfig.incidentCapture.enabled } else { $true }
$effectiveIncidentScreenshotEnabled = if ($PSBoundParameters.ContainsKey('IncidentScreenshotEnabled')) { [bool]$IncidentScreenshotEnabled } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'incidentCapture' -and $existingConfig.incidentCapture.PSObject.Properties.Name -contains 'screenshotEnabled') { [bool]$existingConfig.incidentCapture.screenshotEnabled } else { $true }
$effectiveIncidentArtifactsRoot = if ($PSBoundParameters.ContainsKey('IncidentArtifactsRoot') -and $IncidentArtifactsRoot) { $IncidentArtifactsRoot } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'incidentCapture' -and $existingConfig.incidentCapture.PSObject.Properties.Name -contains 'artifactsRoot') { [string]$existingConfig.incidentCapture.artifactsRoot } else { Join-Path $effectiveStateRoot 'incident-artifacts' }
$effectiveLogonMarkerEnabled = if ($PSBoundParameters.ContainsKey('LogonMarkerEnabled')) { [bool]$LogonMarkerEnabled } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'sessionEvents' -and $existingConfig.sessionEvents.PSObject.Properties.Name -contains 'logonEnabled') { [bool]$existingConfig.sessionEvents.logonEnabled } else { $true }
$effectiveVersion = if ($Version) { $Version } elseif ($existingConfig) { [string]$existingConfig.package.version } else { 'v0.13.2' }

$effectiveUsers = if ($Users -or $UserListPath) {
    Normalize-ActivityWatchUsers -Users $Users -UserListPath $UserListPath -Domain $Domain
}
elseif ($existingConfig) {
    @($existingConfig.userTasks | ForEach-Object { [string]$_.userId })
}
else {
    throw 'Не указаны целевые пользователи.'
}

New-ActivityWatchDirectory -Path $effectiveStateRoot
New-ActivityWatchDirectory -Path $effectiveLogsRoot

if ($RepairPackage) {
    $workingRoot = Join-Path $env:TEMP 'activitywatch-windows-deploy'
    $backupRoot = Join-Path $effectiveStateRoot 'backups'
    $archivePath = Get-ActivityWatchArchive -PackageZipPath $PackageZipPath -PackageUrl $PackageUrl -Version $effectiveVersion -WorkingRoot $workingRoot
    Install-ActivityWatchPackage -ArchivePath $archivePath -InstallRoot $effectiveInstallRoot -WorkingRoot $workingRoot -BackupRoot $backupRoot | Out-Null
}

Get-ActivityWatchExecutableMap -InstallRoot $effectiveInstallRoot | Out-Null

$assetResult = Copy-ActivityWatchCollectorAssets `
    -CollectorScriptSource (Join-Path $PSScriptRoot 'browser-domains-native-collector.ps1') `
    -EndpointCollectorScriptSource (Join-Path $PSScriptRoot 'dlp-endpoint-signals-collector.ps1') `
    -SessionCollectorScriptSource (Join-Path $PSScriptRoot 'worktime-session-collector.ps1') `
    -ExampleRulesSource (Join-Path $PSScriptRoot 'web-category-rules.example.json') `
    -ExamplePolicySource (Join-Path $PSScriptRoot 'dlp-policy.example.json') `
    -StateRoot $effectiveStateRoot `
    -CustomRulesSource $CustomRulesPath `
    -CustomPolicySource $CustomPolicyPath

$taskDefinitions = New-ActivityWatchUserTaskDefinitions -Users $effectiveUsers
Write-ActivityWatchLaunchScript -Path $effectiveLaunchScript -ConfigPath $effectiveConfigPath
Write-ActivityWatchRecoveryScript -Path $effectiveRecoveryScript -ConfigPath $effectiveConfigPath

$config = New-ActivityWatchDeploymentConfig `
    -ServerHost $effectiveServerHost `
    -ServerPort $effectiveServerPort `
    -ServerScheme $effectiveServerScheme `
    -InstallRoot $effectiveInstallRoot `
    -StateRoot $effectiveStateRoot `
    -LogsRoot $effectiveLogsRoot `
    -CollectorScript $effectiveCollector `
    -EndpointCollectorScript $effectiveEndpointCollector `
    -SessionCollectorScript $effectiveSessionCollector `
    -RulesPath $effectiveRules `
    -PolicyPath $effectivePolicy `
    -PollSeconds $effectivePollSeconds `
    -PulseSeconds $effectivePulseSeconds `
    -RecoveryIntervalSeconds $effectiveRecoveryInterval `
    -AfkEnabled $effectiveAfkEnabled `
    -WindowEnabled $effectiveWindowEnabled `
    -LocalAgentLogsEnabled $effectiveLocalAgentLogsEnabled `
    -IncidentCaptureEnabled $effectiveIncidentCaptureEnabled `
    -IncidentScreenshotEnabled $effectiveIncidentScreenshotEnabled `
    -IncidentArtifactsRoot $effectiveIncidentArtifactsRoot `
    -LogonMarkerEnabled $effectiveLogonMarkerEnabled `
    -LaunchScriptPath $effectiveLaunchScript `
    -RecoveryScriptPath $effectiveRecoveryScript `
    -UserTasks $taskDefinitions `
    -PackageVersion $effectiveVersion

Write-ActivityWatchDeploymentConfig -Config $config -Path $effectiveConfigPath
Remove-LegacyActivityWatchEntries
Set-ActivityWatchAcl -InstallRoot $effectiveInstallRoot -StateRoot $effectiveStateRoot -LogsRoot $effectiveLogsRoot
Register-ActivityWatchUserTasks -TaskDefinitions $taskDefinitions -LaunchScriptPath $effectiveLaunchScript -ConfigPath $effectiveConfigPath
Register-ActivityWatchRecoveryTask -TaskName $config.recovery.taskName -RecoveryScriptPath $effectiveRecoveryScript -ConfigPath $effectiveConfigPath
Start-ActivityWatchTasks -TaskDefinitions $taskDefinitions -RecoveryTaskName $config.recovery.taskName

Write-Host 'Укрепление и восстановление ActivityWatch завершены.'
Write-Host "Конфигурация: $effectiveConfigPath"
Write-Host "Пользователи восстановлены: $($effectiveUsers -join ', ')"
