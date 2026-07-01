[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\AWatch-rus\deployment-config.json',
    [ValidateSet('shadow', 'enforce')]
    [string]$Mode = 'shadow',
    [string]$ServiceName = 'AWatchRusCollectorGuard',
    [int]$LoopSeconds = 60,
    [switch]$DisableRecoveryTask
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-Admin {
    $id = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($id)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Run as Administrator.'
    }
}

Assert-Admin

$guardScriptPath = Join-Path $PSScriptRoot 'aw-collector-guard.ps1'
$rustTelemetryPath = Join-Path $PSScriptRoot 'aw-windows-telemetry.exe'
$serviceSourcePath = Join-Path $PSScriptRoot 'AWatchRusCollectorGuardService.cs'
$serviceExePath = Join-Path $PSScriptRoot 'AWatchRusCollectorGuardService.exe'
if (-not (Test-Path -LiteralPath $rustTelemetryPath) -and -not (Test-Path -LiteralPath $guardScriptPath)) {
    throw "Neither Rust collector guard nor PowerShell fallback was found: $rustTelemetryPath ; $guardScriptPath"
}
if (-not (Test-Path -LiteralPath $serviceSourcePath)) {
    throw "Collector guard service source not found: $serviceSourcePath"
}
if (-not (Test-Path -LiteralPath $ConfigPath)) {
    throw "Config not found: $ConfigPath"
}

$cscCandidates = @(
    (Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'),
    (Join-Path $env:WINDIR 'Microsoft.NET\Framework\v4.0.30319\csc.exe')
)
$csc = @($cscCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1)
if (-not $csc) {
    throw 'C# compiler not found. Install .NET Framework build tools or provide AWatchRusCollectorGuardService.exe.'
}

$existing = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($existing) {
    if ($existing.Status -ne 'Stopped') {
        Stop-Service -Name $ServiceName -Force -ErrorAction SilentlyContinue
        try {
            $existing.WaitForStatus('Stopped', [TimeSpan]::FromSeconds(20))
        }
        catch {
        }
    }
    sc.exe delete $ServiceName | Out-Null
    Start-Sleep -Seconds 2
}

& $csc /nologo /target:exe /optimize+ /out:$serviceExePath /reference:System.ServiceProcess.dll $serviceSourcePath | Out-Null
if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $serviceExePath)) {
    throw "Failed to compile $serviceExePath"
}

$logsRoot = Join-Path (Split-Path -Path $ConfigPath -Parent) 'logs'
$serviceLogPath = Join-Path $logsRoot 'collector-guard-service.log'
if (Test-Path -LiteralPath $rustTelemetryPath) {
    $rustArgs = "collector-guard --config-path `"$ConfigPath`" --mode $Mode --loop-seconds $LoopSeconds"
    $binPath = "`"$serviceExePath`" --service-name `"$ServiceName`" --exec `"$rustTelemetryPath`" --args `"$rustArgs`" --log `"$serviceLogPath`""
}
else {
    if (-not (Test-Path -LiteralPath $guardScriptPath)) {
        throw "Collector guard script not found: $guardScriptPath"
    }
    $binPath = "`"$serviceExePath`" --service-name `"$ServiceName`" --script `"$guardScriptPath`" --config `"$ConfigPath`" --mode $Mode --loop $LoopSeconds --log `"$serviceLogPath`""
}

New-Service -Name $ServiceName -BinaryPathName $binPath -DisplayName 'AWatch-rus Collector Guard' -StartupType Automatic | Out-Null
sc.exe description $ServiceName "Session-aware ActivityWatch collector guard for AWatch-rus" | Out-Null
sc.exe failure $ServiceName reset= 300 actions= restart/5000/restart/15000/restart/60000 | Out-Null
sc.exe failureflag $ServiceName 1 | Out-Null

if ($DisableRecoveryTask) {
    Write-Warning 'DisableRecoveryTask is deprecated and ignored: ActivityWatch Recovery must remain enabled as collector guard fallback.'
}
else {
    try {
        Enable-ScheduledTask -TaskName 'ActivityWatch Recovery' -ErrorAction SilentlyContinue | Out-Null
    }
    catch {
    }
}

sc.exe start $ServiceName | Out-Null

Write-Output "Collector guard service installed: $ServiceName"
Write-Output "Mode: $Mode"
Write-Output "Config: $ConfigPath"
Write-Output "Runtime: $(if (Test-Path -LiteralPath $rustTelemetryPath) { 'rust' } else { 'powershell' })"
