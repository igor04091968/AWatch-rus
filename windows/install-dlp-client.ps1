param(
  [Parameter(Mandatory = $true)][string]$ServerHost,
  [int]$ServerPort = 5600,
  [string]$InstallRoot = "C:\ProgramData\AWatch-rus"
)

$ErrorActionPreference = "Stop"

New-Item -ItemType Directory -Path $InstallRoot -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $InstallRoot "logs") -Force | Out-Null

$configPath = Join-Path $InstallRoot "deployment-config.json"
$policyPath = Join-Path $InstallRoot "dlp-policy.json"

$cfg = @{
  server = @{
    host = $ServerHost
    port = $ServerPort
    apiBase = "http://$ServerHost`:$ServerPort/api/0"
  }
  paths = @{
    logsRoot = (Join-Path $InstallRoot "logs")
  }
  dlp = @{
    policyMode = "server"
  }
  localAgentLogsEnabled = $true
}
$cfg | ConvertTo-Json -Depth 8 | Set-Content -Path $configPath -Encoding UTF8

if (-not (Test-Path $policyPath)) {
  @{
    version = 1
    defaults = @{
      enabled = $true
      action = "log"
      severity = "low"
      cooldownSeconds = 300
    }
    rules = @()
  } | ConvertTo-Json -Depth 8 | Set-Content -Path $policyPath -Encoding UTF8
}

Write-Host "DLP client config written: $configPath"
