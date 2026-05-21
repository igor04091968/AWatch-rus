[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\AWatch-rus\deployment-config.json',
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
    [bool]$FileOpsEnabled,
    [bool]$LocalAgentLogsEnabled,
    [bool]$IncidentCaptureEnabled,
    [bool]$IncidentScreenshotEnabled,
    [string]$IncidentArtifactsRoot,
    [string]$EvtxExportRoot,
    [int]$EvtxRetentionDays,
    [string[]]$EvtxChannels,
    [bool]$LogonMarkerEnabled,
    [string]$AwHostname,
    [string]$CustomRulesPath,
    [string]$CustomPolicyPath,
    [ValidateSet('local', 'server')]
    [string]$PolicyMode,
    [bool]$PolicyEngineEnabled,
    [string]$PolicyEngineHost,
    [int]$PolicyEnginePort,
    [ValidateSet('http', 'https')]
    [string]$PolicyEngineScheme,
    [int]$PolicyRefreshSeconds,
    [string]$PolicyCachePath,
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

$effectiveStateRoot = if ($StateRoot) { $StateRoot } elseif ($existingConfig) { [string]$existingConfig.paths.stateRoot } else { 'C:\ProgramData\AWatch-rus' }
$effectiveInstallRoot = if ($InstallRoot) { $InstallRoot } elseif ($existingConfig) { [string]$existingConfig.paths.installRoot } else { 'C:\Program Files\AWatch-rus\bin' }
$effectiveLogsRoot = if ($existingConfig) { [string]$existingConfig.paths.logsRoot } else { Join-Path $effectiveStateRoot 'logs' }
$effectiveConfigPath = if ($ConfigPath) { $ConfigPath } else { Join-Path $effectiveStateRoot 'deployment-config.json' }
$effectiveLaunchScript = Join-Path $effectiveStateRoot 'launch-watchers.ps1'
$effectiveRecoveryScript = Join-Path $effectiveStateRoot 'recovery-loop.ps1'
$effectiveCollector = Join-Path $effectiveStateRoot 'browser-domains-native-collector.ps1'
$effectiveEndpointCollector = if ($existingConfig -and $existingConfig.paths.PSObject.Properties.Name -contains 'endpointCollectorScript') { [string]$existingConfig.paths.endpointCollectorScript } else { Join-Path $effectiveStateRoot 'dlp-endpoint-signals-collector.ps1' }
$effectiveFileCollector = if ($existingConfig -and $existingConfig.paths.PSObject.Properties.Name -contains 'fileCollectorScript') { [string]$existingConfig.paths.fileCollectorScript } else { Join-Path $effectiveStateRoot 'file-operations-collector.ps1' }
$effectiveSessionCollector = if ($existingConfig -and $existingConfig.paths.PSObject.Properties.Name -contains 'sessionCollectorScript') { [string]$existingConfig.paths.sessionCollectorScript } else { Join-Path $effectiveStateRoot 'worktime-session-collector.ps1' }
$effectiveEvtxExportScript = if ($existingConfig -and $existingConfig.paths.PSObject.Properties.Name -contains 'evtxExportScript') { [string]$existingConfig.paths.evtxExportScript } else { Join-Path $effectiveStateRoot 'export-evtx-for-hayabusa.ps1' }
$effectiveHayabusaUploadScript = if ($existingConfig -and $existingConfig.paths.PSObject.Properties.Name -contains 'hayabusaUploadScript') { [string]$existingConfig.paths.hayabusaUploadScript } else { Join-Path $effectiveStateRoot 'export-upload-hayabusa-to-aw-server.ps1' }
$effectiveRules = Join-Path $effectiveStateRoot 'web-category-rules.json'
$effectivePolicy = if ($existingConfig -and $existingConfig.paths.PSObject.Properties.Name -contains 'policyPath') { [string]$existingConfig.paths.policyPath } else { Join-Path $effectiveStateRoot 'dlp-policy.json' }
$effectivePolicyClientScript = if ($existingConfig -and $existingConfig.paths.PSObject.Properties.Name -contains 'policyClientScript') { [string]$existingConfig.paths.policyClientScript } else { Join-Path $effectiveStateRoot 'dlp-policy-client.ps1' }

$effectiveServerHost = if ($ServerHost) { $ServerHost } elseif ($existingConfig) { [string]$existingConfig.server.host } else { $null }
$effectiveServerPort = if ($PSBoundParameters.ContainsKey('ServerPort')) { $ServerPort } elseif ($existingConfig) { [int]$existingConfig.server.port } else { 5600 }
$effectiveServerScheme = if ($ServerScheme) { $ServerScheme } elseif ($existingConfig) { [string]$existingConfig.server.scheme } else { 'http' }
$effectivePollSeconds = if ($PSBoundParameters.ContainsKey('PollSeconds')) { $PollSeconds } elseif ($existingConfig) { [int]$existingConfig.collector.pollSeconds } else { 5 }
$effectivePulseSeconds = if ($PSBoundParameters.ContainsKey('PulseSeconds')) { $PulseSeconds } elseif ($existingConfig) { [int]$existingConfig.collector.pulseSeconds } else { 30 }
$effectiveRecoveryInterval = if ($PSBoundParameters.ContainsKey('RecoveryIntervalSeconds')) { $RecoveryIntervalSeconds } elseif ($existingConfig) { [int]$existingConfig.recovery.intervalSeconds } else { 180 }
$effectiveAfkEnabled = if ($PSBoundParameters.ContainsKey('AfkEnabled')) { [bool]$AfkEnabled } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'collectors' -and $existingConfig.collectors.PSObject.Properties.Name -contains 'afkEnabled') { [bool]$existingConfig.collectors.afkEnabled } else { $true }
$effectiveWindowEnabled = if ($PSBoundParameters.ContainsKey('WindowEnabled')) { [bool]$WindowEnabled } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'collectors' -and $existingConfig.collectors.PSObject.Properties.Name -contains 'windowEnabled') { [bool]$existingConfig.collectors.windowEnabled } else { $true }
$effectiveFileOpsEnabled = if ($PSBoundParameters.ContainsKey('FileOpsEnabled')) { [bool]$FileOpsEnabled } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'collectors' -and $existingConfig.collectors.PSObject.Properties.Name -contains 'fileOpsEnabled') { [bool]$existingConfig.collectors.fileOpsEnabled } else { $true }
$effectiveLocalAgentLogsEnabled = if ($PSBoundParameters.ContainsKey('LocalAgentLogsEnabled')) { [bool]$LocalAgentLogsEnabled } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'logging' -and $existingConfig.logging.PSObject.Properties.Name -contains 'localAgentLogsEnabled') { [bool]$existingConfig.logging.localAgentLogsEnabled } else { $false }
$effectiveIncidentCaptureEnabled = if ($PSBoundParameters.ContainsKey('IncidentCaptureEnabled')) { [bool]$IncidentCaptureEnabled } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'incidentCapture' -and $existingConfig.incidentCapture.PSObject.Properties.Name -contains 'enabled') { [bool]$existingConfig.incidentCapture.enabled } else { $true }
$effectiveIncidentScreenshotEnabled = if ($PSBoundParameters.ContainsKey('IncidentScreenshotEnabled')) { [bool]$IncidentScreenshotEnabled } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'incidentCapture' -and $existingConfig.incidentCapture.PSObject.Properties.Name -contains 'screenshotEnabled') { [bool]$existingConfig.incidentCapture.screenshotEnabled } else { $true }
$effectiveIncidentArtifactsRoot = if ($PSBoundParameters.ContainsKey('IncidentArtifactsRoot') -and $IncidentArtifactsRoot) { $IncidentArtifactsRoot } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'incidentCapture' -and $existingConfig.incidentCapture.PSObject.Properties.Name -contains 'artifactsRoot') { [string]$existingConfig.incidentCapture.artifactsRoot } else { Join-Path $effectiveStateRoot 'incident-artifacts' }
$effectiveEvtxExportRoot = if ($PSBoundParameters.ContainsKey('EvtxExportRoot') -and $EvtxExportRoot) { $EvtxExportRoot } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'forensics' -and $existingConfig.forensics.PSObject.Properties.Name -contains 'evtxExportRoot') { [string]$existingConfig.forensics.evtxExportRoot } else { Join-Path $effectiveStateRoot 'forensics\evtx-exports' }
$effectiveEvtxRetentionDays = if ($PSBoundParameters.ContainsKey('EvtxRetentionDays')) { [int]$EvtxRetentionDays } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'forensics' -and $existingConfig.forensics.PSObject.Properties.Name -contains 'retentionDays') { [int]$existingConfig.forensics.retentionDays } else { 14 }
$effectiveEvtxChannels = if ($PSBoundParameters.ContainsKey('EvtxChannels')) { @($EvtxChannels) } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'forensics' -and $existingConfig.forensics.PSObject.Properties.Name -contains 'evtxChannels') { @($existingConfig.forensics.evtxChannels) } else { @() }
$effectiveLogonMarkerEnabled = if ($PSBoundParameters.ContainsKey('LogonMarkerEnabled')) { [bool]$LogonMarkerEnabled } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'sessionEvents' -and $existingConfig.sessionEvents.PSObject.Properties.Name -contains 'logonEnabled') { [bool]$existingConfig.sessionEvents.logonEnabled } else { $true }
$effectiveAwHostname = if ($PSBoundParameters.ContainsKey('AwHostname') -and -not [string]::IsNullOrWhiteSpace($AwHostname)) { [string]$AwHostname } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'awHostname' -and -not [string]::IsNullOrWhiteSpace([string]$existingConfig.awHostname)) { [string]$existingConfig.awHostname } else { [string]$env:COMPUTERNAME }
$effectiveVersion = if ($Version) { $Version } elseif ($existingConfig) { [string]$existingConfig.package.version } else { 'v0.13.2' }
$effectivePolicyMode = if ($PSBoundParameters.ContainsKey('PolicyMode') -and $PolicyMode) { [string]$PolicyMode } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'policyEngine' -and $existingConfig.policyEngine.PSObject.Properties.Name -contains 'mode') { [string]$existingConfig.policyEngine.mode } else { 'local' }
$effectivePolicyEngineEnabled = if ($PSBoundParameters.ContainsKey('PolicyEngineEnabled')) { [bool]$PolicyEngineEnabled } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'policyEngine' -and $existingConfig.policyEngine.PSObject.Properties.Name -contains 'enabled') { [bool]$existingConfig.policyEngine.enabled } else { $false }
$effectivePolicyEngineHost = if ($PSBoundParameters.ContainsKey('PolicyEngineHost') -and -not [string]::IsNullOrWhiteSpace($PolicyEngineHost)) { [string]$PolicyEngineHost } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'policyEngine' -and $existingConfig.policyEngine.PSObject.Properties.Name -contains 'host') { [string]$existingConfig.policyEngine.host } else { [string]$effectiveServerHost }
$effectivePolicyEnginePort = if ($PSBoundParameters.ContainsKey('PolicyEnginePort')) { [int]$PolicyEnginePort } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'policyEngine' -and $existingConfig.policyEngine.PSObject.Properties.Name -contains 'port') { [int]$existingConfig.policyEngine.port } else { 5601 }
$effectivePolicyEngineScheme = if ($PSBoundParameters.ContainsKey('PolicyEngineScheme') -and $PolicyEngineScheme) { [string]$PolicyEngineScheme } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'policyEngine' -and $existingConfig.policyEngine.PSObject.Properties.Name -contains 'scheme') { [string]$existingConfig.policyEngine.scheme } else { 'http' }
$effectivePolicyRefreshSeconds = if ($PSBoundParameters.ContainsKey('PolicyRefreshSeconds')) { [int]$PolicyRefreshSeconds } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'policyEngine' -and $existingConfig.policyEngine.PSObject.Properties.Name -contains 'refreshSeconds') { [int]$existingConfig.policyEngine.refreshSeconds } else { 300 }
$effectivePolicyCachePath = if ($PSBoundParameters.ContainsKey('PolicyCachePath') -and $PolicyCachePath) { [string]$PolicyCachePath } elseif ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'policyEngine' -and $existingConfig.policyEngine.PSObject.Properties.Name -contains 'cachePath') { [string]$existingConfig.policyEngine.cachePath } else { Join-Path $effectiveStateRoot 'dlp-policy-cache.json' }
$effectiveHayabusaAutoUploadEnabled = if ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'forensics' -and $existingConfig.forensics.PSObject.Properties.Name -contains 'hayabusaAutomation' -and $existingConfig.forensics.hayabusaAutomation.PSObject.Properties.Name -contains 'enabled') { [bool]$existingConfig.forensics.hayabusaAutomation.enabled } else { $true }
$effectiveHayabusaAutoUploadIntervalHours = if ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'forensics' -and $existingConfig.forensics.PSObject.Properties.Name -contains 'hayabusaAutomation' -and $existingConfig.forensics.hayabusaAutomation.PSObject.Properties.Name -contains 'intervalHours') { [int]$existingConfig.forensics.hayabusaAutomation.intervalHours } else { 6 }
$effectiveHayabusaAutoUploadHoursBack = if ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'forensics' -and $existingConfig.forensics.PSObject.Properties.Name -contains 'hayabusaAutomation' -and $existingConfig.forensics.hayabusaAutomation.PSObject.Properties.Name -contains 'hoursBack') { [int]$existingConfig.forensics.hayabusaAutomation.hoursBack } else { 6 }
$effectiveHayabusaAutoUploadMode = if ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'forensics' -and $existingConfig.forensics.PSObject.Properties.Name -contains 'hayabusaAutomation' -and $existingConfig.forensics.hayabusaAutomation.PSObject.Properties.Name -contains 'mode') { [string]$existingConfig.forensics.hayabusaAutomation.mode } else { 'incident' }
$effectiveHayabusaAutoUploadTaskName = if ($existingConfig -and $existingConfig.PSObject.Properties.Name -contains 'forensics' -and $existingConfig.forensics.PSObject.Properties.Name -contains 'hayabusaAutomation' -and $existingConfig.forensics.hayabusaAutomation.PSObject.Properties.Name -contains 'taskName') { [string]$existingConfig.forensics.hayabusaAutomation.taskName } else { 'ActivityWatch Hayabusa Upload' }

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
Enable-ActivityWatchPrintTelemetry

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
    -EmailCollectorScriptSource (Join-Path $PSScriptRoot 'email-outbound-collector.ps1') `
    -FileCollectorScriptSource (Join-Path $PSScriptRoot 'file-operations-collector.ps1') `
    -SessionCollectorScriptSource (Join-Path $PSScriptRoot 'worktime-session-collector.ps1') `
    -EvtxExportScriptSource (Join-Path $PSScriptRoot 'export-evtx-for-hayabusa.ps1') `
    -HayabusaUploadScriptSource (Join-Path $PSScriptRoot 'export-upload-hayabusa-to-aw-server.ps1') `
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
    -PolicyClientScript $effectivePolicyClientScript `
    -EmailCollectorScript $assetResult.EmailCollectorScript `
    -FileCollectorScript $effectiveFileCollector `
    -SessionCollectorScript $effectiveSessionCollector `
    -EvtxExportScript $effectiveEvtxExportScript `
    -HayabusaUploadScript $effectiveHayabusaUploadScript `
    -RulesPath $effectiveRules `
    -PolicyPath $effectivePolicy `
    -PollSeconds $effectivePollSeconds `
    -PulseSeconds $effectivePulseSeconds `
    -RecoveryIntervalSeconds $effectiveRecoveryInterval `
    -AfkEnabled $effectiveAfkEnabled `
    -WindowEnabled $effectiveWindowEnabled `
    -FileOpsEnabled $effectiveFileOpsEnabled `
    -LocalAgentLogsEnabled $effectiveLocalAgentLogsEnabled `
    -IncidentCaptureEnabled $effectiveIncidentCaptureEnabled `
    -IncidentScreenshotEnabled $effectiveIncidentScreenshotEnabled `
    -IncidentArtifactsRoot $effectiveIncidentArtifactsRoot `
    -EvtxExportRoot $effectiveEvtxExportRoot `
    -EvtxRetentionDays $effectiveEvtxRetentionDays `
    -EvtxChannels $effectiveEvtxChannels `
    -LogonMarkerEnabled $effectiveLogonMarkerEnabled `
    -AwHostname $effectiveAwHostname `
    -PolicyMode $effectivePolicyMode `
    -PolicyEngineEnabled $effectivePolicyEngineEnabled `
    -PolicyEngineHost $effectivePolicyEngineHost `
    -PolicyEnginePort $effectivePolicyEnginePort `
    -PolicyEngineScheme $effectivePolicyEngineScheme `
    -PolicyRefreshSeconds $effectivePolicyRefreshSeconds `
    -PolicyCachePath $effectivePolicyCachePath `
    -HayabusaAutoUploadEnabled $effectiveHayabusaAutoUploadEnabled `
    -HayabusaAutoUploadIntervalHours $effectiveHayabusaAutoUploadIntervalHours `
    -HayabusaAutoUploadHoursBack $effectiveHayabusaAutoUploadHoursBack `
    -HayabusaAutoUploadMode $effectiveHayabusaAutoUploadMode `
    -HayabusaAutoUploadTaskName $effectiveHayabusaAutoUploadTaskName `
    -LaunchScriptPath $effectiveLaunchScript `
    -RecoveryScriptPath $effectiveRecoveryScript `
    -UserTasks $taskDefinitions `
    -PackageVersion $effectiveVersion

Write-ActivityWatchDeploymentConfig -Config $config -Path $effectiveConfigPath
Remove-LegacyActivityWatchEntries
Set-ActivityWatchAcl -InstallRoot $effectiveInstallRoot -StateRoot $effectiveStateRoot -LogsRoot $effectiveLogsRoot
Register-ActivityWatchUserTasks -TaskDefinitions $taskDefinitions -LaunchScriptPath $effectiveLaunchScript -ConfigPath $effectiveConfigPath
Register-ActivityWatchRecoveryTask -TaskName $config.recovery.taskName -RecoveryScriptPath $effectiveRecoveryScript -ConfigPath $effectiveConfigPath
Register-ActivityWatchHayabusaAutoUploadTask -ConfigPath $effectiveConfigPath
Start-ActivityWatchTasks -TaskDefinitions $taskDefinitions -RecoveryTaskName $config.recovery.taskName

Write-Host 'Укрепление и восстановление ActivityWatch завершены.'
Write-Host "Конфигурация: $effectiveConfigPath"
Write-Host "Пользователи восстановлены: $($effectiveUsers -join ', ')"
