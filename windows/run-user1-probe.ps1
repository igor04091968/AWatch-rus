[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\AWatch-rus\deployment-config.json',
    [string]$User = 'user1',
    [string]$UserId,
    [string]$AwHostname
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$probeScriptPath = 'C:\ProgramData\AWatch-rus\user1-notepad-probe.ps1'
@'
Start-Process notepad.exe
Start-Sleep -Seconds 10
Get-Process notepad -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
'@ | Set-Content -LiteralPath $probeScriptPath -Encoding UTF8

$config = $null
if (Test-Path -LiteralPath $ConfigPath) {
    $config = Get-Content -Raw -LiteralPath $ConfigPath | ConvertFrom-Json
}
$logicalHost = if (-not [string]::IsNullOrWhiteSpace($AwHostname)) {
    $AwHostname
}
elseif ($config -and $config.PSObject.Properties.Name -contains 'awHostname' -and -not [string]::IsNullOrWhiteSpace([string]$config.awHostname)) {
    [string]$config.awHostname
}
else {
    [string]$env:COMPUTERNAME
}
$accountDomain = if (-not [string]::IsNullOrWhiteSpace($env:USERDOMAIN)) { [string]$env:USERDOMAIN } else { [string]$env:COMPUTERNAME }
$effectiveUserId = if (-not [string]::IsNullOrWhiteSpace($UserId)) { $UserId } else { '{0}\{1}' -f $accountDomain, $User }
$launchTaskName = 'ActivityWatch Launch [{0}_{1}]' -f $logicalHost, $User

schtasks /Run /TN $launchTaskName | Out-Null
Start-Sleep -Seconds 3

$taskName = 'AW User1 Notepad Probe'
Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
$action = New-ScheduledTaskAction -Execute (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe') -Argument "-NoProfile -ExecutionPolicy Bypass -File $probeScriptPath"
$principal = New-ScheduledTaskPrincipal -UserId $effectiveUserId -LogonType Interactive -RunLevel Highest
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -StartWhenAvailable -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Minutes 5)
Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings | Out-Null
Start-ScheduledTask -TaskName $taskName
Start-Sleep -Seconds 5

Write-Host 'QUSER'
quser
Write-Host '---'
Write-Host 'WATCHERS'
Get-Process aw-watcher-afk,aw-watcher-window,powershell,notepad -ErrorAction SilentlyContinue |
    Select-Object Name, Id, SessionId, StartTime |
    Sort-Object SessionId, Name |
    Format-Table -AutoSize

Start-Sleep -Seconds 8
Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $probeScriptPath -Force -ErrorAction SilentlyContinue
