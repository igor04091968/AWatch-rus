[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$ServerHost,
    [int]$ServerPort = 5600,
    [ValidateSet('http', 'https')]
    [string]$ServerScheme = 'http',
    [string]$StateRoot = 'C:\ProgramData\AWatch-rus',
    [string]$InstallRoot = 'C:\Program Files\AWatch-rus\bin',
    [string]$ServiceName = 'AWatchRusStandaloneAgent',
    [string]$AwHostname
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-Admin {
    $id = [Security.Principal.WindowsIdentity]::GetCurrent()
    $p = [Security.Principal.WindowsPrincipal]::new($id)
    if (-not $p.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Run as Administrator.'
    }
}

function Ensure-Dir {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -Path $Path -ItemType Directory -Force | Out-Null
    }
}

Assert-Admin

$logsRoot = Join-Path $StateRoot 'logs'
Ensure-Dir -Path $StateRoot
Ensure-Dir -Path $logsRoot

$collectorScript = Join-Path $StateRoot 'browser-domains-native-collector.ps1'
$endpointCollectorScript = Join-Path $StateRoot 'dlp-endpoint-signals-collector.ps1'
$fileCollectorScript = Join-Path $StateRoot 'file-operations-collector.ps1'
$emailCollectorScript = Join-Path $StateRoot 'email-outbound-collector.ps1'
$sessionCollectorScript = Join-Path $StateRoot 'worktime-session-collector.ps1'
$rulesPath = Join-Path $StateRoot 'web-category-rules.json'
$policyPath = Join-Path $StateRoot 'dlp-policy.json'
$configPath = Join-Path $StateRoot 'deployment-config.json'
$serviceScriptPath = Join-Path $PSScriptRoot 'aw-standalone-service.ps1'

Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'browser-domains-native-collector.ps1') -Destination $collectorScript -Force
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'dlp-endpoint-signals-collector.ps1') -Destination $endpointCollectorScript -Force
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'file-operations-collector.ps1') -Destination $fileCollectorScript -Force
if (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'email-outbound-collector.ps1')) {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'email-outbound-collector.ps1') -Destination $emailCollectorScript -Force
}
if (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'worktime-session-collector.ps1')) {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'worktime-session-collector.ps1') -Destination $sessionCollectorScript -Force
}

if (-not (Test-Path -LiteralPath $rulesPath)) {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'web-category-rules.example.json') -Destination $rulesPath -Force
}
if (-not (Test-Path -LiteralPath $policyPath)) {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'dlp-policy.example.json') -Destination $policyPath -Force
}

$effectiveHostname = if ([string]::IsNullOrWhiteSpace($AwHostname)) { [string]$env:COMPUTERNAME } else { [string]$AwHostname }

$config = [pscustomobject]@{
    version = 1
    generatedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
    awHostname = $effectiveHostname
    server = [pscustomobject]@{
        host = $ServerHost
        port = $ServerPort
        scheme = $ServerScheme
    }
    paths = [pscustomobject]@{
        installRoot = $InstallRoot
        stateRoot = $StateRoot
        logsRoot = $logsRoot
        collectorScript = $collectorScript
        endpointCollectorScript = $endpointCollectorScript
        fileCollectorScript = $fileCollectorScript
        emailCollectorScript = $emailCollectorScript
        sessionCollectorScript = $sessionCollectorScript
        rulesPath = $rulesPath
        policyPath = $policyPath
    }
    collector = [pscustomobject]@{
        pollSeconds = 5
        pulseSeconds = 30
    }
    collectors = [pscustomobject]@{
        afkEnabled = $false
        windowEnabled = $false
        fileOpsEnabled = $true
        emailEnabled = $true
    }
    logging = [pscustomobject]@{
        localAgentLogsEnabled = $true
    }
}

$config | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $configPath -Encoding UTF8

$existing = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($existing) {
    sc.exe stop $ServiceName | Out-Null
    Start-Sleep -Seconds 1
    sc.exe delete $ServiceName | Out-Null
    Start-Sleep -Seconds 1
}

$binPath = "`"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe`" -NoProfile -ExecutionPolicy Bypass -File `"$serviceScriptPath`" -ConfigPath `"$configPath`""
sc.exe create $ServiceName binPath= "$binPath" start= auto DisplayName= "AWatch-rus Standalone Agent" | Out-Null
sc.exe description $ServiceName "Standalone AWatch-rus DLP agent service wrapper" | Out-Null
sc.exe failure $ServiceName reset= 60 actions= restart/5000/restart/5000/restart/5000 | Out-Null
sc.exe start $ServiceName | Out-Null

Write-Output "Standalone service installed: $ServiceName"
Write-Output "Config: $configPath"
Write-Output ("Host: {0} -> {1}://{2}:{3}" -f $effectiveHostname, $ServerScheme, $ServerHost, $ServerPort)
