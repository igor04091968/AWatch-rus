Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Run this script from an elevated PowerShell session.'
    }
}

function New-ActivityWatchDirectory {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -Path $Path -ItemType Directory -Force | Out-Null
    }
}

function Get-ActivityWatchPackageUrl {
    param(
        [string]$Version = 'v0.13.2'
    )

    return "https://github.com/ActivityWatch/activitywatch/releases/download/$Version/activitywatch-$Version-windows-x86_64.zip"
}

function Get-ActivityWatchArchive {
    param(
        [string]$PackageZipPath,
        [string]$PackageUrl,
        [string]$Version = 'v0.13.2',
        [Parameter(Mandatory = $true)]
        [string]$WorkingRoot
    )

    New-ActivityWatchDirectory -Path $WorkingRoot

    if ($PackageZipPath) {
        $resolved = Resolve-Path -LiteralPath $PackageZipPath -ErrorAction Stop
        return $resolved.Path
    }

    if (-not $PackageUrl) {
        $PackageUrl = Get-ActivityWatchPackageUrl -Version $Version
    }

    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    $archivePath = Join-Path $WorkingRoot ("activitywatch-{0}.zip" -f $Version.TrimStart('v'))
    Invoke-WebRequest -Uri $PackageUrl -OutFile $archivePath
    return $archivePath
}

function Get-ActivityWatchPackageRoot {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ExpandedRoot
    )

    $afkBinary = Get-ChildItem -Path $ExpandedRoot -Filter 'aw-watcher-afk.exe' -File -Recurse |
        Select-Object -First 1

    if (-not $afkBinary) {
        throw "Cannot find aw-watcher-afk.exe under $ExpandedRoot."
    }

    return (Split-Path -Path (Split-Path -Path $afkBinary.FullName -Parent) -Parent)
}

function Install-ActivityWatchPackage {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ArchivePath,
        [Parameter(Mandatory = $true)]
        [string]$InstallRoot,
        [Parameter(Mandatory = $true)]
        [string]$WorkingRoot,
        [Parameter(Mandatory = $true)]
        [string]$BackupRoot
    )

    New-ActivityWatchDirectory -Path $WorkingRoot
    New-ActivityWatchDirectory -Path $BackupRoot

    $extractRoot = Join-Path $WorkingRoot ('extract-' + [guid]::NewGuid().Guid)
    if (Test-Path -LiteralPath $extractRoot) {
        Remove-Item -LiteralPath $extractRoot -Recurse -Force
    }
    New-ActivityWatchDirectory -Path $extractRoot

    Expand-Archive -Path $ArchivePath -DestinationPath $extractRoot -Force
    $packageRoot = Get-ActivityWatchPackageRoot -ExpandedRoot $extractRoot

    if (Test-Path -LiteralPath $InstallRoot) {
        $existingItems = Get-ChildItem -LiteralPath $InstallRoot -Force -ErrorAction SilentlyContinue
        if ($existingItems) {
            $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
            $backupPath = Join-Path $BackupRoot ("install-$stamp")
            New-ActivityWatchDirectory -Path $backupPath
            Copy-Item -Path (Join-Path $InstallRoot '*') -Destination $backupPath -Recurse -Force
            Get-ChildItem -LiteralPath $InstallRoot -Force | Remove-Item -Recurse -Force
        }
    }
    else {
        New-ActivityWatchDirectory -Path $InstallRoot
    }

    Copy-Item -Path (Join-Path $packageRoot '*') -Destination $InstallRoot -Recurse -Force

    return [pscustomobject]@{
        PackageRoot = $packageRoot
        ExtractRoot = $extractRoot
        BackupRoot  = $BackupRoot
    }
}

function Get-ActivityWatchExecutableMap {
    param(
        [Parameter(Mandatory = $true)]
        [string]$InstallRoot
    )

    $map = [ordered]@{
        Afk    = Join-Path $InstallRoot 'aw-watcher-afk\aw-watcher-afk.exe'
        Window = Join-Path $InstallRoot 'aw-watcher-window\aw-watcher-window.exe'
    }

    foreach ($entry in $map.GetEnumerator()) {
        if (-not (Test-Path -LiteralPath $entry.Value)) {
            throw "Missing required ActivityWatch binary: $($entry.Value)"
        }
    }

    return [pscustomobject]$map
}

function Normalize-ActivityWatchUsers {
    param(
        [string[]]$Users,
        [string]$UserListPath,
        [string]$Domain
    )

    $collected = New-Object System.Collections.Generic.List[string]

    if ($Users) {
        foreach ($user in $Users) {
            if (-not [string]::IsNullOrWhiteSpace($user)) {
                $collected.Add($user.Trim())
            }
        }
    }

    if ($UserListPath) {
        $resolved = Resolve-Path -LiteralPath $UserListPath -ErrorAction Stop
        $extension = [IO.Path]::GetExtension($resolved.Path)
        if ($extension -ieq '.csv') {
            $rows = Import-Csv -LiteralPath $resolved.Path
            foreach ($row in $rows) {
                foreach ($column in 'User', 'Username', 'SamAccountName', 'Login') {
                    if ($row.PSObject.Properties.Name -contains $column) {
                        $value = [string]$row.$column
                        if (-not [string]::IsNullOrWhiteSpace($value)) {
                            $collected.Add($value.Trim())
                            break
                        }
                    }
                }
            }
        }
        else {
            Get-Content -LiteralPath $resolved.Path | ForEach-Object {
                $line = $_.Trim()
                if ($line -and -not $line.StartsWith('#')) {
                    $collected.Add($line)
                }
            }
        }
    }

    $normalized = $collected |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
        ForEach-Object {
            if ($Domain -and ($_ -notmatch '[\\@]')) {
                '{0}\{1}' -f $Domain, $_
            }
            else {
                $_
            }
        } |
        Sort-Object -Unique

    if (-not $normalized -or $normalized.Count -eq 0) {
        throw 'No target users resolved. Provide -Users or -UserListPath.'
    }

    return @($normalized)
}

function Get-ActivityWatchTaskNameToken {
    param(
        [Parameter(Mandatory = $true)]
        [string]$UserId
    )

    $buffer = [Text.StringBuilder]::new()
    foreach ($character in $UserId.ToCharArray()) {
        if ([char]::IsLetterOrDigit($character)) {
            [void]$buffer.Append($character)
        }
        else {
            [void]$buffer.Append('_')
        }
    }

    return $buffer.ToString().Trim('_')
}

function New-ActivityWatchUserTaskDefinitions {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Users
    )

    $result = foreach ($user in $Users) {
        $token = Get-ActivityWatchTaskNameToken -UserId $user
        [pscustomobject]@{
            UserId         = $user
            LaunchTaskName = "ActivityWatch Launch [$token]"
        }
    }

    return @($result)
}

function Copy-ActivityWatchCollectorAssets {
    param(
        [Parameter(Mandatory = $true)]
        [string]$CollectorScriptSource,
        [Parameter(Mandatory = $true)]
        [string]$EndpointCollectorScriptSource,
        [Parameter(Mandatory = $true)]
        [string]$ExampleRulesSource,
        [Parameter(Mandatory = $true)]
        [string]$ExamplePolicySource,
        [Parameter(Mandatory = $true)]
        [string]$StateRoot,
        [string]$CustomRulesSource,
        [string]$CustomPolicySource
    )

    New-ActivityWatchDirectory -Path $StateRoot

    $collectorTarget = Join-Path $StateRoot 'browser-domains-native-collector.ps1'
    $endpointCollectorTarget = Join-Path $StateRoot 'dlp-endpoint-signals-collector.ps1'
    $exampleRulesTarget = Join-Path $StateRoot 'web-category-rules.example.json'
    $rulesTarget = Join-Path $StateRoot 'web-category-rules.json'
    $examplePolicyTarget = Join-Path $StateRoot 'dlp-policy.example.json'
    $policyTarget = Join-Path $StateRoot 'dlp-policy.json'

    Copy-Item -LiteralPath $CollectorScriptSource -Destination $collectorTarget -Force
    Copy-Item -LiteralPath $EndpointCollectorScriptSource -Destination $endpointCollectorTarget -Force
    Copy-Item -LiteralPath $ExampleRulesSource -Destination $exampleRulesTarget -Force
    Copy-Item -LiteralPath $ExamplePolicySource -Destination $examplePolicyTarget -Force

    if ($CustomRulesSource) {
        $resolvedRules = Resolve-Path -LiteralPath $CustomRulesSource -ErrorAction Stop
        Copy-Item -LiteralPath $resolvedRules.Path -Destination $rulesTarget -Force
    }

    if ($CustomPolicySource) {
        $resolvedPolicy = Resolve-Path -LiteralPath $CustomPolicySource -ErrorAction Stop
        Copy-Item -LiteralPath $resolvedPolicy.Path -Destination $policyTarget -Force
    }
    else {
        Copy-Item -LiteralPath $examplePolicyTarget -Destination $policyTarget -Force
    }

    return [pscustomobject]@{
        CollectorScript         = $collectorTarget
        EndpointCollectorScript = $endpointCollectorTarget
        ExampleRules            = $exampleRulesTarget
        ActiveRules             = $rulesTarget
        ExamplePolicy           = $examplePolicyTarget
        ActivePolicy            = $policyTarget
    }
}

function New-ActivityWatchDeploymentConfig {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ServerHost,
        [Parameter(Mandatory = $true)]
        [int]$ServerPort,
        [Parameter(Mandatory = $true)]
        [string]$ServerScheme,
        [Parameter(Mandatory = $true)]
        [string]$InstallRoot,
        [Parameter(Mandatory = $true)]
        [string]$StateRoot,
        [Parameter(Mandatory = $true)]
        [string]$LogsRoot,
        [Parameter(Mandatory = $true)]
        [string]$CollectorScript,
        [Parameter(Mandatory = $true)]
        [string]$EndpointCollectorScript,
        [Parameter(Mandatory = $true)]
        [string]$RulesPath,
        [Parameter(Mandatory = $true)]
        [string]$PolicyPath,
        [Parameter(Mandatory = $true)]
        [int]$PollSeconds,
        [Parameter(Mandatory = $true)]
        [int]$PulseSeconds,
        [Parameter(Mandatory = $true)]
        [int]$RecoveryIntervalSeconds,
        [Parameter(Mandatory = $true)]
        [string]$LaunchScriptPath,
        [Parameter(Mandatory = $true)]
        [string]$RecoveryScriptPath,
        [Parameter(Mandatory = $true)]
        [pscustomobject[]]$UserTasks,
        [string]$PackageVersion = 'v0.13.2'
    )

    return [pscustomobject]@{
        version  = 1
        generatedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
        server   = [pscustomobject]@{
            host   = $ServerHost
            port   = $ServerPort
            scheme = $ServerScheme
        }
        paths    = [pscustomobject]@{
            installRoot    = $InstallRoot
            stateRoot      = $StateRoot
            logsRoot       = $LogsRoot
            collectorScript = $CollectorScript
            endpointCollectorScript = $EndpointCollectorScript
            rulesPath      = $RulesPath
            policyPath     = $PolicyPath
            launchScript   = $LaunchScriptPath
            recoveryScript = $RecoveryScriptPath
        }
        collector = [pscustomobject]@{
            pollSeconds  = $PollSeconds
            pulseSeconds = $PulseSeconds
        }
        recovery = [pscustomobject]@{
            intervalSeconds = $RecoveryIntervalSeconds
            taskName        = 'ActivityWatch Recovery'
        }
        dlp = [pscustomobject]@{
            incidentBucketPrefix = 'aw-dlp-incidents'
            enabled              = $true
        }
        package = [pscustomobject]@{
            version = $PackageVersion
        }
        userTasks = @($UserTasks)
    }
}

function Write-ActivityWatchDeploymentConfig {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject]$Config,
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $directory = Split-Path -Path $Path -Parent
    if ($directory) {
        New-ActivityWatchDirectory -Path $directory
    }

    $json = $Config | ConvertTo-Json -Depth 8
    Set-Content -LiteralPath $Path -Value $json -Encoding UTF8
}

function Read-ActivityWatchDeploymentConfig {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        throw "Deployment config not found: $Path"
    }

    return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
}

function Write-ActivityWatchLaunchScript {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [Parameter(Mandatory = $true)]
        [string]$ConfigPath
    )

    $content = @"
param(
    [string]`$ConfigPath = '$ConfigPath'
)

Set-StrictMode -Version Latest
`$ErrorActionPreference = 'Stop'

function Get-DeploymentConfig {
    param([string]`$Path)
    return Get-Content -LiteralPath `$Path -Raw | ConvertFrom-Json
}

function Test-ProcessInSession {
    param(
        [string]`$Name,
        [int]`$SessionId
    )

    return [bool](Get-Process -Name `$Name -ErrorAction SilentlyContinue | Where-Object { `$_.SessionId -eq `$SessionId } | Select-Object -First 1)
}

function Test-CollectorRunning {
    param(
        [string]`$ScriptPath,
        [int]`$SessionId
    )

    `$escapedCollector = [Regex]::Escape(`$ScriptPath)
    `$processes = Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
        Where-Object {
            (`$_.Name -ieq 'powershell.exe' -or `$_.Name -ieq 'pwsh.exe') -and
            `$_.SessionId -eq `$SessionId -and
            `$_.CommandLine -match `$escapedCollector
        }

    return [bool](`$processes | Select-Object -First 1)
}

function Start-CollectorScriptIfNeeded {
    param(
        [string]`$ScriptPath,
        [string]`$ConfigPath,
        [string]`$PowerShellExe,
        [int]`$SessionId
    )

    if (-not (Test-Path -LiteralPath `$ScriptPath)) {
        return
    }

    if (Test-CollectorRunning -ScriptPath `$ScriptPath -SessionId `$SessionId) {
        return
    }

    Start-Process -FilePath `$PowerShellExe -ArgumentList @(
        '-NoProfile',
        '-WindowStyle', 'Hidden',
        '-ExecutionPolicy', 'Bypass',
        '-File', `$ScriptPath,
        '-ConfigPath', `$ConfigPath
    ) -WindowStyle Hidden
}

`$config = Get-DeploymentConfig -Path `$ConfigPath
`$sessionId = (Get-Process -Id `$PID).SessionId
`$installRoot = [string]`$config.paths.installRoot
`$collectorScript = [string]`$config.paths.collectorScript
`$endpointCollectorScript = if (`$config.paths.PSObject.Properties.Name -contains 'endpointCollectorScript') { [string]`$config.paths.endpointCollectorScript } else { '' }
`$afkExe = Join-Path `$installRoot 'aw-watcher-afk\aw-watcher-afk.exe'
`$windowExe = Join-Path `$installRoot 'aw-watcher-window\aw-watcher-window.exe'
`$serverArgs = @('--host', [string]`$config.server.host, '--port', [string]`$config.server.port)
`$powershellExe = Join-Path `$env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'

if (-not (Test-Path -LiteralPath `$afkExe)) {
    throw "Missing aw-watcher-afk.exe: `$afkExe"
}

if (-not (Test-Path -LiteralPath `$windowExe)) {
    throw "Missing aw-watcher-window.exe: `$windowExe"
}

if (-not (Test-ProcessInSession -Name 'aw-watcher-afk' -SessionId `$sessionId)) {
    Start-Process -FilePath `$afkExe -ArgumentList `$serverArgs -WindowStyle Hidden
}

if (-not (Test-ProcessInSession -Name 'aw-watcher-window' -SessionId `$sessionId)) {
    Start-Process -FilePath `$windowExe -ArgumentList `$serverArgs -WindowStyle Hidden
}

Start-CollectorScriptIfNeeded -ScriptPath `$collectorScript -ConfigPath `$ConfigPath -PowerShellExe `$powershellExe -SessionId `$sessionId
Start-CollectorScriptIfNeeded -ScriptPath `$endpointCollectorScript -ConfigPath `$ConfigPath -PowerShellExe `$powershellExe -SessionId `$sessionId
"@

    Set-Content -LiteralPath $Path -Value $content -Encoding UTF8
}

function Write-ActivityWatchRecoveryScript {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [Parameter(Mandatory = $true)]
        [string]$ConfigPath
    )

    $content = @"
param(
    [string]`$ConfigPath = '$ConfigPath'
)

Set-StrictMode -Version Latest
`$ErrorActionPreference = 'Continue'

function Get-DeploymentConfig {
    param([string]`$Path)
    return Get-Content -LiteralPath `$Path -Raw | ConvertFrom-Json
}

while (`$true) {
    try {
        `$config = Get-DeploymentConfig -Path `$ConfigPath
        foreach (`$task in @(`$config.userTasks)) {
            Start-ScheduledTask -TaskName ([string]`$task.launchTaskName) -ErrorAction SilentlyContinue
        }
    }
    catch {
    }

    `$config = Get-DeploymentConfig -Path `$ConfigPath
    Start-Sleep -Seconds ([int]`$config.recovery.intervalSeconds)
}
"@

    Set-Content -LiteralPath $Path -Value $content -Encoding UTF8
}

function Remove-LegacyActivityWatchEntries {
    $legacyTaskNames = @(
        'ActivityWatch Watchers',
        'ActivityWatch Guard',
        'ActivityWatch Heal'
    )

    foreach ($taskName in $legacyTaskNames) {
        Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
    }

    $runKey = 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Run'
    foreach ($name in 'ActivityWatchAFK', 'ActivityWatchWindow', 'ActivityWatchBrowserCollector') {
        Remove-ItemProperty -Path $runKey -Name $name -ErrorAction SilentlyContinue
    }
}

function Register-ActivityWatchUserTasks {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject[]]$TaskDefinitions,
        [Parameter(Mandatory = $true)]
        [string]$LaunchScriptPath,
        [Parameter(Mandatory = $true)]
        [string]$ConfigPath
    )

    $powershellExe = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'

    foreach ($definition in $TaskDefinitions) {
        Unregister-ScheduledTask -TaskName $definition.LaunchTaskName -Confirm:$false -ErrorAction SilentlyContinue

        $action = New-ScheduledTaskAction -Execute $powershellExe -Argument "-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$LaunchScriptPath`" -ConfigPath `"$ConfigPath`""
        $trigger = New-ScheduledTaskTrigger -AtLogOn -User $definition.UserId
        $principal = New-ScheduledTaskPrincipal -UserId $definition.UserId -LogonType InteractiveToken -RunLevel Highest
        $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -StartWhenAvailable -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Hours 0)

        Register-ScheduledTask -TaskName $definition.LaunchTaskName -Action $action -Trigger $trigger -Principal $principal -Settings $settings | Out-Null
    }
}

function Register-ActivityWatchRecoveryTask {
    param(
        [Parameter(Mandatory = $true)]
        [string]$TaskName,
        [Parameter(Mandatory = $true)]
        [string]$RecoveryScriptPath,
        [Parameter(Mandatory = $true)]
        [string]$ConfigPath
    )

    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false -ErrorAction SilentlyContinue

    $powershellExe = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $action = New-ScheduledTaskAction -Execute $powershellExe -Argument "-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$RecoveryScriptPath`" -ConfigPath `"$ConfigPath`""
    $trigger = New-ScheduledTaskTrigger -AtStartup
    $principal = New-ScheduledTaskPrincipal -UserId 'SYSTEM' -LogonType ServiceAccount -RunLevel Highest
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -StartWhenAvailable -Hidden -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Hours 0)

    Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Principal $principal -Settings $settings | Out-Null
}

function Set-ActivityWatchAcl {
    param(
        [Parameter(Mandatory = $true)]
        [string]$InstallRoot,
        [Parameter(Mandatory = $true)]
        [string]$StateRoot,
        [Parameter(Mandatory = $true)]
        [string]$LogsRoot
    )

    foreach ($path in $InstallRoot, $StateRoot, $LogsRoot) {
        New-ActivityWatchDirectory -Path $path
    }

    & icacls $InstallRoot /inheritance:r /grant:r '*S-1-5-18:(OI)(CI)(F)' '*S-1-5-32-544:(OI)(CI)(F)' '*S-1-5-32-545:(OI)(CI)(RX)' | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "icacls failed for $InstallRoot"
    }

    & icacls $StateRoot /inheritance:r /grant:r '*S-1-5-18:(OI)(CI)(F)' '*S-1-5-32-544:(OI)(CI)(F)' '*S-1-5-32-545:(OI)(CI)(RX)' | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "icacls failed for $StateRoot"
    }

    & icacls $LogsRoot /inheritance:r /grant:r '*S-1-5-18:(OI)(CI)(F)' '*S-1-5-32-544:(OI)(CI)(F)' '*S-1-5-32-545:(OI)(CI)(M)' | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "icacls failed for $LogsRoot"
    }
}

function Start-ActivityWatchTasks {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject[]]$TaskDefinitions,
        [string]$RecoveryTaskName = 'ActivityWatch Recovery'
    )

    foreach ($definition in $TaskDefinitions) {
        Start-ScheduledTask -TaskName $definition.LaunchTaskName -ErrorAction SilentlyContinue
    }

    Start-ScheduledTask -TaskName $RecoveryTaskName -ErrorAction SilentlyContinue
}

Export-ModuleMember -Function *-ActivityWatch*, Assert-Administrator, Normalize-ActivityWatchUsers, Get-ActivityWatchPackageUrl, Remove-LegacyActivityWatchEntries
