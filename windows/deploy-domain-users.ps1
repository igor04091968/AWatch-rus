[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$ServerHost,
    [string[]]$Users,
    [string]$UserListPath,
    [string]$Domain,
    [int]$ServerPort = 5600,
    [ValidateSet('http', 'https')]
    [string]$ServerScheme = 'http',
    [string]$Version = 'v0.13.2',
    [string]$PackageUrl,
    [string]$PackageZipPath,
    [string]$InstallRoot = 'C:\Program Files\AWatch-rus\bin',
    [string]$StateRoot = 'C:\ProgramData\AWatch-rus',
    [int]$PollSeconds = 5,
    [int]$PulseSeconds = 30,
    [int]$RecoveryIntervalSeconds = 180,
    [bool]$AfkEnabled = $true,
    [bool]$WindowEnabled = $true,
    [bool]$FileOpsEnabled = $true,
    [bool]$LocalAgentLogsEnabled = $false,
    [bool]$IncidentCaptureEnabled = $true,
    [bool]$IncidentScreenshotEnabled = $true,
    [string]$IncidentArtifactsRoot,
    [string]$EvtxExportRoot,
    [int]$EvtxRetentionDays = 14,
    [string[]]$EvtxChannels = @(),
    [bool]$LogonMarkerEnabled = $true,
    [string]$AwHostname,
    [string]$CustomRulesPath,
    [string]$CustomPolicyPath,
    [ValidateSet('local', 'server')]
    [string]$PolicyMode = 'local',
    [bool]$PolicyEngineEnabled = $false,
    [string]$PolicyEngineHost,
    [int]$PolicyEnginePort = 5601,
    [ValidateSet('http', 'https')]
    [string]$PolicyEngineScheme = 'http',
    [int]$PolicyRefreshSeconds = 300,
    [string]$PolicyCachePath,
    [switch]$IntegrationTestEnabled
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$modulePath = Join-Path $PSScriptRoot 'ActivityWatch.Windows.Common.psm1'
Import-Module $modulePath -Force

Assert-Administrator

$targetUsers = Normalize-ActivityWatchUsers -Users $Users -UserListPath $UserListPath -Domain $Domain
$workingRoot = Join-Path $env:TEMP 'activitywatch-windows-deploy'
$backupRoot = Join-Path $StateRoot 'backups'
$logsRoot = Join-Path $StateRoot 'logs'
$configPath = Join-Path $StateRoot 'deployment-config.json'
$launchScriptPath = Join-Path $StateRoot 'launch-watchers.ps1'
$recoveryScriptPath = Join-Path $StateRoot 'recovery-loop.ps1'
$collectorSource = Join-Path $PSScriptRoot 'browser-domains-native-collector.ps1'
$endpointCollectorSource = Join-Path $PSScriptRoot 'dlp-endpoint-signals-collector.ps1'
$policyClientSource = Join-Path $PSScriptRoot 'dlp-policy-client.ps1'
$emailCollectorSource = Join-Path $PSScriptRoot 'email-outbound-collector.ps1'
$fileCollectorSource = Join-Path $PSScriptRoot 'file-operations-collector.ps1'
$sessionCollectorSource = Join-Path $PSScriptRoot 'worktime-session-collector.ps1'
$evtxExportScriptSource = Join-Path $PSScriptRoot 'export-evtx-for-hayabusa.ps1'
$hayabusaUploadScriptSource = Join-Path $PSScriptRoot 'export-upload-hayabusa-to-aw-server.ps1'
$exampleRulesSource = Join-Path $PSScriptRoot 'web-category-rules.example.json'
$examplePolicySource = Join-Path $PSScriptRoot 'dlp-policy.example.json'

New-ActivityWatchDirectory -Path $StateRoot
New-ActivityWatchDirectory -Path $logsRoot
Enable-ActivityWatchPrintTelemetry

$archivePath = Get-ActivityWatchArchive -PackageZipPath $PackageZipPath -PackageUrl $PackageUrl -Version $Version -WorkingRoot $workingRoot
Install-ActivityWatchPackage -ArchivePath $archivePath -InstallRoot $InstallRoot -WorkingRoot $workingRoot -BackupRoot $backupRoot | Out-Null
Get-ActivityWatchExecutableMap -InstallRoot $InstallRoot | Out-Null

$assetResult = Copy-ActivityWatchCollectorAssets `
    -CollectorScriptSource $collectorSource `
    -EndpointCollectorScriptSource $endpointCollectorSource `
    -PolicyClientScriptSource $policyClientSource `
    -EmailCollectorScriptSource $emailCollectorSource `
    -FileCollectorScriptSource $fileCollectorSource `
    -SessionCollectorScriptSource $sessionCollectorSource `
    -EvtxExportScriptSource $evtxExportScriptSource `
    -HayabusaUploadScriptSource $hayabusaUploadScriptSource `
    -ExampleRulesSource $exampleRulesSource `
    -ExamplePolicySource $examplePolicySource `
    -StateRoot $StateRoot `
    -CustomRulesSource $CustomRulesPath `
    -CustomPolicySource $CustomPolicyPath
$taskDefinitions = New-ActivityWatchUserTaskDefinitions -Users $targetUsers

Write-ActivityWatchLaunchScript -Path $launchScriptPath -ConfigPath $configPath
Write-ActivityWatchRecoveryScript -Path $recoveryScriptPath -ConfigPath $configPath

$config = New-ActivityWatchDeploymentConfig `
    -ServerHost $ServerHost `
    -ServerPort $ServerPort `
    -ServerScheme $ServerScheme `
    -InstallRoot $InstallRoot `
    -StateRoot $StateRoot `
    -LogsRoot $logsRoot `
    -CollectorScript $assetResult.CollectorScript `
    -EndpointCollectorScript $assetResult.EndpointCollectorScript `
    -PolicyClientScript $assetResult.PolicyClientScript `
    -EmailCollectorScript $assetResult.EmailCollectorScript `
    -FileCollectorScript $assetResult.FileCollectorScript `
    -SessionCollectorScript $assetResult.SessionCollectorScript `
    -EvtxExportScript $assetResult.EvtxExportScript `
    -RulesPath $assetResult.ActiveRules `
    -PolicyPath $assetResult.ActivePolicy `
    -PollSeconds $PollSeconds `
    -PulseSeconds $PulseSeconds `
    -RecoveryIntervalSeconds $RecoveryIntervalSeconds `
    -AfkEnabled $AfkEnabled `
    -WindowEnabled $WindowEnabled `
    -FileOpsEnabled $FileOpsEnabled `
    -LocalAgentLogsEnabled $LocalAgentLogsEnabled `
    -IncidentCaptureEnabled $IncidentCaptureEnabled `
    -IncidentScreenshotEnabled $IncidentScreenshotEnabled `
    -IncidentArtifactsRoot $IncidentArtifactsRoot `
    -EvtxExportRoot $EvtxExportRoot `
    -EvtxRetentionDays $EvtxRetentionDays `
    -EvtxChannels $EvtxChannels `
    -LogonMarkerEnabled $LogonMarkerEnabled `
    -AwHostname $AwHostname `
    -PolicyMode $PolicyMode `
    -PolicyEngineEnabled $PolicyEngineEnabled `
    -PolicyEngineHost $PolicyEngineHost `
    -PolicyEnginePort $PolicyEnginePort `
    -PolicyEngineScheme $PolicyEngineScheme `
    -PolicyRefreshSeconds $PolicyRefreshSeconds `
    -PolicyCachePath $PolicyCachePath `
    -LaunchScriptPath $launchScriptPath `
    -RecoveryScriptPath $recoveryScriptPath `
    -UserTasks $taskDefinitions `
    -PackageVersion $Version `
    -IntegrationTestEnabled:$IntegrationTestEnabled

Write-ActivityWatchDeploymentConfig -Config $config -Path $configPath
Remove-LegacyActivityWatchEntries
Set-ActivityWatchAcl -InstallRoot $InstallRoot -StateRoot $StateRoot -LogsRoot $logsRoot
Register-ActivityWatchUserTasks -TaskDefinitions $taskDefinitions -LaunchScriptPath $launchScriptPath -ConfigPath $configPath
Register-ActivityWatchRecoveryTask -TaskName $config.recovery.taskName -RecoveryScriptPath $recoveryScriptPath -ConfigPath $configPath
Start-ActivityWatchTasks -TaskDefinitions $taskDefinitions -RecoveryTaskName $config.recovery.taskName

Write-Host 'ActivityWatch развёрнут для пользователей:'
$targetUsers | ForEach-Object { Write-Host " - $_" }
Write-Host "Сервер: ${ServerScheme}://$ServerHost`:$ServerPort"
Write-Host "Каталог данных: $StateRoot"
Write-Host "Файл DLP-политики: $($assetResult.ActivePolicy)"
