[CmdletBinding()]
param(
  [string]$IsccPath = "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
  [string]$IssPath = "$PSScriptRoot\AWatch-rus-Setup.iss",
  [switch]$SkipClean
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $IssPath)) {
  throw "ISS file not found: $IssPath"
}
if (-not (Test-Path -LiteralPath $IsccPath)) {
  throw "ISCC.exe not found: $IsccPath"
}

$outputDir = Join-Path $PSScriptRoot 'output'
if ((-not $SkipClean) -and (Test-Path -LiteralPath $outputDir)) {
  Remove-Item -LiteralPath $outputDir -Recurse -Force
}

Write-Host "Building installer from: $IssPath"
& $IsccPath $IssPath
if ($LASTEXITCODE -ne 0) {
  throw "Inno Setup build failed with code: $LASTEXITCODE"
}

$artifact = Get-ChildItem -Path $outputDir -Filter '*.exe' -File | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $artifact) {
  throw "Build finished but no .exe artifact found in: $outputDir"
}

Write-Host "Installer artifact: $($artifact.FullName)"
