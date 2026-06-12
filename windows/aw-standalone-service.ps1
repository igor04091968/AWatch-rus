[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\AWatch-rus\deployment-config.json',
    [int]$LoopSeconds = 20
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-Config {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        throw "Config not found: $Path"
    }
    Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
}

function Write-ServiceLog {
    param([string]$Message)
    try {
        Add-Content -LiteralPath $script:LogPath -Value ('{0} {1}' -f (Get-Date -Format s), $Message)
    }
    catch {}
}

function Start-CollectorIfNeeded {
    param(
        [string]$ScriptPath,
        [string]$ConfigPath
    )

    if ([string]::IsNullOrWhiteSpace($ScriptPath) -or -not (Test-Path -LiteralPath $ScriptPath)) {
        return
    }

    $escaped = [Regex]::Escape($ScriptPath)
    $running = Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
        Where-Object {
            $_.Name -eq 'powershell.exe' -and
            $_.CommandLine -match $escaped -and
            $_.CommandLine -match [Regex]::Escape($ConfigPath)
        } |
        Select-Object -First 1

    if ($running) {
        return
    }

    $args = @('-NoProfile', '-ExecutionPolicy', 'Bypass')
    if ($ScriptPath -like '*dlp-endpoint-signals*') {
        $args += '-STA'
    }
    $args += @('-File', $ScriptPath, '-ConfigPath', $ConfigPath)
    Start-Process -FilePath 'powershell.exe' -ArgumentList $args -WindowStyle Hidden | Out-Null
    Write-ServiceLog ("started collector: {0}" -f $ScriptPath)
}

function Test-RustCollectorRunning {
    param([string]$Subcommand)
    if ([string]::IsNullOrWhiteSpace($Subcommand)) { return $false }
    return [bool](
        Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
            Where-Object {
                $_.Name -ieq 'aw-windows-telemetry.exe' -and
                $_.CommandLine -and
                $_.CommandLine -match [Regex]::Escape($Subcommand)
            } |
            Select-Object -First 1
    )
}

function Start-RustCollectorIfNeeded {
    param(
        [string]$ExePath,
        [string]$Subcommand,
        [string]$ConfigPath
    )

    if ([string]::IsNullOrWhiteSpace($ExePath) -or [string]::IsNullOrWhiteSpace($Subcommand)) {
        return $false
    }
    if (-not (Test-Path -LiteralPath $ExePath)) {
        return $false
    }
    if (Test-RustCollectorRunning -Subcommand $Subcommand) {
        return $true
    }
    Start-Process -FilePath $ExePath -ArgumentList @($Subcommand, '--config-path', $ConfigPath, '--mode', 'enforce') -WindowStyle Hidden | Out-Null
    Write-ServiceLog ("started rust collector: {0}" -f $Subcommand)
    return $true
}

$cfg = Get-Config -Path $ConfigPath
$stateRoot = if ($cfg.paths -and $cfg.paths.stateRoot) { [string]$cfg.paths.stateRoot } else { 'C:\ProgramData\AWatch-rus' }
$logsRoot = Join-Path $stateRoot 'logs'
if (-not (Test-Path -LiteralPath $logsRoot)) {
    New-Item -Path $logsRoot -ItemType Directory -Force | Out-Null
}
$script:LogPath = Join-Path $logsRoot 'standalone-agent-service.log'

Write-ServiceLog ('service loop started, config={0}' -f $ConfigPath)

while ($true) {
    try {
        $cfg = Get-Config -Path $ConfigPath
        $paths = $cfg.paths
        $collectors = $cfg.collectors
        $telemetryExe = if ($paths.PSObject.Properties.Name -contains 'file1cTelemetryExecutable' -and -not [string]::IsNullOrWhiteSpace([string]$paths.file1cTelemetryExecutable)) { [string]$paths.file1cTelemetryExecutable } else { Join-Path $PSScriptRoot 'aw-windows-telemetry.exe' }
        $isSession0 = ([System.Diagnostics.Process]::GetCurrentProcess().SessionId -eq 0)

        # In Session 0 (SYSTEM) collectors that depend on interactive desktop/user profile
        # will crash/exit or spin in useless restart loops. Keep only headless-safe collectors here.
        $startBrowser = $true
        $startFileOps = $true
        $startEmail   = $true
        $startWorktime = $true
        $dlpEndpointMode = 'rust_primary'
        $fileOpsMode = 'rust_primary'
        if ($collectors) {
            if ($collectors.PSObject.Properties.Name -contains 'fileOpsEnabled') { $startFileOps = [bool]$collectors.fileOpsEnabled }
            if ($collectors.PSObject.Properties.Name -contains 'emailEnabled')   { $startEmail   = [bool]$collectors.emailEnabled }
            if ($collectors.PSObject.Properties.Name -contains 'worktimeSessionEnabled') { $startWorktime = [bool]$collectors.worktimeSessionEnabled }
            $dlpEndpointMode = if ($collectors.PSObject.Properties.Name -contains 'dlpEndpointMode') { [string]$collectors.dlpEndpointMode } else { 'rust_primary' }
            $fileOpsMode = if ($collectors.PSObject.Properties.Name -contains 'fileOpsMode') { [string]$collectors.fileOpsMode } else { 'rust_primary' }
            $worktimeSessionMode = if ($collectors.PSObject.Properties.Name -contains 'worktimeSessionMode') { [string]$collectors.worktimeSessionMode } else { 'powershell_primary' }
            $worktimeLegacyFallbackEnabled = if ($collectors.PSObject.Properties.Name -contains 'worktimeLegacyFallbackEnabled') { [bool]$collectors.worktimeLegacyFallbackEnabled } else { $true }
            if ($worktimeSessionMode -ieq 'rust_primary') {
                $rustAgentRunning = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object { $_.Name -ieq 'awatch-agent-rs.exe' }).Count -gt 0
                $startWorktime = $worktimeLegacyFallbackEnabled -and (-not $rustAgentRunning)
            }
        }
        if ($isSession0) {
            $startBrowser = $false
            $startFileOps = $false
            $startEmail = $false
        }

        if ($startBrowser) {
            Start-CollectorIfNeeded -ScriptPath ([string]$paths.collectorScript) -ConfigPath $ConfigPath
        }
        if ($dlpEndpointMode -ieq 'rust_primary') {
            if (-not (Start-RustCollectorIfNeeded -ExePath $telemetryExe -Subcommand 'dlp-endpoint-collector' -ConfigPath $ConfigPath)) {
                Start-CollectorIfNeeded -ScriptPath ([string]$paths.endpointCollectorScript) -ConfigPath $ConfigPath
            }
        }
        else {
            Start-CollectorIfNeeded -ScriptPath ([string]$paths.endpointCollectorScript) -ConfigPath $ConfigPath
        }
        if ($startFileOps) {
            if ($fileOpsMode -ieq 'rust_primary') {
                if (-not (Start-RustCollectorIfNeeded -ExePath $telemetryExe -Subcommand 'file-operations-collector' -ConfigPath $ConfigPath)) {
                    Start-CollectorIfNeeded -ScriptPath ([string]$paths.fileCollectorScript) -ConfigPath $ConfigPath
                }
            }
            else {
                Start-CollectorIfNeeded -ScriptPath ([string]$paths.fileCollectorScript) -ConfigPath $ConfigPath
            }
        }
        if ($paths.PSObject.Properties.Name -contains 'emailCollectorScript') {
            if ($startEmail) {
                Start-CollectorIfNeeded -ScriptPath ([string]$paths.emailCollectorScript) -ConfigPath $ConfigPath
            }
        }
        if ($paths.PSObject.Properties.Name -contains 'sessionCollectorScript') {
            if ($startWorktime) {
                Start-CollectorIfNeeded -ScriptPath ([string]$paths.sessionCollectorScript) -ConfigPath $ConfigPath
            }
        }
    }
    catch {
        Write-ServiceLog ("loop error: {0}" -f $_.Exception.Message)
    }
    Start-Sleep -Seconds ([Math]::Max($LoopSeconds, 5))
}
