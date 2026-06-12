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
    [bool]$ProcessEventsEnabled = $false,
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
    [string]$ReportPath,
    [bool]$HayabusaAutoUploadEnabled = $true,
    [int]$HayabusaAutoUploadIntervalHours = 6,
    [int]$HayabusaAutoUploadHoursBack = 6,
    [string]$HayabusaAutoUploadMode = 'incident',
    [string]$HayabusaAutoUploadTaskName = 'ActivityWatch Hayabusa Upload',
    [string]$HayabusaAutoUploadRunAsUser,
    [bool]$File1CAutoUploadEnabled = $true,
    [int]$File1CAutoUploadIntervalHours = 6,
    [int]$File1CAutoUploadIntervalMinutes = 15,
    [string]$File1CAutoUploadTaskName = 'ActivityWatch File1C Upload',
    [string]$File1CAutoUploadRunAsUser,
    [string]$File1CTargetHost,
    [string]$File1CTargetUser = 'igor',
    [string]$File1CRegistryWorkbookPath = 'E:\USER1\СПИСОК ПРЕДПРИЯТИЙ И ИХ РАСПРЕДЕЛЕНИЕ.xlsx',
    [switch]$SkipHardening,
    [switch]$ValidateAfterDeploy,
    [switch]$IntegrationTestEnabled
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$modulePath = Join-Path $PSScriptRoot 'ActivityWatch.Windows.Common.psm1'
Import-Module $modulePath -Force

Assert-Administrator

$resolvedUsers = Normalize-ActivityWatchUsers -Users $Users -UserListPath $UserListPath -Domain $Domain
$timestamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$effectiveReportPath = if ($ReportPath) { $ReportPath } else { Join-Path $StateRoot "ensemble-report-$timestamp.json" }
$deployScript = Join-Path $PSScriptRoot 'deploy-domain-users.ps1'
$hardeningScript = Join-Path $PSScriptRoot 'hardening-recovery.ps1'
$validationScript = Join-Path $PSScriptRoot 'validate-deployment.ps1'

if (-not (Test-Path -LiteralPath $deployScript)) {
    throw "Не найден скрипт: $deployScript"
}

& $deployScript `
    -ServerHost $ServerHost `
    -Users $resolvedUsers `
    -ServerPort $ServerPort `
    -ServerScheme $ServerScheme `
    -Version $Version `
    -PackageUrl $PackageUrl `
    -PackageZipPath $PackageZipPath `
    -InstallRoot $InstallRoot `
    -StateRoot $StateRoot `
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
    -ProcessEventsEnabled $ProcessEventsEnabled `
    -AwHostname $AwHostname `
    -CustomRulesPath $CustomRulesPath `
    -CustomPolicyPath $CustomPolicyPath `
    -PolicyMode $PolicyMode `
    -PolicyEngineEnabled $PolicyEngineEnabled `
    -PolicyEngineHost $PolicyEngineHost `
    -PolicyEnginePort $PolicyEnginePort `
    -PolicyEngineScheme $PolicyEngineScheme `
    -PolicyRefreshSeconds $PolicyRefreshSeconds `
    -PolicyCachePath $PolicyCachePath `
    -HayabusaAutoUploadEnabled $HayabusaAutoUploadEnabled `
    -HayabusaAutoUploadIntervalHours $HayabusaAutoUploadIntervalHours `
    -HayabusaAutoUploadHoursBack $HayabusaAutoUploadHoursBack `
    -HayabusaAutoUploadMode $HayabusaAutoUploadMode `
    -HayabusaAutoUploadTaskName $HayabusaAutoUploadTaskName `
    -HayabusaAutoUploadRunAsUser $HayabusaAutoUploadRunAsUser `
    -File1CAutoUploadEnabled $File1CAutoUploadEnabled `
    -File1CAutoUploadIntervalHours $File1CAutoUploadIntervalHours `
    -File1CAutoUploadIntervalMinutes $File1CAutoUploadIntervalMinutes `
    -File1CAutoUploadTaskName $File1CAutoUploadTaskName `
    -File1CAutoUploadRunAsUser $File1CAutoUploadRunAsUser `
    -File1CTargetHost $File1CTargetHost `
    -File1CTargetUser $File1CTargetUser `
    -File1CRegistryWorkbookPath $File1CRegistryWorkbookPath `
    -IntegrationTestEnabled:$IntegrationTestEnabled

if (-not $SkipHardening) {
    & $hardeningScript `
        -ConfigPath (Join-Path $StateRoot 'deployment-config.json') `
        -ServerHost $ServerHost `
        -ServerPort $ServerPort `
        -ServerScheme $ServerScheme `
        -Users $resolvedUsers `
        -InstallRoot $InstallRoot `
        -StateRoot $StateRoot `
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
        -ProcessEventsEnabled $ProcessEventsEnabled `
        -AwHostname $AwHostname `
        -CustomRulesPath $CustomRulesPath `
        -CustomPolicyPath $CustomPolicyPath `
        -PolicyMode $PolicyMode `
        -PolicyEngineEnabled $PolicyEngineEnabled `
        -PolicyEngineHost $PolicyEngineHost `
        -PolicyEnginePort $PolicyEnginePort `
        -PolicyEngineScheme $PolicyEngineScheme `
        -PolicyRefreshSeconds $PolicyRefreshSeconds `
        -PolicyCachePath $PolicyCachePath
}

$report = [ordered]@{
    generatedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
    server = [ordered]@{
        host = $ServerHost
        port = $ServerPort
        scheme = $ServerScheme
    }
    packageVersion = $Version
    users = @($resolvedUsers)
    paths = [ordered]@{
        installRoot = $InstallRoot
        stateRoot = $StateRoot
        configPath = Join-Path $StateRoot 'deployment-config.json'
    }
    collectors = [ordered]@{
        afkEnabled = $AfkEnabled
        windowEnabled = $WindowEnabled
        fileOpsEnabled = $FileOpsEnabled
        worktimeSessionEnabled = $true
        worktimeSessionMode = 'powershell_primary'
        worktimeLegacyFallbackEnabled = $true
    }
    hardeningApplied = (-not $SkipHardening)
}

if ($ValidateAfterDeploy) {
    if (-not (Test-Path -LiteralPath $validationScript)) {
        throw "Не найден скрипт: $validationScript"
    }

    $validation = & $validationScript -ConfigPath (Join-Path $StateRoot 'deployment-config.json')
    $report.validation = $validation
}

$reportDirectory = Split-Path -Path $effectiveReportPath -Parent
if ($reportDirectory) {
    New-ActivityWatchDirectory -Path $reportDirectory
}

$report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $effectiveReportPath -Encoding UTF8

Write-Host 'Комплексное развёртывание ActivityWatch завершено.'
Write-Host "Пользователи: $($resolvedUsers -join ', ')"
Write-Host "Отчёт: $effectiveReportPath"
