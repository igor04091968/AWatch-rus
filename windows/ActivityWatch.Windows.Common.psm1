Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Запустите этот скрипт из PowerShell с правами администратора.'
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

function Enable-ActivityWatchPrintTelemetry {
    $policyPath = 'HKLM:\Software\Policies\Microsoft\Windows NT\Printers'
    if (-not (Test-Path -LiteralPath $policyPath)) {
        New-Item -Path $policyPath -Force | Out-Null
    }
    New-ItemProperty -Path $policyPath -Name 'ShowJobTitleInEventLogs' -Value 1 -PropertyType DWord -Force | Out-Null

    & wevtutil.exe sl 'Microsoft-Windows-PrintService/Operational' /e:true | Out-Null
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
    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    $suffix = ([guid]::NewGuid().Guid.Substring(0, 8))
    $archivePath = Join-Path $WorkingRoot ("activitywatch-{0}-{1}-{2}.zip" -f $Version.TrimStart('v'), $stamp, $suffix)
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
        throw "Не удалось найти aw-watcher-afk.exe в $ExpandedRoot."
    }

    return (Split-Path -Path (Split-Path -Path $afkBinary.FullName -Parent) -Parent)
}

function Expand-ActivityWatchArchiveSafe {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ArchivePath,
        [Parameter(Mandatory = $true)]
        [string]$DestinationPath,
        [int]$Attempts = 3
    )

    for ($attempt = 1; $attempt -le $Attempts; $attempt++) {
        try {
            if (Test-Path -LiteralPath $DestinationPath) {
                Remove-Item -LiteralPath $DestinationPath -Recurse -Force -ErrorAction SilentlyContinue
            }
            New-ActivityWatchDirectory -Path $DestinationPath
            Expand-Archive -Path $ArchivePath -DestinationPath $DestinationPath -Force -ErrorAction Stop
            return
        }
        catch {
            if ($attempt -lt $Attempts) {
                Start-Sleep -Milliseconds (500 * $attempt)
                continue
            }
        }
    }

    # Fallback for intermittent Expand-Archive issues in Windows PowerShell.
    if (Test-Path -LiteralPath $DestinationPath) {
        Remove-Item -LiteralPath $DestinationPath -Recurse -Force -ErrorAction SilentlyContinue
    }
    New-ActivityWatchDirectory -Path $DestinationPath
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [System.IO.Compression.ZipFile]::ExtractToDirectory($ArchivePath, $DestinationPath)
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

    # Cleanup stale extraction directories from previous failed deployments.
    Get-ChildItem -LiteralPath $WorkingRoot -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -like 'extract-*' } |
        ForEach-Object {
            try { Remove-Item -LiteralPath $_.FullName -Recurse -Force -ErrorAction SilentlyContinue } catch {}
        }

    # Ensure nothing is holding locks inside InstallRoot during upgrade.
    foreach ($procName in @('aw-watcher-afk', 'aw-watcher-window', 'aw-server', 'aw-qt')) {
        try {
            Get-Process -Name $procName -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
        }
        catch {
        }
    }
    Start-Sleep -Seconds 2

    $extractRoot = Join-Path $WorkingRoot ('extract-' + [guid]::NewGuid().Guid)
    if (Test-Path -LiteralPath $extractRoot) {
        Remove-Item -LiteralPath $extractRoot -Recurse -Force
    }
    New-ActivityWatchDirectory -Path $extractRoot

    $archiveSize = (Get-Item -LiteralPath $ArchivePath -ErrorAction Stop).Length
    $workDrive = (Get-PSDrive -Name ([System.IO.Path]::GetPathRoot($WorkingRoot).TrimEnd('\').TrimEnd(':')) -ErrorAction SilentlyContinue)
    if ($workDrive) {
        # Require at least ~2.5x archive size to handle extraction + copy safely.
        $required = [int64]([Math]::Ceiling($archiveSize * 2.5))
        if ([int64]$workDrive.Free -lt $required) {
            throw ("Недостаточно свободного места на {0}: free={1} bytes, required>={2} bytes" -f $workDrive.Name, $workDrive.Free, $required)
        }
    }

    Expand-ActivityWatchArchiveSafe -ArchivePath $ArchivePath -DestinationPath $extractRoot
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
            throw "Не найден обязательный исполняемый файл ActivityWatch: $($entry.Value)"
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
        throw 'Не удалось определить целевых пользователей. Укажите -Users или -UserListPath.'
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
        [string]$PolicyClientScriptSource,
        [Parameter(Mandatory = $true)]
        [string]$FileCollectorScriptSource,
        [Parameter(Mandatory = $true)]
        [string]$SessionCollectorScriptSource,
        [string]$EmailCollectorScriptSource,
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
    $policyClientTarget = Join-Path $StateRoot 'dlp-policy-client.ps1'
    $fileCollectorTarget = Join-Path $StateRoot 'file-operations-collector.ps1'
    $sessionCollectorTarget = Join-Path $StateRoot 'worktime-session-collector.ps1'
    $emailCollectorTarget = Join-Path $StateRoot 'email-outbound-collector.ps1'
    $exampleRulesTarget = Join-Path $StateRoot 'web-category-rules.example.json'
    $rulesTarget = Join-Path $StateRoot 'web-category-rules.json'
    $examplePolicyTarget = Join-Path $StateRoot 'dlp-policy.example.json'
    $policyTarget = Join-Path $StateRoot 'dlp-policy.json'

    Copy-Item -LiteralPath $CollectorScriptSource -Destination $collectorTarget -Force
    Copy-Item -LiteralPath $EndpointCollectorScriptSource -Destination $endpointCollectorTarget -Force
    if ($PolicyClientScriptSource -and (Test-Path -LiteralPath $PolicyClientScriptSource)) {
        Copy-Item -LiteralPath $PolicyClientScriptSource -Destination $policyClientTarget -Force
    }
    Copy-Item -LiteralPath $FileCollectorScriptSource -Destination $fileCollectorTarget -Force
    Copy-Item -LiteralPath $SessionCollectorScriptSource -Destination $sessionCollectorTarget -Force
    if ($EmailCollectorScriptSource -and (Test-Path -LiteralPath $EmailCollectorScriptSource)) {
        Copy-Item -LiteralPath $EmailCollectorScriptSource -Destination $emailCollectorTarget -Force
    }
    Copy-Item -LiteralPath $ExampleRulesSource -Destination $exampleRulesTarget -Force
    Copy-Item -LiteralPath $ExamplePolicySource -Destination $examplePolicyTarget -Force

    if ($CustomRulesSource) {
        $resolvedRules = Resolve-Path -LiteralPath $CustomRulesSource -ErrorAction Stop
        Copy-Item -LiteralPath $resolvedRules.Path -Destination $rulesTarget -Force
    }
    else {
        Copy-Item -LiteralPath $exampleRulesTarget -Destination $rulesTarget -Force
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
        PolicyClientScript      = $policyClientTarget
        FileCollectorScript     = $fileCollectorTarget
        SessionCollectorScript  = $sessionCollectorTarget
        EmailCollectorScript    = $emailCollectorTarget
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
        [string]$PolicyClientScript,
        [Parameter(Mandatory = $true)]
        [string]$FileCollectorScript,
        [Parameter(Mandatory = $true)]
        [string]$SessionCollectorScript,
        [string]$EmailCollectorScript,
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
        [bool]$AfkEnabled = $true,
        [bool]$WindowEnabled = $true,
        [bool]$FileOpsEnabled = $true,
        [bool]$LocalAgentLogsEnabled = $true,
        [bool]$IncidentCaptureEnabled = $true,
        [bool]$IncidentScreenshotEnabled = $true,
        [string]$IncidentArtifactsRoot,
        [bool]$LogonMarkerEnabled = $true,
        [Parameter(Mandatory = $true)]
        [string]$LaunchScriptPath,
        [Parameter(Mandatory = $true)]
        [string]$RecoveryScriptPath,
        [string]$AwHostname,
        [ValidateSet('local', 'server')]
        [string]$PolicyMode = 'local',
        [bool]$PolicyEngineEnabled = $false,
        [string]$PolicyEngineHost,
        [int]$PolicyEnginePort = 5601,
        [ValidateSet('http', 'https')]
        [string]$PolicyEngineScheme = 'http',
        [int]$PolicyRefreshSeconds = 300,
        [string]$PolicyCachePath,
        [Parameter(Mandatory = $true)]
        [pscustomobject[]]$UserTasks,
        [string]$PackageVersion = 'v0.13.2',
        [switch]$IntegrationTestEnabled
    )

    $effectiveIncidentArtifactsRoot = if ($IncidentArtifactsRoot) { $IncidentArtifactsRoot } else { Join-Path $StateRoot 'incident-artifacts' }
    $effectivePolicyEngineHost = if ([string]::IsNullOrWhiteSpace($PolicyEngineHost)) { $ServerHost } else { $PolicyEngineHost }
    $effectivePolicyCachePath = if ([string]::IsNullOrWhiteSpace($PolicyCachePath)) { Join-Path $StateRoot 'dlp-policy-cache.json' } else { $PolicyCachePath }

    return [pscustomobject]@{
        version  = 1
        generatedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
        awHostname = if ([string]::IsNullOrWhiteSpace($AwHostname)) { [string]$env:COMPUTERNAME } else { [string]$AwHostname }
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
            policyClientScript = $PolicyClientScript
            emailCollectorScript = $EmailCollectorScript
            fileCollectorScript = $FileCollectorScript
            sessionCollectorScript = $SessionCollectorScript
            rulesPath      = $RulesPath
            policyPath     = $PolicyPath
            launchScript   = $LaunchScriptPath
            recoveryScript = $RecoveryScriptPath
        }
        collector = [pscustomobject]@{
            pollSeconds  = $PollSeconds
            pulseSeconds = $PulseSeconds
        }
        collectors = [pscustomobject]@{
            afkEnabled   = $AfkEnabled
            windowEnabled = $WindowEnabled
            fileOpsEnabled = $FileOpsEnabled
            emailEnabled = $false
        }
        logging = [pscustomobject]@{
            localAgentLogsEnabled = $LocalAgentLogsEnabled
        }
        incidentCapture = [pscustomobject]@{
            enabled           = $IncidentCaptureEnabled
            screenshotEnabled = $IncidentScreenshotEnabled
            artifactsRoot     = $effectiveIncidentArtifactsRoot
        }
        sessionEvents = [pscustomobject]@{
            logonEnabled = $LogonMarkerEnabled
            bucketPrefix = 'aw-session-events'
        }
        recovery = [pscustomobject]@{
            intervalSeconds = $RecoveryIntervalSeconds
            taskName        = 'ActivityWatch Recovery'
        }
        dlp = [pscustomobject]@{
            incidentBucketPrefix = 'aw-dlp-incidents'
            enabled              = $true
        }
        policyEngine = [pscustomobject]@{
            enabled        = $PolicyEngineEnabled
            mode           = $PolicyMode
            host           = $effectivePolicyEngineHost
            port           = $PolicyEnginePort
            scheme         = $PolicyEngineScheme
            refreshSeconds = $PolicyRefreshSeconds
            cachePath      = $effectivePolicyCachePath
        }
        package = [pscustomobject]@{
            version = $PackageVersion
        }
        userTasks = @($UserTasks)
        integrationTestEnabled = [bool]$IntegrationTestEnabled
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
        throw "Конфигурация развёртывания не найдена: $Path"
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

[System.Net.ServicePointManager]::SecurityProtocol = [System.Net.SecurityProtocolType]::Tls12
Add-Type -AssemblyName System.Net.Http
`$script:MaxCollectorPowerShellProcesses = 24

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

function Get-CollectorPowerShellProcessCount {
    `$processes = Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
        Where-Object {
            (`$_.Name -ieq 'powershell.exe' -or `$_.Name -ieq 'pwsh.exe') -and
            `$_.CommandLine -match 'AWatch-rus' -and
            `$_.CommandLine -match '\.ps1'
        }

    return @(`$processes).Count
}

function New-LaunchLock {
    param([string]`$StateRoot, [int]`$SessionId)

    `$lockPath = Join-Path `$env:TEMP ("launch-watchers-session-{0}.lock" -f `$SessionId)
    if (Test-Path -LiteralPath `$lockPath) {
        try {
            `$lockData = Get-Content -LiteralPath `$lockPath -Raw | ConvertFrom-Json
            `$existingPid = [int]`$lockData.pid
            if (`$existingPid -gt 0 -and (Get-Process -Id `$existingPid -ErrorAction SilentlyContinue)) {
                return `$null
            }
        }
        catch {
        }
    }

    `$payload = @{
        pid       = `$PID
        sessionId = `$SessionId
        createdAt = (Get-Date).ToUniversalTime().ToString('o')
    } | ConvertTo-Json -Compress
    Set-Content -LiteralPath `$lockPath -Value `$payload -Encoding UTF8
    return `$lockPath
}

function Invoke-AwJsonPost {
    param(
        [Parameter(Mandatory = `$true)][string]`$Uri,
        [Parameter(Mandatory = `$true)][string]`$Json
    )

    `$httpClient = New-Object System.Net.Http.HttpClient
    try {
        `$content = New-Object System.Net.Http.StringContent(`$Json, [System.Text.Encoding]::UTF8, 'application/json')
        `$response = `$httpClient.PostAsync(`$Uri, `$content).Result
        if (-not `$response.IsSuccessStatusCode) {
            return `$false
        }
        return `$true
    }
    catch {
        return `$false
    }
    finally {
        `$httpClient.Dispose()
    }
}

function Ensure-Bucket {
    param(
        [string]`$BucketId,
        [string]`$ClientName,
        [string]`$BucketType
    )

    if (`$script:KnownBuckets.ContainsKey(`$BucketId)) {
        return
    }

    try {
        Invoke-RestMethod -Method Get -Uri "`$(`$script:ApiBase)/buckets/`$BucketId" | Out-Null
        `$script:KnownBuckets[`$BucketId] = `$true
        return
    }
    catch {
    }

    `$body = @{
        client   = `$ClientName
        type     = `$BucketType
        hostname = `$script:Hostname
    } | ConvertTo-Json -Compress

    try {
        if (-not (Invoke-AwJsonPost -Uri "`$(`$script:ApiBase)/buckets/`$BucketId" -Json `$body)) {
            return
        }
    }
    catch {
        try {
            Invoke-RestMethod -Method Get -Uri "`$(`$script:ApiBase)/buckets/`$BucketId" | Out-Null
        }
        catch {
            return
        }
    }

    `$script:KnownBuckets[`$BucketId] = `$true
}

function Send-LogonMarkerIfNeeded {
    param(
        [pscustomobject]`$Config,
        [int]`$SessionId
    )

    `$sessionEvents = if (`$Config.PSObject.Properties.Name -contains 'sessionEvents') { `$Config.sessionEvents } else { `$null }
    `$logging = if (`$Config.PSObject.Properties.Name -contains 'logging') { `$Config.logging } else { `$null }
    `$logonEnabled = if (`$sessionEvents -and `$sessionEvents.PSObject.Properties.Name -contains 'logonEnabled') { [bool]`$sessionEvents.logonEnabled } else { `$false }
    if (-not `$logonEnabled) {
        return
    }

    `$bucketPrefix = if (`$sessionEvents -and `$sessionEvents.PSObject.Properties.Name -contains 'bucketPrefix' -and -not [string]::IsNullOrWhiteSpace([string]`$sessionEvents.bucketPrefix)) {
        [string]`$sessionEvents.bucketPrefix
    }
    else {
        'aw-session-events'
    }

    `$stateRoot = [string]`$Config.paths.stateRoot
    `$markerRoots = New-Object System.Collections.Generic.List[string]
    if (-not [string]::IsNullOrWhiteSpace(`$env:LOCALAPPDATA)) {
        `$markerRoots.Add((Join-Path `$env:LOCALAPPDATA 'AWatch-rus\markers'))
    }
    if (-not [string]::IsNullOrWhiteSpace(`$stateRoot)) {
        `$markerRoots.Add((Join-Path `$stateRoot 'markers'))
    }

    `$markerDir = `$null
    foreach (`$candidate in `$markerRoots) {
        try {
            if (-not (Test-Path -LiteralPath `$candidate)) {
                New-Item -Path `$candidate -ItemType Directory -Force | Out-Null
            }

            `$probePath = Join-Path `$candidate 'write-test.tmp'
            Set-Content -LiteralPath `$probePath -Value 'ok' -Encoding ASCII
            Remove-Item -LiteralPath `$probePath -Force -ErrorAction SilentlyContinue
            `$markerDir = `$candidate
            break
        }
        catch {
        }
    }

    if (-not `$markerDir) {
        return
    }

    `$markerFile = Join-Path `$markerDir ("logon-{0}-{1}.marker" -f `$env:USERNAME, `$SessionId)
    if (Test-Path -LiteralPath `$markerFile) {
        return
    }

    Set-Content -LiteralPath `$markerFile -Value ((Get-Date).ToUniversalTime().ToString('o')) -Encoding UTF8

    `$bucketId = ('{0}_{1}' -f `$bucketPrefix, `$script:Hostname)
    Ensure-Bucket -BucketId `$bucketId -ClientName 'aw-session-events' -BucketType 'aw.session.event'

    `$payload = @{
        timestamp = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
        duration  = 0
        data      = @{
            eventType = 'logon'
            username  = `$env:USERNAME
            userId    = "`$(`$env:USERDOMAIN)\`$(`$env:USERNAME)"
            sessionId = `$SessionId
            hostname  = `$script:Hostname
            source    = 'launch-watchers-awatch-rus'
        }
    } | ConvertTo-Json -Depth 5 -Compress

    try {
        Invoke-AwJsonPost -Uri "`$(`$script:ApiBase)/buckets/`$bucketId/heartbeat?pulsetime=1" -Json `$payload
    }
    catch {
        Remove-Item -LiteralPath `$markerFile -Force -ErrorAction SilentlyContinue
        throw
    }
}

function Start-CollectorScriptIfNeeded {
    param(
        [string]`$ScriptPath,
        [string]`$ConfigPath,
        [string]`$PowerShellExe,
        [int]`$SessionId
    )

    if ([string]::IsNullOrWhiteSpace(`$ScriptPath)) {
        return
    }

    if (-not (Test-Path -LiteralPath `$ScriptPath)) {
        return
    }

    if (Test-CollectorRunning -ScriptPath `$ScriptPath -SessionId `$SessionId) {
        return
    }

    if ((Get-CollectorPowerShellProcessCount) -ge `$script:MaxCollectorPowerShellProcesses) {
        return
    }

    `$staParam = if (`$ScriptPath -like "*endpoint-signals*") { "-STA" } else { `$null }
    `$argumentList = @('-NoProfile', '-WindowStyle', 'Hidden', '-ExecutionPolicy', 'Bypass')
    if (`$staParam) { `$argumentList += `$staParam }
    `$argumentList += @('-File', `$ScriptPath, '-ConfigPath', `$ConfigPath)
    Start-Process -FilePath `$PowerShellExe -ArgumentList `$argumentList -WindowStyle Hidden
}

`$config = Get-DeploymentConfig -Path `$ConfigPath
`$sessionId = (Get-Process -Id `$PID).SessionId
`$installRoot = [string]`$config.paths.installRoot
`$stateRoot = [string]`$config.paths.stateRoot
`$script:ApiBase = '{0}://{1}:{2}/api/0' -f [string]`$config.server.scheme, [string]`$config.server.host, [string]`$config.server.port
`$script:Hostname = if (`$config.PSObject.Properties.Name -contains 'awHostname' -and -not [string]::IsNullOrWhiteSpace([string]`$config.awHostname)) { [string]`$config.awHostname } else { `$env:COMPUTERNAME }
`$script:KnownBuckets = @{}
`$collectorScript = [string]`$config.paths.collectorScript
`$endpointCollectorScript = if (`$config.paths.PSObject.Properties.Name -contains 'endpointCollectorScript') { [string]`$config.paths.endpointCollectorScript } else { Join-Path `$stateRoot 'dlp-endpoint-signals-collector.ps1' }
`$fileCollectorScript = if (`$config.paths.PSObject.Properties.Name -contains 'fileCollectorScript') { [string]`$config.paths.fileCollectorScript } else { Join-Path `$stateRoot 'file-operations-collector.ps1' }
`$sessionCollectorScript = if (`$config.paths.PSObject.Properties.Name -contains 'sessionCollectorScript') { [string]`$config.paths.sessionCollectorScript } else { Join-Path `$stateRoot 'worktime-session-collector.ps1' }
`$afkExe = Join-Path `$installRoot 'aw-watcher-afk\aw-watcher-afk.exe'
`$windowExe = Join-Path `$installRoot 'aw-watcher-window\aw-watcher-window.exe'
`$serverArgs = @('--host', [string]`$config.server.host, '--port', [string]`$config.server.port)
`$powershellExe = Join-Path `$env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
`$afkEnabled = if (`$config.PSObject.Properties.Name -contains 'collectors' -and `$config.collectors.PSObject.Properties.Name -contains 'afkEnabled') { [bool]`$config.collectors.afkEnabled } else { `$true }
`$windowEnabled = if (`$config.PSObject.Properties.Name -contains 'collectors' -and `$config.collectors.PSObject.Properties.Name -contains 'windowEnabled') { [bool]`$config.collectors.windowEnabled } else { `$true }
`$fileOpsEnabled = if (`$config.PSObject.Properties.Name -contains 'collectors' -and `$config.collectors.PSObject.Properties.Name -contains 'fileOpsEnabled') { [bool]`$config.collectors.fileOpsEnabled } else { `$true }
`$emailEnabled = if (`$config.PSObject.Properties.Name -contains 'collectors' -and `$config.collectors.PSObject.Properties.Name -contains 'emailEnabled') { [bool]`$config.collectors.emailEnabled } else { `$false }
`$emailCollectorScript = if (`$config.paths.PSObject.Properties.Name -contains 'emailCollectorScript') { [string]`$config.paths.emailCollectorScript } else { Join-Path `$stateRoot 'email-outbound-collector.ps1' }
`$launchLockPath = New-LaunchLock -StateRoot `$stateRoot -SessionId `$sessionId
if (-not `$launchLockPath) {
    return
}

try {
    if (`$afkEnabled -and -not (Test-Path -LiteralPath `$afkExe)) {
        throw "Не найден aw-watcher-afk.exe: `$afkExe"
    }

    if (`$windowEnabled -and -not (Test-Path -LiteralPath `$windowExe)) {
        throw "Не найден aw-watcher-window.exe: `$windowExe"
    }

    if (`$afkEnabled -and -not (Test-ProcessInSession -Name 'aw-watcher-afk' -SessionId `$sessionId)) {
        Start-Process -FilePath `$afkExe -ArgumentList `$serverArgs -WindowStyle Hidden
    }

    if (`$windowEnabled -and -not (Test-ProcessInSession -Name 'aw-watcher-window' -SessionId `$sessionId)) {
        Start-Process -FilePath `$windowExe -ArgumentList `$serverArgs -WindowStyle Hidden
    }

    try {
        Send-LogonMarkerIfNeeded -Config `$config -SessionId `$sessionId
    }
    catch {
    }
    Start-CollectorScriptIfNeeded -ScriptPath `$collectorScript -ConfigPath `$ConfigPath -PowerShellExe `$powershellExe -SessionId `$sessionId
    Start-CollectorScriptIfNeeded -ScriptPath `$endpointCollectorScript -ConfigPath `$ConfigPath -PowerShellExe `$powershellExe -SessionId `$sessionId
    if (`$fileOpsEnabled) {
        Start-CollectorScriptIfNeeded -ScriptPath `$fileCollectorScript -ConfigPath `$ConfigPath -PowerShellExe `$powershellExe -SessionId `$sessionId
    }
    if (`$emailEnabled -and (Test-Path -LiteralPath `$emailCollectorScript)) {
        Start-CollectorScriptIfNeeded -ScriptPath `$emailCollectorScript -ConfigPath `$ConfigPath -PowerShellExe `$powershellExe -SessionId `$sessionId
    }
}
finally {
    if (`$launchLockPath -and (Test-Path -LiteralPath `$launchLockPath)) {
        Remove-Item -LiteralPath `$launchLockPath -Force -ErrorAction SilentlyContinue
    }
}
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

function Get-RecoveryConfigPaths {
    param([string]`$PrimaryConfigPath)

    `$paths = New-Object System.Collections.Generic.List[string]
    if (`$PrimaryConfigPath -and (Test-Path -LiteralPath `$PrimaryConfigPath)) {
        `$paths.Add((Resolve-Path -LiteralPath `$PrimaryConfigPath).Path)
    }

    `$searchRoot = `$env:ProgramData
    if (`$PrimaryConfigPath) {
        `$stateRoot = Split-Path -Path `$PrimaryConfigPath -Parent
        `$candidateRoot = Split-Path -Path `$stateRoot -Parent
        if (`$candidateRoot -and (Test-Path -LiteralPath `$candidateRoot)) {
            `$searchRoot = `$candidateRoot
        }
    }

    if (Test-Path -LiteralPath `$searchRoot) {
        Get-ChildItem -LiteralPath `$searchRoot -Directory -ErrorAction SilentlyContinue |
            Where-Object { `$_.Name -like 'ActivityWatch*' } |
            ForEach-Object {
                `$candidate = Join-Path `$_.FullName 'deployment-config.json'
                if (Test-Path -LiteralPath `$candidate) {
                    `$paths.Add(`$candidate)
                }
            }
    }

    return @(`$paths | Sort-Object -Unique)
}

function Get-RecoveryTaskNames {
    param([string[]]`$ConfigPaths)

    `$taskNames = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    foreach (`$candidatePath in @(`$ConfigPaths)) {
        try {
            `$config = Get-DeploymentConfig -Path `$candidatePath
            foreach (`$task in @(`$config.userTasks)) {
                `$taskName = [string]`$task.launchTaskName
                if (-not [string]::IsNullOrWhiteSpace(`$taskName)) {
                    [void]`$taskNames.Add(`$taskName)
                }
            }
        }
        catch {
        }
    }

    return @(`$taskNames)
}

function New-RecoveryLock {
    param([string]`$PrimaryConfigPath)

    `$stateRoot = if (`$PrimaryConfigPath) { Split-Path -Path `$PrimaryConfigPath -Parent } else { Join-Path `$env:ProgramData 'AWatch-rus' }
    if (-not (Test-Path -LiteralPath `$stateRoot)) {
        New-Item -Path `$stateRoot -ItemType Directory -Force | Out-Null
    }

    `$lockPath = Join-Path `$stateRoot 'recovery-loop.lock'
    if (Test-Path -LiteralPath `$lockPath) {
        try {
            `$lockData = Get-Content -LiteralPath `$lockPath -Raw | ConvertFrom-Json
            `$existingPid = [int]`$lockData.pid
            if (`$existingPid -gt 0 -and (Get-Process -Id `$existingPid -ErrorAction SilentlyContinue)) {
                return `$null
            }
        }
        catch {
        }
    }

    `$payload = @{
        pid       = `$PID
        createdAt = (Get-Date).ToUniversalTime().ToString('o')
    } | ConvertTo-Json -Compress
    Set-Content -LiteralPath `$lockPath -Value `$payload -Encoding UTF8
    return `$lockPath
}

function Start-TaskIfNotRunning {
    param([string]`$TaskName)
    if ([string]::IsNullOrWhiteSpace(`$TaskName)) {
        return
    }

    try {
        `$task = Get-ScheduledTask -TaskName `$TaskName -ErrorAction SilentlyContinue
        if (-not `$task) {
            return
        }
        if ([string]`$task.State -eq 'Running') {
            return
        }
        Start-ScheduledTask -TaskName `$TaskName -ErrorAction SilentlyContinue
    }
    catch {
    }
}

function Test-CollectorRunningGlobal {
    param([string]`$ScriptPath)
    if ([string]::IsNullOrWhiteSpace(`$ScriptPath)) {
        return `$false
    }

    return [bool]@(
        Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
            Where-Object {
                (`$_.Name -ieq 'powershell.exe' -or `$_.Name -ieq 'pwsh.exe') -and
                `$_.CommandLine -match [Regex]::Escape(`$ScriptPath)
            }
    ).Count
}

function Start-CollectorScriptGlobalIfNeeded {
    param(
        [string]`$ScriptPath,
        [string]`$ConfigPath
    )

    if ([string]::IsNullOrWhiteSpace(`$ScriptPath)) {
        return
    }

    if (-not (Test-Path -LiteralPath `$ScriptPath)) {
        return
    }

    if (Test-CollectorRunningGlobal -ScriptPath `$ScriptPath) {
        return
    }

    `$powershellExe = Join-Path `$env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    `$argumentList = @('-NoProfile', '-WindowStyle', 'Hidden', '-ExecutionPolicy', 'Bypass', '-File', `$ScriptPath, '-ConfigPath', `$ConfigPath)
    Start-Process -FilePath `$powershellExe -ArgumentList `$argumentList -WindowStyle Hidden
}

`$recoveryLockPath = New-RecoveryLock -PrimaryConfigPath `$ConfigPath
if (-not `$recoveryLockPath) {
    return
}

try {
    while (`$true) {
        `$sleepSeconds = 180
        try {
            `$configPaths = Get-RecoveryConfigPaths -PrimaryConfigPath `$ConfigPath
            `$config = Get-DeploymentConfig -Path `$ConfigPath
            `$stateRoot = [string]`$config.paths.stateRoot
            `$sessionCollectorScript = if (`$config.paths.PSObject.Properties.Name -contains 'sessionCollectorScript') { [string]`$config.paths.sessionCollectorScript } else { Join-Path `$stateRoot 'worktime-session-collector.ps1' }
            Start-CollectorScriptGlobalIfNeeded -ScriptPath `$sessionCollectorScript -ConfigPath `$ConfigPath
            foreach (`$taskName in Get-RecoveryTaskNames -ConfigPaths `$configPaths) {
                Start-TaskIfNotRunning -TaskName `$taskName
            }

            if (`$config -and `$config.recovery -and `$config.recovery.intervalSeconds) {
                `$sleepSeconds = [Math]::Max([int]`$config.recovery.intervalSeconds, 30)
            }
        }
        catch {
        }

        Start-Sleep -Seconds `$sleepSeconds
    }
}
finally {
    if (`$recoveryLockPath -and (Test-Path -LiteralPath `$recoveryLockPath)) {
        Remove-Item -LiteralPath `$recoveryLockPath -Force -ErrorAction SilentlyContinue
    }
}
"@

    Set-Content -LiteralPath $Path -Value $content -Encoding UTF8
}

function Get-ActivityWatchHiddenLauncherPath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ScriptPath
    )

    $directory = Split-Path -Path $ScriptPath -Parent
    $baseName = [IO.Path]::GetFileNameWithoutExtension($ScriptPath)
    return Join-Path $directory ("{0}-hidden.vbs" -f $baseName)
}

function Write-ActivityWatchHiddenPowerShellWrapper {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [Parameter(Mandatory = $true)]
        [string]$ScriptPath,
        [Parameter(Mandatory = $true)]
        [string]$ConfigPath
    )

    $directory = Split-Path -Path $Path -Parent
    if ($directory) {
        New-ActivityWatchDirectory -Path $directory
    }

    $powershellExe = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $escapedPowerShellExe = $powershellExe.Replace('"', '""')
    $escapedScriptPath = $ScriptPath.Replace('"', '""')
    $escapedConfigPath = $ConfigPath.Replace('"', '""')

    $content = @"
Set shell = CreateObject("WScript.Shell")
shell.Run """$escapedPowerShellExe"" -NoProfile -ExecutionPolicy Bypass -File ""$escapedScriptPath"" -ConfigPath ""$escapedConfigPath""", 0, False
"@

    Set-Content -LiteralPath $Path -Value $content -Encoding ASCII
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

function Remove-ActivityWatchScheduledTask {
    param(
        [Parameter(Mandatory = $true)]
        [string]$TaskName
    )

    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false -ErrorAction SilentlyContinue
    & cmd.exe /c "schtasks /Delete /TN `"$TaskName`" /F >nul 2>&1" | Out-Null

    for ($attempt = 0; $attempt -lt 10; $attempt++) {
        $task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
        if (-not $task) {
            return
        }

        Start-Sleep -Milliseconds 300
    }
}

function Set-ActivityWatchScheduledTaskAction {
    param(
        [Parameter(Mandatory = $true)]
        [string]$TaskName,
        [Parameter(Mandatory = $true)]
        [string]$Execute,
        [Parameter(Mandatory = $true)]
        [string]$Arguments
    )

    $task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
    if (-not $task) {
        return $false
    }

    $newAction = New-ScheduledTaskAction -Execute $Execute -Argument $Arguments
    try {
        # Non-interactive update path. Avoids schtasks.exe /Change password prompt for user-bound tasks.
        Set-ScheduledTask -TaskName $TaskName -Action $newAction -ErrorAction Stop | Out-Null
        return $true
    }
    catch {
        $taskCommand = ('"{0}" {1}' -f $Execute, $Arguments)
        & schtasks.exe /Change /TN $TaskName /TR $taskCommand | Out-Null
        if ($LASTEXITCODE -ne 0) {
            throw "Не удалось обновить action задачи ${TaskName}: $($_.Exception.Message)"
        }
        return $true
    }
}

function Get-ActivityWatchScheduledTaskByCommand {
    param(
        [Parameter(Mandatory = $true)]
        [string]$TaskName,
        [string]$CommandMatch
    )

    $task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
    if ($task) {
        return $task
    }

    if ([string]::IsNullOrWhiteSpace($CommandMatch)) {
        return $null
    }

    foreach ($candidate in @(Get-ScheduledTask | Where-Object { $_.TaskName -like 'ActivityWatch Launch*' })) {
        foreach ($action in @($candidate.Actions)) {
            if ([string]$action.Arguments -like "*$CommandMatch*") {
                return $candidate
            }
        }
    }

    return $null
}

function Remove-StaleActivityWatchUserTasks {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject[]]$TaskDefinitions,
        [Parameter(Mandatory = $true)]
        [string]$LaunchScriptPath
    )

    $launcherPath = Get-ActivityWatchHiddenLauncherPath -ScriptPath $LaunchScriptPath
    $desiredTaskNames = @($TaskDefinitions | ForEach-Object { [string]$_.LaunchTaskName })

    foreach ($candidate in @(Get-ScheduledTask | Where-Object { $_.TaskName -like 'ActivityWatch Launch*' })) {
        $taskName = [string]$candidate.TaskName
        if ($desiredTaskNames -contains $taskName) {
            continue
        }

        $usesCurrentLauncher = $false
        foreach ($action in @($candidate.Actions)) {
            if ([string]$action.Arguments -like "*$launcherPath*") {
                $usesCurrentLauncher = $true
                break
            }
        }

        if ($usesCurrentLauncher) {
            Remove-ActivityWatchScheduledTask -TaskName $taskName
        }
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

    $wscriptExe = Join-Path $env:SystemRoot 'System32\wscript.exe'
    $launcherPath = Get-ActivityWatchHiddenLauncherPath -ScriptPath $LaunchScriptPath
    Write-ActivityWatchHiddenPowerShellWrapper -Path $launcherPath -ScriptPath $LaunchScriptPath -ConfigPath $ConfigPath
    Remove-StaleActivityWatchUserTasks -TaskDefinitions $TaskDefinitions -LaunchScriptPath $LaunchScriptPath

    foreach ($definition in $TaskDefinitions) {
        $action = New-ScheduledTaskAction -Execute $wscriptExe -Argument "//B //NoLogo `"$launcherPath`""
        $trigger = New-ScheduledTaskTrigger -AtLogOn -User $definition.UserId
        $principal = New-ScheduledTaskPrincipal -UserId $definition.UserId -LogonType Interactive -RunLevel Highest
        $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -StartWhenAvailable -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Hours 0)
        $existingTask = Get-ActivityWatchScheduledTaskByCommand -TaskName $definition.LaunchTaskName -CommandMatch $ConfigPath

        if ($existingTask) {
            $updated = Set-ActivityWatchScheduledTaskAction -TaskName $existingTask.TaskName -Execute $wscriptExe -Arguments $action.Arguments
            if ($updated) {
                continue
            }
        }

        Remove-ActivityWatchScheduledTask -TaskName $definition.LaunchTaskName
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

    Remove-ActivityWatchScheduledTask -TaskName $TaskName

    $wscriptExe = Join-Path $env:SystemRoot 'System32\wscript.exe'
    $launcherPath = Get-ActivityWatchHiddenLauncherPath -ScriptPath $RecoveryScriptPath
    Write-ActivityWatchHiddenPowerShellWrapper -Path $launcherPath -ScriptPath $RecoveryScriptPath -ConfigPath $ConfigPath
    $action = New-ScheduledTaskAction -Execute $wscriptExe -Argument "//B //NoLogo `"$launcherPath`""
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
        throw "icacls завершился с ошибкой для $InstallRoot"
    }

    & icacls $StateRoot /inheritance:r /grant:r '*S-1-5-18:(OI)(CI)(F)' '*S-1-5-32-544:(OI)(CI)(F)' '*S-1-5-32-545:(OI)(CI)(RX)' | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "icacls завершился с ошибкой для $StateRoot"
    }

    & icacls $LogsRoot /inheritance:r /grant:r '*S-1-5-18:(OI)(CI)(F)' '*S-1-5-32-544:(OI)(CI)(F)' '*S-1-5-32-545:(OI)(CI)(M)' | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "icacls завершился с ошибкой для $LogsRoot"
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
