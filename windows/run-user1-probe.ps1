[CmdletBinding()]
param(
    [string]$UserId = 'HOST-EXAMPLE\user1'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$probeScriptPath = 'C:\ProgramData\AWatch-rus\user1-notepad-probe.ps1'
@'
Start-Process notepad.exe
Start-Sleep -Seconds 10
Get-Process notepad -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
'@ | Set-Content -LiteralPath $probeScriptPath -Encoding UTF8

schtasks /Run /TN 'ActivityWatch Launch [HOST-EXAMPLE_user1]' | Out-Null
Start-Sleep -Seconds 3

$taskName = 'AW User1 Notepad Probe'
Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
$action = New-ScheduledTaskAction -Execute (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe') -Argument "-NoProfile -ExecutionPolicy Bypass -File $probeScriptPath"
$principal = New-ScheduledTaskPrincipal -UserId $UserId -LogonType Interactive -RunLevel Highest
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
