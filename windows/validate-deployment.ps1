[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\AWatch-rus\deployment-config.json'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$modulePath = Join-Path $PSScriptRoot 'ActivityWatch.Windows.Common.psm1'
Import-Module $modulePath -Force

$config = Read-ActivityWatchDeploymentConfig -Path $ConfigPath
$installRoot = [string]$config.paths.installRoot
$stateRoot = [string]$config.paths.stateRoot
$collectorScript = [string]$config.paths.collectorScript
$endpointCollectorScript = if ($config.paths.PSObject.Properties.Name -contains 'endpointCollectorScript') { [string]$config.paths.endpointCollectorScript } else { Join-Path $stateRoot 'dlp-endpoint-signals-collector.ps1' }
$fileCollectorScript = if ($config.paths.PSObject.Properties.Name -contains 'fileCollectorScript') { [string]$config.paths.fileCollectorScript } else { Join-Path $stateRoot 'file-operations-collector.ps1' }
$sessionCollectorScript = if ($config.paths.PSObject.Properties.Name -contains 'sessionCollectorScript') { [string]$config.paths.sessionCollectorScript } else { Join-Path $stateRoot 'worktime-session-collector.ps1' }
$evtxExportScript = if ($config.paths.PSObject.Properties.Name -contains 'evtxExportScript') { [string]$config.paths.evtxExportScript } else { Join-Path $stateRoot 'export-evtx-for-hayabusa.ps1' }
$rulesPath = [string]$config.paths.rulesPath
$policyPath = if ($config.paths.PSObject.Properties.Name -contains 'policyPath') { [string]$config.paths.policyPath } else { Join-Path $stateRoot 'dlp-policy.json' }
$policyClientScript = if ($config.paths.PSObject.Properties.Name -contains 'policyClientScript') { [string]$config.paths.policyClientScript } else { Join-Path $stateRoot 'dlp-policy-client.ps1' }
$launchScript = [string]$config.paths.launchScript
$recoveryScript = [string]$config.paths.recoveryScript
$awHostname = if ($config.PSObject.Properties.Name -contains 'awHostname' -and -not [string]::IsNullOrWhiteSpace([string]$config.awHostname)) { [string]$config.awHostname } else { [string]$env:COMPUTERNAME }
$serverUrl = '{0}://{1}:{2}' -f [string]$config.server.scheme, [string]$config.server.host, [int]$config.server.port
$apiBase = "$serverUrl/api/0"
$pollSeconds = if ($config.PSObject.Properties.Name -contains 'collector' -and $config.collector.PSObject.Properties.Name -contains 'pollSeconds') { [int]$config.collector.pollSeconds } else { 5 }
$pulseSeconds = if ($config.PSObject.Properties.Name -contains 'collector' -and $config.collector.PSObject.Properties.Name -contains 'pulseSeconds') { [int]$config.collector.pulseSeconds } else { [Math]::Max($pollSeconds * 3, 30) }
$freshnessSeconds = [Math]::Max($pollSeconds * 3, 30)
$sessionFreshnessSeconds = [Math]::Max($pollSeconds * 4, 45)
$transportStaleSeconds = [Math]::Max($pollSeconds * 12, 180)
$queueMaxDepth = 1000

$afkExpected = if ($config.PSObject.Properties.Name -contains 'collectors' -and $config.collectors.PSObject.Properties.Name -contains 'afkEnabled') { [bool]$config.collectors.afkEnabled } else { $true }
$windowExpected = if ($config.PSObject.Properties.Name -contains 'collectors' -and $config.collectors.PSObject.Properties.Name -contains 'windowEnabled') { [bool]$config.collectors.windowEnabled } else { $true }
$fileOpsExpected = if ($config.PSObject.Properties.Name -contains 'collectors' -and $config.collectors.PSObject.Properties.Name -contains 'fileOpsEnabled') { [bool]$config.collectors.fileOpsEnabled } else { $true }

function Get-LoggedOnUsers {
    $users = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    $activeStates = @('Active', 'Активно')
    $inactiveStates = @('Disc', 'Disconnected', 'Idle', 'Listen', 'Диск', 'Откл', 'Отключен')
    try {
        $lines = & quser.exe 2>$null
        foreach ($line in @($lines)) {
            $normalized = [string]$line
            if ([string]::IsNullOrWhiteSpace($normalized)) { continue }
            $normalized = $normalized.TrimStart(' ', '>')
            if ([string]::IsNullOrWhiteSpace($normalized)) { continue }
            if ($normalized -match '^(USERNAME|ПОЛЬЗОВАТЕЛЬ)\s+') { continue }
            $parts = $normalized -split '\s+'
            if ($parts.Count -lt 1) { continue }
            $user = [string]$parts[0]
            if ([string]::IsNullOrWhiteSpace($user)) { continue }
            $state = $null
            foreach ($part in @($parts | Select-Object -Skip 1)) {
                $token = [string]$part
                if ([string]::IsNullOrWhiteSpace($token)) { continue }
                if ($activeStates -contains $token -or $inactiveStates -contains $token) {
                    $state = $token
                    break
                }
            }
            if ($null -ne $state -and $activeStates -notcontains $state) { continue }
            [void]$users.Add($user)
            [void]$users.Add(('{0}\{1}' -f $env:COMPUTERNAME, $user))
            if (-not [string]::IsNullOrWhiteSpace($env:USERDOMAIN)) {
                [void]$users.Add(('{0}\{1}' -f $env:USERDOMAIN, $user))
            }
        }
    }
    catch {
    }
    return @($users)
}

function Test-UserHasSession {
    param(
        [string]$UserId,
        [string[]]$LoggedOnUsers
    )

    if ([string]::IsNullOrWhiteSpace($UserId)) { return $false }
    $candidateIds = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    [void]$candidateIds.Add($UserId)
    $leafUser = $UserId
    if ($leafUser -match '^[^\\]+\\(.+)$') {
        $leafUser = $Matches[1]
        [void]$candidateIds.Add($leafUser)
    }
    [void]$candidateIds.Add(('{0}\{1}' -f $env:COMPUTERNAME, $leafUser))
    if (-not [string]::IsNullOrWhiteSpace($env:USERDOMAIN)) {
        [void]$candidateIds.Add(('{0}\{1}' -f $env:USERDOMAIN, $leafUser))
    }
    foreach ($candidate in @($candidateIds)) {
        if ($LoggedOnUsers -contains $candidate) { return $true }
    }
    return $false
}

function Get-CollectorProcesses {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ScriptPath
    )

    return @(
        Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
            Where-Object {
                ($_.Name -ieq 'powershell.exe' -or $_.Name -ieq 'pwsh.exe') -and
                $_.CommandLine -and
                $_.CommandLine -match [Regex]::Escape($ScriptPath)
            } |
            Select-Object @{ Name = 'Name'; Expression = { $_.Name } }, @{ Name = 'Id'; Expression = { [int]$_.ProcessId } }, @{ Name = 'SessionId'; Expression = { [int]$_.SessionId } }, @{ Name = 'CommandLine'; Expression = { [string]$_.CommandLine } }
    )
}

function Get-DuplicateProcessGroups {
    param(
        [object[]]$Processes,
        [bool]$PerSession = $true
    )

    if (-not $Processes -or @($Processes).Count -eq 0) { return @() }
    $groups = if ($PerSession) {
        $Processes | Group-Object -Property Name, SessionId
    }
    else {
        $Processes | Group-Object -Property Name
    }

    return @(
        $groups |
            Where-Object { $_.Count -gt 1 } |
            ForEach-Object {
                [pscustomobject]@{
                    name = [string]$_.Name
                    count = [int]$_.Count
                    members = @($_.Group | Select-Object Name, Id, SessionId, CommandLine)
                }
            }
    )
}

function Convert-ToUtcDate {
    param($Value)

    if ($null -eq $Value) { return $null }
    try {
        return ([DateTimeOffset]::Parse([string]$Value)).UtcDateTime
    }
    catch {
        return $null
    }
}

function Get-BucketHealth {
    param(
        [Parameter(Mandatory = $true)]
        [string]$BucketId,
        [Parameter(Mandatory = $true)]
        [int]$MaxAgeSeconds,
        [bool]$Required = $true,
        [bool]$RequireFreshEvent = $true
    )

    $events = @()
    $queryOk = $false
    $errorMessage = $null
    try {
        $response = Invoke-RestMethod -Method Get -Uri "$apiBase/buckets/$BucketId/events?limit=25" -TimeoutSec 15 -DisableKeepAlive -ErrorAction Stop
        $events = @($response)
        $queryOk = $true
    }
    catch {
        $errorMessage = $_.Exception.Message
    }

    $latestTimestampUtc = $null
    $ageSeconds = $null
    if ($events.Count -gt 0) {
        $latestTimestampUtc = @(
            $events |
                ForEach-Object { Convert-ToUtcDate $_.timestamp } |
                Where-Object { $null -ne $_ } |
                Sort-Object -Descending
        ) | Select-Object -First 1
        if ($null -ne $latestTimestampUtc) {
            $ageSeconds = [int][Math]::Floor(((Get-Date).ToUniversalTime() - $latestTimestampUtc).TotalSeconds)
        }
    }

    $hasFreshEvent = ($null -ne $ageSeconds -and $ageSeconds -le $MaxAgeSeconds)
    $hasAnyEvent = ($events.Count -gt 0)
    $ok = if (-not $Required) { $true } elseif ($RequireFreshEvent) { $queryOk -and $hasFreshEvent } else { $queryOk -and $hasAnyEvent }

    return [pscustomobject]@{
        bucketId = $BucketId
        required = [bool]$Required
        requireFreshEvent = [bool]$RequireFreshEvent
        maxAgeSeconds = [int]$MaxAgeSeconds
        queryOk = [bool]$queryOk
        latestTimestampUtc = if ($null -ne $latestTimestampUtc) { $latestTimestampUtc.ToString('o') } else { $null }
        ageSeconds = if ($null -ne $ageSeconds) { [int]$ageSeconds } else { $null }
        count = [int]$events.Count
        ok = [bool]$ok
        error = $errorMessage
    }
}

function Get-TransportQueueHealth {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [Parameter(Mandatory = $true)]
        [string]$QueuePath,
        [Parameter(Mandatory = $true)]
        [string]$LockPath,
        [Parameter(Mandatory = $true)]
        [int]$StaleAfterSeconds,
        [Parameter(Mandatory = $true)]
        [int]$MaxDepth,
        [int]$ActiveProcessCount = 0,
        [bool]$Required = $true
    )

    $queueExists = Test-Path -LiteralPath $QueuePath
    $depth = 0
    $sizeBytes = 0
    $ageSeconds = $null
    $lastWriteUtc = $null
    if ($queueExists) {
        $item = Get-Item -LiteralPath $QueuePath -ErrorAction SilentlyContinue
        if ($item) {
            $sizeBytes = [int64]$item.Length
            $lastWriteUtc = $item.LastWriteTimeUtc
            $ageSeconds = [int][Math]::Floor(((Get-Date).ToUniversalTime() - $lastWriteUtc).TotalSeconds)
        }
        try {
            $depth = [int]((Get-Content -LiteralPath $QueuePath -ErrorAction SilentlyContinue | Measure-Object).Count)
        }
        catch {
            $depth = 0
        }
    }

    $lockExists = Test-Path -LiteralPath $LockPath
    $lockHeld = $false
    if ($lockExists) {
        try {
            $lockHandle = [System.IO.File]::Open($LockPath, [System.IO.FileMode]::Open, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None)
            $lockHandle.Dispose()
        }
        catch {
            $lockHeld = $true
        }
    }

    $staleQueue = ($depth -gt 0 -and $null -ne $ageSeconds -and $ageSeconds -gt $StaleAfterSeconds -and -not $lockHeld)
    $orphanedQueue = ($depth -gt 0 -and $ActiveProcessCount -le 0 -and $null -ne $ageSeconds -and $ageSeconds -gt $StaleAfterSeconds)
    $oversizedQueue = ($depth -gt $MaxDepth)
    $ok = if (-not $Required) { $true } else { -not ($staleQueue -or $orphanedQueue -or $oversizedQueue) }

    return [pscustomobject]@{
        name = $Name
        required = [bool]$Required
        queuePath = $QueuePath
        queueExists = [bool]$queueExists
        depth = [int]$depth
        sizeBytes = [int64]$sizeBytes
        lastWriteUtc = if ($null -ne $lastWriteUtc) { $lastWriteUtc.ToString('o') } else { $null }
        ageSeconds = if ($null -ne $ageSeconds) { [int]$ageSeconds } else { $null }
        lockPath = $LockPath
        lockExists = [bool]$lockExists
        lockHeld = [bool]$lockHeld
        activeProcessCount = [int]$ActiveProcessCount
        staleAfterSeconds = [int]$StaleAfterSeconds
        maxDepth = [int]$MaxDepth
        staleQueue = [bool]$staleQueue
        orphanedQueue = [bool]$orphanedQueue
        oversizedQueue = [bool]$oversizedQueue
        ok = [bool]$ok
    }
}

function Get-TaskSnapshot {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$TaskNames
    )

    return @(
        foreach ($taskName in @($TaskNames | Sort-Object -Unique)) {
            $task = Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object { $_.TaskName -eq $taskName } | Select-Object -First 1
            if ($null -eq $task) {
                [pscustomobject]@{
                    taskName = $taskName
                    present = $false
                    enabled = $false
                    state = 'Отсутствует'
                    lastResult = $null
                    ok = $false
                }
                continue
            }

            $taskInfo = $null
            try {
                $taskInfo = Get-ScheduledTaskInfo -TaskName $task.TaskName -TaskPath $task.TaskPath -ErrorAction Stop
            }
            catch {
            }

            $enabled = $true
            try {
                if ($task.Settings.PSObject.Properties.Name -contains 'Enabled') {
                    $enabled = [bool]$task.Settings.Enabled
                }
            }
            catch {
            }

            [pscustomobject]@{
                taskName = [string]$task.TaskName
                present = $true
                enabled = [bool]$enabled
                state = [string]$task.State
                lastResult = if ($taskInfo) { [int64]$taskInfo.LastTaskResult } else { $null }
                ok = [bool]($enabled)
            }
        }
    )
}

$requiredFiles = @(
    $collectorScript,
    $endpointCollectorScript,
    $sessionCollectorScript,
    $evtxExportScript,
    $rulesPath,
    $policyPath,
    $policyClientScript,
    $launchScript,
    $recoveryScript,
    $ConfigPath
)
if ($fileOpsExpected) {
    $requiredFiles += $fileCollectorScript
}
if ($afkExpected) {
    $requiredFiles += (Join-Path $installRoot 'aw-watcher-afk\aw-watcher-afk.exe')
}
if ($windowExpected) {
    $requiredFiles += (Join-Path $installRoot 'aw-watcher-window\aw-watcher-window.exe')
}

$missingFiles = @(
    $requiredFiles |
        Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) } |
        Where-Object { -not (Test-Path -LiteralPath $_) }
)

$runningWatchers = @()
$expectedWatcherNames = @()
if ($afkExpected) { $expectedWatcherNames += 'aw-watcher-afk' }
if ($windowExpected) { $expectedWatcherNames += 'aw-watcher-window' }
if ($expectedWatcherNames.Count -gt 0) {
    $runningWatchers = @(Get-Process -Name $expectedWatcherNames -ErrorAction SilentlyContinue | Select-Object Name, Id, SessionId)
}

$sessionCollectorProcesses = @(Get-CollectorProcesses -ScriptPath $sessionCollectorScript)
$endpointCollectorProcesses = @(Get-CollectorProcesses -ScriptPath $endpointCollectorScript)
$fileCollectorProcesses = if ($fileOpsExpected) { @(Get-CollectorProcesses -ScriptPath $fileCollectorScript) } else { @() }
$browserCollectorProcesses = @(Get-CollectorProcesses -ScriptPath $collectorScript)

$loggedOnUsers = Get-LoggedOnUsers
$sessionBoundUsers = @(
    @($config.userTasks) |
        Where-Object { Test-UserHasSession -UserId ([string]$_.userId) -LoggedOnUsers $loggedOnUsers } |
        ForEach-Object { [string]$_.userId }
)
$sessionScopedExpectedCount = [int]$sessionBoundUsers.Count
$sessionScopedCollectorsRequired = ($sessionScopedExpectedCount -gt 0)

$taskNames = @()
if ($config.userTasks) {
    $taskNames += @($config.userTasks | ForEach-Object { [string]$_.launchTaskName })
}
$taskNames += [string]$config.recovery.taskName
$tasks = @(Get-TaskSnapshot -TaskNames $taskNames)

$watcherDuplicates = @(Get-DuplicateProcessGroups -Processes $runningWatchers -PerSession $true)
$sessionCollectorDuplicates = @(Get-DuplicateProcessGroups -Processes $sessionCollectorProcesses -PerSession $false)
$endpointCollectorDuplicates = @(Get-DuplicateProcessGroups -Processes $endpointCollectorProcesses -PerSession $true)
$fileCollectorDuplicates = @(Get-DuplicateProcessGroups -Processes $fileCollectorProcesses -PerSession $true)
$browserCollectorDuplicates = @(Get-DuplicateProcessGroups -Processes $browserCollectorProcesses -PerSession $true)

$watcherByName = @{}
foreach ($watcher in $runningWatchers) {
    if (-not $watcherByName.ContainsKey([string]$watcher.Name)) {
        $watcherByName[[string]$watcher.Name] = 0
    }
    $watcherByName[[string]$watcher.Name]++
}

$bucketChecks = @(
    Get-BucketHealth -BucketId ('aw-worktime-sessions_' + $awHostname) -MaxAgeSeconds $sessionFreshnessSeconds -Required $true -RequireFreshEvent $true
)
if ($sessionScopedCollectorsRequired -and $afkExpected) {
    $bucketChecks += Get-BucketHealth -BucketId ('aw-watcher-afk_' + $awHostname) -MaxAgeSeconds $freshnessSeconds -Required $true -RequireFreshEvent $false
}
if ($sessionScopedCollectorsRequired -and $windowExpected) {
    $bucketChecks += Get-BucketHealth -BucketId ('aw-watcher-window_' + $awHostname) -MaxAgeSeconds $freshnessSeconds -Required $true -RequireFreshEvent $false
}
if ($sessionScopedCollectorsRequired) {
    $bucketChecks += Get-BucketHealth -BucketId ('aw-dlp-endpoint-signals_' + $awHostname) -MaxAgeSeconds $freshnessSeconds -Required $true -RequireFreshEvent $true
}
if ($sessionScopedCollectorsRequired -and $fileOpsExpected) {
    $bucketChecks += Get-BucketHealth -BucketId ('aw-file-operations_' + $awHostname) -MaxAgeSeconds $transportStaleSeconds -Required $false -RequireFreshEvent $true
}

$queueChecks = @(
    Get-TransportQueueHealth -Name 'endpoint' -QueuePath (Join-Path $stateRoot 'dlp-endpoint-signals-queue.jsonl') -LockPath (Join-Path $stateRoot 'dlp-endpoint-signals-queue.lock') -StaleAfterSeconds $transportStaleSeconds -MaxDepth $queueMaxDepth -ActiveProcessCount @($endpointCollectorProcesses).Count -Required $sessionScopedCollectorsRequired
)
if ($fileOpsExpected) {
    $queueChecks += Get-TransportQueueHealth -Name 'fileops' -QueuePath (Join-Path $stateRoot 'file-operations-queue.jsonl') -LockPath (Join-Path $stateRoot 'file-operations-queue.lock') -StaleAfterSeconds $transportStaleSeconds -MaxDepth $queueMaxDepth -ActiveProcessCount @($fileCollectorProcesses).Count -Required $sessionScopedCollectorsRequired
}

$printServiceOperationalEnabled = $false
try {
    $printServiceLog = Get-WinEvent -ListLog 'Microsoft-Windows-PrintService/Operational' -ErrorAction Stop
    $printServiceOperationalEnabled = [bool]$printServiceLog.IsEnabled
}
catch {
}

$printJobTitlePolicyEnabled = $false
try {
    $printPolicy = Get-ItemProperty -LiteralPath 'HKLM:\Software\Policies\Microsoft\Windows NT\Printers' -Name 'ShowJobTitleInEventLogs' -ErrorAction Stop
    $printJobTitlePolicyEnabled = ([int]$printPolicy.ShowJobTitleInEventLogs -eq 1)
}
catch {
}

$watcherCountsOk = $true
if ($sessionScopedCollectorsRequired) {
    if ($afkExpected) {
        $watcherCountsOk = $watcherCountsOk -and (($watcherByName['aw-watcher-afk'] | ForEach-Object { [int]$_ }) -ge $sessionScopedExpectedCount)
    }
    if ($windowExpected) {
        $watcherCountsOk = $watcherCountsOk -and (($watcherByName['aw-watcher-window'] | ForEach-Object { [int]$_ }) -ge $sessionScopedExpectedCount)
    }
}

$endpointProcessOk = if (-not $sessionScopedCollectorsRequired) { $true } else { (@($endpointCollectorProcesses).Count -ge $sessionScopedExpectedCount) }
$fileProcessOk = if (-not $fileOpsExpected -or -not $sessionScopedCollectorsRequired) { $true } else { (@($fileCollectorProcesses).Count -ge $sessionScopedExpectedCount) }
$browserProcessOk = if (-not $sessionScopedCollectorsRequired) { $true } else { (@($browserCollectorProcesses).Count -ge $sessionScopedExpectedCount) }
$sessionCollectorOk = (@($sessionCollectorProcesses).Count -eq 1)

$result = [ordered]@{
    generatedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
    configPath = $ConfigPath
    serverUrl = $serverUrl
    apiBase = $apiBase
    awHostname = $awHostname
    installRoot = $installRoot
    stateRoot = $stateRoot
    timing = [ordered]@{
        pollSeconds = [int]$pollSeconds
        pulseSeconds = [int]$pulseSeconds
        freshnessSeconds = [int]$freshnessSeconds
        sessionFreshnessSeconds = [int]$sessionFreshnessSeconds
        transportStaleSeconds = [int]$transportStaleSeconds
    }
    files = [ordered]@{
        required = $requiredFiles
        missing = $missingFiles
        ok = ($missingFiles.Count -eq 0)
    }
    tasks = [ordered]@{
        list = $tasks
        ok = [bool]($tasks.Count -gt 0 -and -not ($tasks | Where-Object { -not $_.present -or -not $_.enabled }))
    }
    processes = [ordered]@{
        sessionBoundUsers = $sessionBoundUsers
        sessionScopedExpectedCount = [int]$sessionScopedExpectedCount
        watchers = @($runningWatchers)
        watcherDuplicates = @($watcherDuplicates)
        sessionCollectors = @($sessionCollectorProcesses)
        sessionCollectorDuplicates = @($sessionCollectorDuplicates)
        browserCollectors = @($browserCollectorProcesses)
        browserCollectorDuplicates = @($browserCollectorDuplicates)
        endpointCollectors = @($endpointCollectorProcesses)
        endpointCollectorDuplicates = @($endpointCollectorDuplicates)
        fileCollectors = @($fileCollectorProcesses)
        fileCollectorDuplicates = @($fileCollectorDuplicates)
        ok = [bool](
            $watcherCountsOk -and
            $sessionCollectorOk -and
            $browserProcessOk -and
            $endpointProcessOk -and
            $fileProcessOk -and
            ($watcherDuplicates.Count -eq 0) -and
            ($sessionCollectorDuplicates.Count -eq 0) -and
            ($browserCollectorDuplicates.Count -eq 0) -and
            ($endpointCollectorDuplicates.Count -eq 0) -and
            ($fileCollectorDuplicates.Count -eq 0)
        )
    }
    buckets = [ordered]@{
        list = @($bucketChecks)
        ok = [bool](-not ($bucketChecks | Where-Object { -not $_.ok }))
    }
    queues = [ordered]@{
        list = @($queueChecks)
        ok = [bool](-not ($queueChecks | Where-Object { -not $_.ok }))
    }
    printTelemetry = [ordered]@{
        operationalLogEnabled = $printServiceOperationalEnabled
        jobTitlePolicyEnabled = $printJobTitlePolicyEnabled
        ok = [bool]($printServiceOperationalEnabled -and $printJobTitlePolicyEnabled)
    }
    forensics = [ordered]@{
        evtxExportRoot = if ($config.PSObject.Properties.Name -contains 'forensics' -and $config.forensics.PSObject.Properties.Name -contains 'evtxExportRoot') { [string]$config.forensics.evtxExportRoot } else { $null }
        retentionDays = if ($config.PSObject.Properties.Name -contains 'forensics' -and $config.forensics.PSObject.Properties.Name -contains 'retentionDays') { [int]$config.forensics.retentionDays } else { $null }
        evtxChannels = if ($config.PSObject.Properties.Name -contains 'forensics' -and $config.forensics.PSObject.Properties.Name -contains 'evtxChannels') { @($config.forensics.evtxChannels) } else { @() }
        ok = [bool](
            ($config.PSObject.Properties.Name -contains 'forensics') -and
            ($config.forensics.PSObject.Properties.Name -contains 'evtxExportRoot') -and
            ($config.forensics.PSObject.Properties.Name -contains 'retentionDays') -and
            ($config.forensics.PSObject.Properties.Name -contains 'evtxChannels') -and
            (@($config.forensics.evtxChannels).Count -gt 0)
        )
    }
}

$result.summary = [ordered]@{
    failedSections = @(
        'files', 'tasks', 'processes', 'buckets', 'queues', 'printTelemetry', 'forensics' |
            Where-Object { -not [bool]$result.$_.ok }
    )
}

$result.overallOk = [bool](
    $result.files.ok -and
    $result.tasks.ok -and
    $result.processes.ok -and
    $result.buckets.ok -and
    $result.queues.ok -and
    $result.printTelemetry.ok -and
    $result.forensics.ok
)

$result
