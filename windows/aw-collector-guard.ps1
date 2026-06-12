[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\AWatch-rus\deployment-config.json',
    [ValidateSet('shadow', 'enforce')]
    [string]$Mode = 'shadow',
    [int]$LoopSeconds = 60,
    [int]$InteractiveMaxAgeSeconds = 900,
    [int]$HeadlessMaxAgeSeconds = 900,
    [int]$RestartWindowSeconds = 600,
    [int]$MaxRestarts = 3,
    [int]$ActionCooldownSeconds = 300,
    [int]$InteractiveActionCooldownSeconds = 60,
    [switch]$Once,
    [switch]$HeadlessEndpointEnabled,
    [switch]$HeadlessFileOpsEnabled,
    [switch]$SelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$modulePath = Join-Path $PSScriptRoot 'ActivityWatch.Windows.Common.psm1'
Import-Module $modulePath -Force

function New-GuardLock {
    param([string]$StateRoot)

    if (-not (Test-Path -LiteralPath $StateRoot)) {
        New-Item -Path $StateRoot -ItemType Directory -Force | Out-Null
    }

    $lockPath = Join-Path $StateRoot 'collector-guard.lock'
    if (Test-Path -LiteralPath $lockPath) {
        try {
            $lockData = Get-Content -LiteralPath $lockPath -Raw | ConvertFrom-Json
            $existingPid = [int]$lockData.pid
            if ($existingPid -gt 0 -and (Get-Process -Id $existingPid -ErrorAction SilentlyContinue)) {
                return $null
            }
        }
        catch {
        }
    }

    $payload = @{
        pid = $PID
        createdAt = (Get-Date).ToUniversalTime().ToString('o')
    } | ConvertTo-Json -Compress
    Set-Content -LiteralPath $lockPath -Value $payload -Encoding UTF8
    return $lockPath
}

function Write-GuardLog {
    param(
        [string]$LogPath,
        [string]$Message
    )

    try {
        $directory = Split-Path -Path $LogPath -Parent
        if ($directory -and -not (Test-Path -LiteralPath $directory)) {
            New-Item -Path $directory -ItemType Directory -Force | Out-Null
        }
        Add-Content -LiteralPath $LogPath -Value ('{0} {1}' -f (Get-Date -Format s), $Message)
    }
    catch {
    }
}

function Get-AwApiBase {
    param([pscustomobject]$Config)

    $scheme = if ($Config.server.PSObject.Properties.Name -contains 'scheme') { [string]$Config.server.scheme } else { 'http' }
    $hostName = [string]$Config.server.host
    $port = [int]$Config.server.port
    return ('{0}://{1}:{2}/api/0' -f $scheme, $hostName, $port)
}

function Invoke-AwJson {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Method,
        [Parameter(Mandatory = $true)]
        [string]$Uri,
        [object]$Body
    )

    $params = @{
        Method = $Method
        Uri = $Uri
        TimeoutSec = 15
        ErrorAction = 'Stop'
    }
    if ($null -ne $Body) {
        $params.Body = ($Body | ConvertTo-Json -Depth 16 -Compress)
        $params.ContentType = 'application/json'
    }
    return Invoke-RestMethod @params
}

function Ensure-AwBucket {
    param(
        [string]$ApiBase,
        [string]$BucketId,
        [string]$ClientName,
        [string]$BucketType,
        [string]$Hostname
    )

    try {
        Invoke-AwJson -Method 'GET' -Uri "$ApiBase/buckets/$BucketId" | Out-Null
        return $true
    }
    catch {
    }

    try {
        $body = @{
            client = $ClientName
            type = $BucketType
            hostname = $Hostname
        }
        Invoke-AwJson -Method 'POST' -Uri "$ApiBase/buckets/$BucketId" -Body $body | Out-Null
        return $true
    }
    catch {
        return $false
    }
}

function Get-LatestBucketAge {
    param(
        [string]$ApiBase,
        [string]$BucketId
    )

    try {
        $events = Invoke-AwJson -Method 'GET' -Uri "$ApiBase/buckets/$BucketId/events?limit=20"
        $latest = @($events | Where-Object { $null -ne $_.timestamp } | Sort-Object timestamp -Descending | Select-Object -First 1)
        if (-not $latest) {
            return [pscustomobject]@{ bucket = $BucketId; found = $false; timestamp = $null; ageSeconds = $null }
        }
        $ts = [DateTimeOffset]::Parse([string]$latest.timestamp).UtcDateTime
        $age = [Math]::Max(0, [int]((Get-Date).ToUniversalTime() - $ts).TotalSeconds)
        return [pscustomobject]@{ bucket = $BucketId; found = $true; timestamp = [string]$latest.timestamp; ageSeconds = $age }
    }
    catch {
        return [pscustomobject]@{ bucket = $BucketId; found = $false; timestamp = $null; ageSeconds = $null; error = $_.Exception.Message }
    }
}

function Send-GuardHeartbeat {
    param(
        [string]$ApiBase,
        [string]$Hostname,
        [object]$State,
        [int]$PulseSeconds
    )

    $bucketId = "aw-rus-collector-guard_$Hostname"
    if (-not (Ensure-AwBucket -ApiBase $ApiBase -BucketId $bucketId -ClientName 'aw-rus-collector-guard' -BucketType 'aw.rus.collector.guard' -Hostname $Hostname)) {
        return $false
    }

    $event = @{
        timestamp = (Get-Date).ToUniversalTime().ToString('o')
        duration = 0
        data = $State
    }

    try {
        Invoke-AwJson -Method 'POST' -Uri "$ApiBase/buckets/$bucketId/heartbeat?pulsetime=$PulseSeconds" -Body $event | Out-Null
        return $true
    }
    catch {
        return $false
    }
}

function Read-GuardRuntime {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        return [pscustomobject]@{ restartHistory = @{}; lastAction = @{}; quarantine = @{} }
    }
    try {
        $state = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
        if ($null -eq $state.restartHistory) { $state | Add-Member -NotePropertyName restartHistory -NotePropertyValue @{} }
        if ($null -eq $state.lastAction) { $state | Add-Member -NotePropertyName lastAction -NotePropertyValue @{} }
        if ($null -eq $state.quarantine) { $state | Add-Member -NotePropertyName quarantine -NotePropertyValue @{} }
        return $state
    }
    catch {
        return [pscustomobject]@{ restartHistory = @{}; lastAction = @{}; quarantine = @{} }
    }
}

function Write-GuardRuntime {
    param(
        [string]$Path,
        [object]$Runtime
    )

    $directory = Split-Path -Path $Path -Parent
    if ($directory -and -not (Test-Path -LiteralPath $directory)) {
        New-Item -Path $directory -ItemType Directory -Force | Out-Null
    }
    $Runtime | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath $Path -Encoding UTF8
}

function Get-RuntimeMapValue {
    param(
        [object]$Map,
        [string]$Key
    )

    if ($null -eq $Map) {
        return $null
    }
    if ($Map -is [hashtable] -and $Map.ContainsKey($Key)) {
        return $Map[$Key]
    }
    $propertyNames = @($Map.PSObject.Properties | ForEach-Object { $_.Name })
    if ($propertyNames -contains $Key) {
        return $Map.$Key
    }
    return $null
}

function Set-RuntimeMapValue {
    param(
        [object]$Map,
        [string]$Key,
        [object]$Value
    )

    if ($Map -is [hashtable]) {
        $Map[$Key] = $Value
        return
    }
    $propertyNames = @($Map.PSObject.Properties | ForEach-Object { $_.Name })
    if ($propertyNames -contains $Key) {
        $Map.$Key = $Value
    }
    else {
        $Map | Add-Member -NotePropertyName $Key -NotePropertyValue $Value -Force
    }
}

function Remove-RuntimeMapValue {
    param(
        [object]$Map,
        [string]$Key
    )

    if ($null -eq $Map) {
        return
    }
    if ($Map -is [hashtable]) {
        if ($Map.ContainsKey($Key)) {
            $Map.Remove($Key)
        }
        return
    }
    $property = $Map.PSObject.Properties[$Key]
    if ($null -ne $property) {
        $Map.PSObject.Properties.Remove($Key)
    }
}

function Reset-GuardActionBudget {
    param(
        [object]$Runtime,
        [string]$Key
    )

    Remove-RuntimeMapValue -Map $Runtime.restartHistory -Key $Key
    Remove-RuntimeMapValue -Map $Runtime.lastAction -Key $Key
    Remove-RuntimeMapValue -Map $Runtime.quarantine -Key $Key
}

function Invoke-GuardSelfTest {
    $emptyObject = [pscustomobject]@{}
    if ($null -ne (Get-RuntimeMapValue -Map $emptyObject -Key 'missing')) {
        throw 'empty PSCustomObject should not return a missing runtime-map value'
    }
    Set-RuntimeMapValue -Map $emptyObject -Key 'headless:worktime-session' -Value 123
    if ((Get-RuntimeMapValue -Map $emptyObject -Key 'headless:worktime-session') -ne 123) {
        throw 'failed to set runtime-map value on empty PSCustomObject'
    }

    $hash = @{}
    Set-RuntimeMapValue -Map $hash -Key 'headless:worktime-session' -Value @(1, 2)
    $hashValue = @(Get-RuntimeMapValue -Map $hash -Key 'headless:worktime-session')
    if ($hashValue.Count -ne 2) {
        throw 'failed to round-trip runtime-map value on hashtable'
    }

    $runtime = [pscustomobject]@{ restartHistory = [pscustomobject]@{}; lastAction = [pscustomobject]@{}; quarantine = [pscustomobject]@{} }
    $allowed = Test-ActionAllowed -Runtime $runtime -Key 'headless:worktime-session' -CooldownSeconds 1 -WindowSeconds 60 -MaxCount 3
    if (-not $allowed.allowed) {
        throw "expected action to be allowed, got $($allowed.reason)"
    }
    Register-GuardAction -Runtime $runtime -Key 'headless:worktime-session'
    $blocked = Test-ActionAllowed -Runtime $runtime -Key 'headless:worktime-session' -CooldownSeconds 300 -WindowSeconds 60 -MaxCount 3
    if ($blocked.allowed -or $blocked.reason -ne 'cooldown') {
        throw 'expected cooldown after registering guard action'
    }
    $budgetRuntime = [pscustomobject]@{ restartHistory = [pscustomobject]@{}; lastAction = [pscustomobject]@{}; quarantine = [pscustomobject]@{} }
    foreach ($i in 1..3) {
        Register-GuardAction -Runtime $budgetRuntime -Key 'task:test'
    }
    $budgetBlocked = Test-ActionAllowed -Runtime $budgetRuntime -Key 'task:test' -CooldownSeconds 0 -WindowSeconds 600 -MaxCount 3
    if ($budgetBlocked.allowed -or $budgetBlocked.reason -ne 'quarantine') {
        throw 'expected quarantine when restart budget is exhausted'
    }
    Reset-GuardActionBudget -Runtime $budgetRuntime -Key 'task:test'
    $budgetAllowed = Test-ActionAllowed -Runtime $budgetRuntime -Key 'task:test' -CooldownSeconds 0 -WindowSeconds 600 -MaxCount 3
    if (-not $budgetAllowed.allowed) {
        throw 'expected reset action budget to clear quarantine'
    }

    $oldComputerName = $env:COMPUTERNAME
    try {
        $env:COMPUTERNAME = 'SHARKON2025'
        $sessionRecords = @(
            [pscustomobject]@{ SessionName = 'USER5'; UserName = 'USER5'; SessionId = 2; State = 'Disc'; IsLive = $false },
            [pscustomobject]@{ SessionName = 'console'; UserName = ''; SessionId = 1; State = 'Conn'; IsLive = $true }
        )
        $taskDefs = @(
            [pscustomobject]@{ taskName = 'ActivityWatch Launch [SHARKON2025_user5]'; userId = 'SHARKON2025\user5' }
        )
        if (-not (Test-ActivityWatchUserHasManagedSession -UserId 'SHARKON2025\user5' -SessionRecords $sessionRecords -IncludeDisconnected)) {
            throw 'expected disconnected managed session to match task user'
        }
        if (Test-ActivityWatchUserHasManagedSession -UserId 'SHARKON2025\user5' -SessionRecords $sessionRecords -IncludeLive) {
            throw 'disconnected managed session should not match live-only filter'
        }
        $managed = @(Get-ActivityWatchManagedInteractiveSessions -TaskDefinitions $taskDefs -SessionRecords $sessionRecords -IncludeDisconnected)
        if ($managed.Count -ne 1 -or [int]$managed[0].SessionId -ne 2) {
            throw 'failed to enumerate disconnected managed session'
        }
    }
    finally {
        if ($null -eq $oldComputerName) {
            Remove-Item Env:COMPUTERNAME -ErrorAction SilentlyContinue
        }
        else {
            $env:COMPUTERNAME = $oldComputerName
        }
    }

    Write-Output 'collector guard self-test OK'
}

function Test-ActionAllowed {
    param(
        [object]$Runtime,
        [string]$Key,
        [int]$CooldownSeconds,
        [int]$WindowSeconds,
        [int]$MaxCount
    )

    $now = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    $last = Get-RuntimeMapValue -Map $Runtime.lastAction -Key $Key
    if ($null -ne $last -and ($now - [int64]$last) -lt $CooldownSeconds) {
        return [pscustomobject]@{ allowed = $false; reason = 'cooldown' }
    }

    $history = @(Get-RuntimeMapValue -Map $Runtime.restartHistory -Key $Key)
    $history = @($history | Where-Object { ($now - [int64]$_) -le $WindowSeconds })
    Set-RuntimeMapValue -Map $Runtime.restartHistory -Key $Key -Value @($history)
    if ($history.Count -ge $MaxCount) {
        Set-RuntimeMapValue -Map $Runtime.quarantine -Key $Key -Value @{
            since = (Get-Date).ToUniversalTime().ToString('o')
            reason = 'restart-budget-exhausted'
            count = $history.Count
        }
        return [pscustomobject]@{ allowed = $false; reason = 'quarantine' }
    }

    Remove-RuntimeMapValue -Map $Runtime.quarantine -Key $Key
    return [pscustomobject]@{ allowed = $true; reason = 'ok' }
}

function Register-GuardAction {
    param(
        [object]$Runtime,
        [string]$Key
    )

    $now = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    $history = @(Get-RuntimeMapValue -Map $Runtime.restartHistory -Key $Key)
    $history += $now
    Set-RuntimeMapValue -Map $Runtime.restartHistory -Key $Key -Value @($history)
    Set-RuntimeMapValue -Map $Runtime.lastAction -Key $Key -Value $now
}

function Get-CollectorProcessSnapshot {
    param([pscustomobject]$Config)

    $scriptPaths = [ordered]@{}
    foreach ($name in @('collectorScript', 'endpointCollectorScript', 'fileCollectorScript', 'emailCollectorScript', 'sessionCollectorScript')) {
        if ($Config.paths.PSObject.Properties.Name -contains $name) {
            $value = [string]$Config.paths.$name
            if (-not [string]::IsNullOrWhiteSpace($value)) {
                $scriptPaths[$name] = $value
            }
        }
    }

    $powershellCollectors = @()
    try {
        $processes = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -ieq 'powershell.exe' -or $_.Name -ieq 'pwsh.exe' })
        foreach ($proc in $processes) {
            $commandLine = [string]$proc.CommandLine
            foreach ($entry in $scriptPaths.GetEnumerator()) {
                if ($commandLine -match [Regex]::Escape([string]$entry.Value)) {
                    $powershellCollectors += [pscustomobject]@{
                        name = [string]$entry.Key
                        processId = [int]$proc.ProcessId
                        sessionId = [int]$proc.SessionId
                        scriptPath = [string]$entry.Value
                    }
                }
            }
        }
    }
    catch {
    }

    $watchers = @()
    try {
        $watchers = @(Get-Process -Name 'aw-watcher-afk','aw-watcher-window' -ErrorAction SilentlyContinue |
            Select-Object @{Name='name'; Expression={$_.Name}}, @{Name='processId'; Expression={$_.Id}}, @{Name='sessionId'; Expression={$_.SessionId}})
    }
    catch {
        $watchers = @()
    }

    return [pscustomobject]@{
        watchers = @($watchers)
        collectors = @($powershellCollectors)
    }
}

function Invoke-ExactTaskRun {
    param([string]$TaskName)

    & schtasks.exe /Run /TN $TaskName | Out-Null
    return ($LASTEXITCODE -eq 0)
}

function Invoke-GuardCycle {
    param(
        [object]$Runtime,
        [string]$RuntimePath,
        [string]$LogPath
    )

    $config = Read-ActivityWatchDeploymentConfig -Path $ConfigPath
    $stateRoot = if ($config.paths.PSObject.Properties.Name -contains 'stateRoot') { [string]$config.paths.stateRoot } else { Split-Path -Path $ConfigPath -Parent }
    $hostname = if ($config.PSObject.Properties.Name -contains 'awHostname' -and -not [string]::IsNullOrWhiteSpace([string]$config.awHostname)) { [string]$config.awHostname } else { [string]$env:COMPUTERNAME }
    $apiBase = Get-AwApiBase -Config $config
    $configPaths = Get-ActivityWatchRecoveryConfigPaths -PrimaryConfigPath $ConfigPath
    $taskDefs = @(Get-ActivityWatchRecoveryTaskDefinitions -ConfigPaths $configPaths)
    $sessionRecords = @(Get-ActivityWatchSessionRecords)
    $liveSessions = @(Get-ActivityWatchLiveInteractiveSessions -SessionRecords $sessionRecords)
    $managedInteractiveSessions = @(Get-ActivityWatchManagedInteractiveSessions -TaskDefinitions $taskDefs -SessionRecords $sessionRecords -IncludeLive -IncludeDisconnected)
    $processSnapshot = Get-CollectorProcessSnapshot -Config $config
    $liveSessionIds = @($liveSessions | ForEach-Object { [int]$_.SessionId })
    $managedSessionIds = @($managedInteractiveSessions | ForEach-Object { [int]$_.SessionId } | Sort-Object -Unique)

    $bucketChecks = [ordered]@{}
    foreach ($bucket in @(
            "aw-worktime-sessions_$hostname",
            "aw-watcher-afk_$hostname",
            "aw-watcher-window_$hostname",
            "aw-dlp-endpoint-signals_$hostname"
        )) {
        $bucketChecks[$bucket] = Get-LatestBucketAge -ApiBase $apiBase -BucketId $bucket
    }

    $actions = New-Object System.Collections.Generic.List[object]
    $problems = New-Object System.Collections.Generic.List[string]

    $worktimeAge = $bucketChecks["aw-worktime-sessions_$hostname"].ageSeconds
    $sessionCollectorScript = if ($config.paths.PSObject.Properties.Name -contains 'sessionCollectorScript') { [string]$config.paths.sessionCollectorScript } else { Join-Path $stateRoot 'worktime-session-collector.ps1' }
    $worktimeSessionEnabled = if ($config.PSObject.Properties.Name -contains 'collectors' -and $config.collectors.PSObject.Properties.Name -contains 'worktimeSessionEnabled') { [bool]$config.collectors.worktimeSessionEnabled } else { $true }
    $worktimeSessionMode = if ($config.PSObject.Properties.Name -contains 'collectors' -and $config.collectors.PSObject.Properties.Name -contains 'worktimeSessionMode') { [string]$config.collectors.worktimeSessionMode } else { 'powershell_primary' }
    $worktimeLegacyFallbackEnabled = if ($config.PSObject.Properties.Name -contains 'collectors' -and $config.collectors.PSObject.Properties.Name -contains 'worktimeLegacyFallbackEnabled') { [bool]$config.collectors.worktimeLegacyFallbackEnabled } else { $true }
    $rustAgentRunning = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object { $_.Name -ieq 'awatch-agent-rs.exe' }).Count -gt 0
    $rustPrimary = $worktimeSessionMode -ieq 'rust_primary'
    $rustWorktimeStale = ($null -eq $worktimeAge -or [int]$worktimeAge -gt $HeadlessMaxAgeSeconds)
    $allowPowerShellWorktime = $worktimeSessionEnabled -and (-not $rustPrimary -or ($worktimeLegacyFallbackEnabled -and (-not $rustAgentRunning -or $rustWorktimeStale)))
    $sessionCollectorRunning = $allowPowerShellWorktime -and (Test-ActivityWatchCollectorRunningGlobal -ScriptPath $sessionCollectorScript)
    $headlessKey = 'headless:worktime-session'
    $needsHeadlessAction = $allowPowerShellWorktime -and (-not $sessionCollectorRunning -or $rustWorktimeStale)
    if ($needsHeadlessAction) {
        $key = $headlessKey
        $allowed = Test-ActionAllowed -Runtime $Runtime -Key $key -CooldownSeconds $ActionCooldownSeconds -WindowSeconds $RestartWindowSeconds -MaxCount $MaxRestarts
        if ($allowed.allowed) {
            if ($Mode -eq 'enforce') {
                Start-ActivityWatchCollectorScriptGlobalIfNeeded -ScriptPath $sessionCollectorScript -ConfigPath $ConfigPath
                Register-GuardAction -Runtime $Runtime -Key $key
                Write-GuardLog -LogPath $LogPath -Message "started $key"
                $actions.Add([pscustomobject]@{ action = 'start'; target = $key; applied = $true }) | Out-Null
            }
            else {
                $actions.Add([pscustomobject]@{ action = 'start'; target = $key; applied = $false; mode = 'shadow' }) | Out-Null
            }
        }
        else {
            $problems.Add("$key action blocked: $($allowed.reason)") | Out-Null
        }
    }
    else {
        Reset-GuardActionBudget -Runtime $Runtime -Key $headlessKey
    }

    if ($Mode -eq 'enforce') {
        Stop-ActivityWatchProcessesInNonLiveSessions -SessionRecords $sessionRecords -Config $config -TaskDefinitions $taskDefs -PreserveManagedSessions
    }

    $interactiveStale = $false
    foreach ($bucket in @("aw-watcher-afk_$hostname", "aw-watcher-window_$hostname", "aw-dlp-endpoint-signals_$hostname")) {
        $age = $bucketChecks[$bucket].ageSeconds
        if ($null -eq $age -or [int]$age -gt $InteractiveMaxAgeSeconds) {
            $interactiveStale = $true
        }
    }

    $watchersInLiveSessions = @($processSnapshot.watchers | Where-Object { $liveSessionIds -contains [int]$_.sessionId })
    $watchersInManagedSessions = @($processSnapshot.watchers | Where-Object { $managedSessionIds -contains [int]$_.sessionId })
    $liveWatcherMissing = $false
    if ($liveSessions.Count -gt 0) {
        $hasAfk = @($watchersInLiveSessions | Where-Object { [string]$_.name -ieq 'aw-watcher-afk' }).Count -gt 0
        $hasWindow = @($watchersInLiveSessions | Where-Object { [string]$_.name -ieq 'aw-watcher-window' }).Count -gt 0
        $liveWatcherMissing = (-not $hasAfk) -or (-not $hasWindow)
    }
    $managedWatcherMissing = $false
    if ($managedInteractiveSessions.Count -gt 0) {
        $afkEnabled = if ($config.PSObject.Properties.Name -contains 'collectors' -and $config.collectors.PSObject.Properties.Name -contains 'afkEnabled') { [bool]$config.collectors.afkEnabled } else { $true }
        $windowEnabled = if ($config.PSObject.Properties.Name -contains 'collectors' -and $config.collectors.PSObject.Properties.Name -contains 'windowEnabled') { [bool]$config.collectors.windowEnabled } else { $true }
        foreach ($managedSession in @($managedInteractiveSessions)) {
            $sessionId = [int]$managedSession.SessionId
            $sessionWatchers = @($watchersInManagedSessions | Where-Object { [int]$_.sessionId -eq $sessionId })
            $hasManagedAfk = @($sessionWatchers | Where-Object { [string]$_.name -ieq 'aw-watcher-afk' }).Count -gt 0
            $hasManagedWindow = @($sessionWatchers | Where-Object { [string]$_.name -ieq 'aw-watcher-window' }).Count -gt 0
            if (($afkEnabled -and -not $hasManagedAfk) -or ($windowEnabled -and -not $hasManagedWindow)) {
                $managedWatcherMissing = $true
                break
            }
        }
    }

    if ($managedInteractiveSessions.Count -eq 0 -and $liveSessions.Count -gt 0 -and $interactiveStale) {
        $problems.Add('interactive buckets stale but no managed interactive sessions found') | Out-Null
    }

    $needsInteractiveTaskAction = $managedInteractiveSessions.Count -gt 0 -and (
        $liveWatcherMissing -or
        $managedWatcherMissing -or
        ($interactiveStale -and $liveSessions.Count -gt 0)
    )

    if ($needsInteractiveTaskAction) {
        foreach ($taskDef in $taskDefs) {
            if (-not (Test-ActivityWatchUserHasManagedSession -UserId ([string]$taskDef.userId) -SessionRecords $sessionRecords -IncludeLive -IncludeDisconnected)) {
                continue
            }
            $key = "task:$($taskDef.taskName)"
            $allowed = Test-ActionAllowed -Runtime $Runtime -Key $key -CooldownSeconds $InteractiveActionCooldownSeconds -WindowSeconds $RestartWindowSeconds -MaxCount $MaxRestarts
            if (-not $allowed.allowed) {
                $problems.Add("$key action blocked: $($allowed.reason)") | Out-Null
                continue
            }

            if ($Mode -eq 'enforce') {
                $ok = Invoke-ExactTaskRun -TaskName ([string]$taskDef.taskName)
                if ($ok) {
                    Register-GuardAction -Runtime $Runtime -Key $key
                }
                Write-GuardLog -LogPath $LogPath -Message ("run {0} ok={1}" -f $key, $ok)
                $actions.Add([pscustomobject]@{ action = 'run-task'; target = [string]$taskDef.taskName; applied = $true; ok = $ok }) | Out-Null
            }
            else {
                $actions.Add([pscustomobject]@{ action = 'run-task'; target = [string]$taskDef.taskName; applied = $false; mode = 'shadow' }) | Out-Null
            }
        }
    }
    else {
        foreach ($taskDef in $taskDefs) {
            if (Test-ActivityWatchUserHasManagedSession -UserId ([string]$taskDef.userId) -SessionRecords $sessionRecords -IncludeLive -IncludeDisconnected) {
                Reset-GuardActionBudget -Runtime $Runtime -Key "task:$($taskDef.taskName)"
            }
        }
    }

    $status = 'ok'
    if ($problems.Count -gt 0) {
        $status = 'warn'
    }
    if ($managedInteractiveSessions.Count -gt 0 -and $interactiveStale -and $Mode -eq 'shadow') {
        $status = 'warn'
    }

    $sessionState = @(
        foreach ($session in @($sessionRecords)) {
            [pscustomobject]@{
                SessionName = [string]$session.SessionName
                UserName = [string]$session.UserName
                SessionId = [int]$session.SessionId
                State = [string]$session.State
                IsLive = [bool]$session.IsLive
            }
        }
    )

    $state = @{}
    $state['status'] = $status
    $state['mode'] = $Mode
    $state['host'] = $hostname
    $state['generatedAtUtc'] = (Get-Date).ToUniversalTime().ToString('o')
    $state['pid'] = $PID
    $state['sessions'] = @($sessionState)
    $state['liveSessionCount'] = $liveSessions.Count
    $state['managedSessionCount'] = $managedInteractiveSessions.Count
    $state['managedSessions'] = @($managedInteractiveSessions)
    $bucketState = @{}
    foreach ($key in $bucketChecks.Keys) {
        $bucketState[$key] = $bucketChecks[$key]
    }

    $state['processes'] = $processSnapshot
    $state['buckets'] = $bucketState
    $state['actions'] = @($actions.ToArray())
    $state['problems'] = @($problems.ToArray())
    $state['quarantine'] = $Runtime.quarantine

    $statePath = Join-Path $stateRoot 'collector-guard-state.json'
    $state | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath $statePath -Encoding UTF8
    Write-GuardRuntime -Path $RuntimePath -Runtime $Runtime
    [void](Send-GuardHeartbeat -ApiBase $apiBase -Hostname $hostname -State $state -PulseSeconds ([Math]::Max($LoopSeconds * 2, 60)))
    return $state
}

if ($SelfTest) {
    Invoke-GuardSelfTest
    exit 0
}

$initialConfig = Read-ActivityWatchDeploymentConfig -Path $ConfigPath
$initialStateRoot = if ($initialConfig.paths.PSObject.Properties.Name -contains 'stateRoot') { [string]$initialConfig.paths.stateRoot } else { Split-Path -Path $ConfigPath -Parent }
$initialLogsRoot = if ($initialConfig.paths.PSObject.Properties.Name -contains 'logsRoot') { [string]$initialConfig.paths.logsRoot } else { Join-Path $initialStateRoot 'logs' }
$logPath = Join-Path $initialLogsRoot 'collector-guard.log'
$runtimePath = Join-Path $initialStateRoot 'collector-guard-runtime.json'
$lockPath = New-GuardLock -StateRoot $initialStateRoot
if (-not $lockPath) {
    Write-GuardLog -LogPath $logPath -Message 'another collector guard instance is already running'
    exit 0
}

try {
    $runtime = Read-GuardRuntime -Path $runtimePath
    Write-GuardLog -LogPath $logPath -Message "collector guard started mode=$Mode loop=$LoopSeconds once=$($Once.IsPresent)"
    while ($true) {
        try {
            Invoke-GuardCycle -Runtime $runtime -RuntimePath $runtimePath -LogPath $logPath | Out-Null
        }
        catch {
            Write-GuardLog -LogPath $logPath -Message ("cycle error: {0}; at {1}" -f $_.Exception.Message, $_.ScriptStackTrace)
        }

        if ($Once) {
            break
        }
        Start-Sleep -Seconds ([Math]::Max($LoopSeconds, 15))
    }
}
finally {
    if ($lockPath -and (Test-Path -LiteralPath $lockPath)) {
        Remove-Item -LiteralPath $lockPath -Force -ErrorAction SilentlyContinue
    }
    Write-GuardLog -LogPath $logPath -Message 'collector guard stopped'
}
