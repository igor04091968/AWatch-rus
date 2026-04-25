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
    [string]$InstallRoot = 'C:\Program Files\ActivityWatch',
    [string]$StateRoot = 'C:\ProgramData\ActivityWatch',
    [int]$PollSeconds = 5,
    [int]$PulseSeconds = 30,
    [int]$RecoveryIntervalSeconds = 180,
    [string]$CustomRulesPath,
    [string]$CustomPolicyPath,
    [string]$ReportPath,
    [switch]$SkipHardening,
    [switch]$ValidateAfterDeploy
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
    throw "Missing script: $deployScript"
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
    -CustomRulesPath $CustomRulesPath `
    -CustomPolicyPath $CustomPolicyPath

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
        -CustomRulesPath $CustomRulesPath `
        -CustomPolicyPath $CustomPolicyPath
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
    hardeningApplied = (-not $SkipHardening)
}

if ($ValidateAfterDeploy) {
    if (-not (Test-Path -LiteralPath $validationScript)) {
        throw "Missing script: $validationScript"
    }

    $validation = & $validationScript -ConfigPath (Join-Path $StateRoot 'deployment-config.json')
    $report.validation = $validation
}

$reportDirectory = Split-Path -Path $effectiveReportPath -Parent
if ($reportDirectory) {
    New-ActivityWatchDirectory -Path $reportDirectory
}

$report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $effectiveReportPath -Encoding UTF8

Write-Host 'ActivityWatch ensemble deploy completed.'
Write-Host "Users: $($resolvedUsers -join ', ')"
Write-Host "Report: $effectiveReportPath"
