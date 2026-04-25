[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$ServerHost,
    [Parameter(Mandatory = $true)]
    [string]$TargetUser,
    [int]$ServerPort = 5600,
    [ValidateSet('http', 'https')]
    [string]$ServerScheme = 'http',
    [string]$Version = 'v0.13.2',
    [string]$PackageUrl,
    [string]$PackageZipPath,
    [string]$InstallRoot = 'C:\Program Files\ActivityWatch',
    [string]$StateRoot = 'C:\ProgramData\ActivityWatch',
    [int]$PollSeconds = 5,
    [int]$PulseSeconds = 30,
    [int]$RecoveryIntervalSeconds = 180,
    [string]$CustomRulesPath,
    [string]$CustomPolicyPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$modulePath = Join-Path $PSScriptRoot 'ActivityWatch.Windows.Common.psm1'
Import-Module $modulePath -Force

Assert-Administrator

$workingRoot = Join-Path $env:TEMP 'activitywatch-windows-deploy'
$backupRoot = Join-Path $StateRoot 'backups'
$logsRoot = Join-Path $StateRoot 'logs'
$configPath = Join-Path $StateRoot 'deployment-config.json'
$launchScriptPath = Join-Path $StateRoot 'launch-watchers.ps1'
$recoveryScriptPath = Join-Path $StateRoot 'recovery-loop.ps1'
$collectorSource = Join-Path $PSScriptRoot 'browser-domains-native-collector.ps1'
$endpointCollectorSource = Join-Path $PSScriptRoot 'dlp-endpoint-signals-collector.ps1'
$exampleRulesSource = Join-Path $PSScriptRoot 'web-category-rules.example.json'
$examplePolicySource = Join-Path $PSScriptRoot 'dlp-policy.example.json'

New-ActivityWatchDirectory -Path $StateRoot
New-ActivityWatchDirectory -Path $logsRoot

$archivePath = Get-ActivityWatchArchive -PackageZipPath $PackageZipPath -PackageUrl $PackageUrl -Version $Version -WorkingRoot $workingRoot
Install-ActivityWatchPackage -ArchivePath $archivePath -InstallRoot $InstallRoot -WorkingRoot $workingRoot -BackupRoot $backupRoot | Out-Null
Get-ActivityWatchExecutableMap -InstallRoot $InstallRoot | Out-Null

$assetResult = Copy-ActivityWatchCollectorAssets `
    -CollectorScriptSource $collectorSource `
    -EndpointCollectorScriptSource $endpointCollectorSource `
    -ExampleRulesSource $exampleRulesSource `
    -ExamplePolicySource $examplePolicySource `
    -StateRoot $StateRoot `
    -CustomRulesSource $CustomRulesPath `
    -CustomPolicySource $CustomPolicyPath
$taskDefinitions = New-ActivityWatchUserTaskDefinitions -Users @($TargetUser)

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
    -RulesPath $assetResult.ActiveRules `
    -PolicyPath $assetResult.ActivePolicy `
    -PollSeconds $PollSeconds `
    -PulseSeconds $PulseSeconds `
    -RecoveryIntervalSeconds $RecoveryIntervalSeconds `
    -LaunchScriptPath $launchScriptPath `
    -RecoveryScriptPath $recoveryScriptPath `
    -UserTasks $taskDefinitions `
    -PackageVersion $Version

Write-ActivityWatchDeploymentConfig -Config $config -Path $configPath
Remove-LegacyActivityWatchEntries
Set-ActivityWatchAcl -InstallRoot $InstallRoot -StateRoot $StateRoot -LogsRoot $logsRoot
Register-ActivityWatchUserTasks -TaskDefinitions $taskDefinitions -LaunchScriptPath $launchScriptPath -ConfigPath $configPath
Register-ActivityWatchRecoveryTask -TaskName $config.recovery.taskName -RecoveryScriptPath $recoveryScriptPath -ConfigPath $configPath
Start-ActivityWatchTasks -TaskDefinitions $taskDefinitions -RecoveryTaskName $config.recovery.taskName

Write-Host "ActivityWatch deployed for $TargetUser"
Write-Host "Server: ${ServerScheme}://$ServerHost`:$ServerPort"
Write-Host "Install root: $InstallRoot"
Write-Host "State root: $StateRoot"
Write-Host "Rules file: $($assetResult.ActiveRules)"
Write-Host "Policy file: $($assetResult.ActivePolicy)"
