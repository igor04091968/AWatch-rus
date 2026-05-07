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

        Start-CollectorIfNeeded -ScriptPath ([string]$paths.collectorScript) -ConfigPath $ConfigPath
        Start-CollectorIfNeeded -ScriptPath ([string]$paths.endpointCollectorScript) -ConfigPath $ConfigPath
        Start-CollectorIfNeeded -ScriptPath ([string]$paths.fileCollectorScript) -ConfigPath $ConfigPath
        if ($paths.PSObject.Properties.Name -contains 'emailCollectorScript') {
            Start-CollectorIfNeeded -ScriptPath ([string]$paths.emailCollectorScript) -ConfigPath $ConfigPath
        }
        if ($paths.PSObject.Properties.Name -contains 'sessionCollectorScript') {
            Start-CollectorIfNeeded -ScriptPath ([string]$paths.sessionCollectorScript) -ConfigPath $ConfigPath
        }
    }
    catch {
        Write-ServiceLog ("loop error: {0}" -f $_.Exception.Message)
    }
    Start-Sleep -Seconds ([Math]::Max($LoopSeconds, 5))
}

