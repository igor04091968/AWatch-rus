[CmdletBinding()]
param(
  [Parameter(Mandatory=$true)][string]$ServerHost,
  [int]$ServerPort = 5600,
  [Parameter(Mandatory=$true)][string]$Domain,
  [Parameter(Mandatory=$true)][string]$Users,
  [string]$InstallRoot = 'C:\Program Files\ActivityWatch',
  [string]$StateRoot = 'C:\ProgramData\ActivityWatch',
  [string]$CustomRulesPath,
  [string]$CustomPolicyPath,
  [bool]$AfkEnabled = $true,
  [bool]$WindowEnabled = $true
)

$ErrorActionPreference = 'Stop'

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$ensembleScript = Join-Path $scriptRoot 'deploy-ensemble.ps1'

if (-not (Test-Path -LiteralPath $ensembleScript)) {
  throw "deploy-ensemble.ps1 not found at $ensembleScript"
}

$usersList = $Users.Split(',') | ForEach-Object { $_.Trim() } | Where-Object { $_ }
if (-not $usersList -or $usersList.Count -eq 0) {
  throw 'Users must contain at least one username.'
}

$params = @{
  ServerHost = $ServerHost
  ServerPort = $ServerPort
  Domain = $Domain
  Users = $usersList
  InstallRoot = $InstallRoot
  StateRoot = $StateRoot
  AfkEnabled = $AfkEnabled
  WindowEnabled = $WindowEnabled
}

if ($CustomRulesPath -and (Test-Path -LiteralPath $CustomRulesPath)) {
  $params['CustomRulesPath'] = $CustomRulesPath
}
if ($CustomPolicyPath -and (Test-Path -LiteralPath $CustomPolicyPath)) {
  $params['CustomPolicyPath'] = $CustomPolicyPath
}

& $ensembleScript @params
