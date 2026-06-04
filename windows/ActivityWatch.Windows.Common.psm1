Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$script:ActivityWatchBuiltInAdministratorName = $null

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
    Get-ChildItem -LiteralPath $WorkingRoot -File -Filter 'activitywatch-*.zip' -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -Skip 2 |
        ForEach-Object {
            try { Remove-Item -LiteralPath $_.FullName -Force -ErrorAction SilentlyContinue } catch {}
        }
    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    $suffix = ([guid]::NewGuid().Guid.Substring(0, 8))
    $archivePath = Join-Path $WorkingRoot ("activitywatch-{0}-{1}-{2}.zip" -f $Version.TrimStart('v'), $stamp, $suffix)
    Invoke-WebRequest -Uri $PackageUrl -OutFile $archivePath
    return $archivePath
}

function Remove-ActivityWatchOldInstallBackups {
    param(
        [Parameter(Mandatory = $true)]
        [string]$BackupRoot,
        [int]$Keep = 2
    )

    if (-not (Test-Path -LiteralPath $BackupRoot)) {
        return
    }

    Get-ChildItem -LiteralPath $BackupRoot -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -like 'install-*' } |
        Sort-Object LastWriteTime -Descending |
        Select-Object -Skip $Keep |
        ForEach-Object {
            try { Remove-Item -LiteralPath $_.FullName -Recurse -Force -ErrorAction SilentlyContinue } catch {}
        }
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
    Remove-ActivityWatchOldInstallBackups -BackupRoot $BackupRoot

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
    $workDriveName = [System.IO.Path]::GetPathRoot($WorkingRoot).TrimEnd('\').TrimEnd(':')
    $workDrive = Get-PSDrive -Name $workDriveName -ErrorAction SilentlyContinue
    $freeBytes = $null
    if ($workDrive -and $null -ne $workDrive.Free) {
        $freeBytes = [int64]$workDrive.Free
    }
    elseif ($workDriveName) {
        try {
            $disk = Get-CimInstance Win32_LogicalDisk -Filter ("DeviceID='{0}:'" -f $workDriveName) -ErrorAction Stop
            if ($disk -and $null -ne $disk.FreeSpace) {
                $freeBytes = [int64]$disk.FreeSpace
            }
        }
        catch {
        }
    }
    if ($null -ne $freeBytes) {
        # Require at least ~2.5x archive size to handle extraction + copy safely.
        $required = [int64]([Math]::Ceiling($archiveSize * 2.5))
        if ($freeBytes -lt $required) {
            throw ("Недостаточно свободного места на {0}: free={1} bytes, required>={2} bytes" -f $workDriveName, $freeBytes, $required)
        }
    }

    try {
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
    finally {
        if (Test-Path -LiteralPath $extractRoot) {
            try { Remove-Item -LiteralPath $extractRoot -Recurse -Force -ErrorAction SilentlyContinue } catch {}
        }
        if ((Test-Path -LiteralPath $ArchivePath) -and ($ArchivePath -like (Join-Path $WorkingRoot 'activitywatch-*.zip'))) {
            try { Remove-Item -LiteralPath $ArchivePath -Force -ErrorAction SilentlyContinue } catch {}
        }
        Remove-ActivityWatchOldInstallBackups -BackupRoot $BackupRoot
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

function Repair-ActivityWatchPotentialMojibake {
    param([string]$Value)

    if ([string]::IsNullOrWhiteSpace($Value)) {
        return $Value
    }

    if ($Value -notmatch '[\u0400-\u04FF]') {
        return $Value
    }

    try {
        $bytes = [Text.Encoding]::GetEncoding(1251).GetBytes($Value)
        $repaired = [Text.Encoding]::UTF8.GetString($bytes)
        if (-not [string]::IsNullOrWhiteSpace($repaired) -and $repaired -match '[\u0400-\u04FF]') {
            return $repaired
        }
    }
    catch {
    }

    return $Value
}

function Get-ActivityWatchBuiltInAdministratorName {
    if ($script:ActivityWatchBuiltInAdministratorName) {
        return $script:ActivityWatchBuiltInAdministratorName
    }

    if (-not [string]::IsNullOrWhiteSpace($env:AWATCH_RUS_BUILTIN_ADMINISTRATOR_NAME)) {
        $script:ActivityWatchBuiltInAdministratorName = [string]$env:AWATCH_RUS_BUILTIN_ADMINISTRATOR_NAME
        return $script:ActivityWatchBuiltInAdministratorName
    }

    try {
        $account = Get-CimInstance Win32_UserAccount -Filter "LocalAccount=True" -ErrorAction Stop |
            Where-Object { [string]$_.SID -match '-500$' } |
            Select-Object -First 1
        if ($account -and -not [string]::IsNullOrWhiteSpace([string]$account.Name)) {
            $script:ActivityWatchBuiltInAdministratorName = [string]$account.Name
            return $script:ActivityWatchBuiltInAdministratorName
        }
    }
    catch {
    }

    if ([string]$env:COMPUTERNAME -ieq 'HOST-EXAMPLE') {
        $script:ActivityWatchBuiltInAdministratorName = 'Администратор'
        return $script:ActivityWatchBuiltInAdministratorName
    }

    $script:ActivityWatchBuiltInAdministratorName = 'Administrator'
    return $script:ActivityWatchBuiltInAdministratorName
}

function Normalize-ActivityWatchUserId {
    param(
        [string]$UserId,
        [string]$Domain
    )

    if ([string]::IsNullOrWhiteSpace($UserId)) {
        return $null
    }

    $normalized = Repair-ActivityWatchPotentialMojibake -Value $UserId.Trim()
    $resolvedDomain = $null
    $leafUser = $normalized

    if ($normalized -match '^([^\\]+)\\(.+)$') {
        $resolvedDomain = Repair-ActivityWatchPotentialMojibake -Value $Matches[1]
        $leafUser = Repair-ActivityWatchPotentialMojibake -Value $Matches[2]
    }

    if ($leafUser -match '^(?i:administrator|администратор)$') {
        $leafUser = Get-ActivityWatchBuiltInAdministratorName
    }

    if ([string]::IsNullOrWhiteSpace($resolvedDomain) -and -not [string]::IsNullOrWhiteSpace($Domain)) {
        $resolvedDomain = Repair-ActivityWatchPotentialMojibake -Value $Domain.Trim()
    }

    if (-not [string]::IsNullOrWhiteSpace($resolvedDomain)) {
        return ('{0}\{1}' -f $resolvedDomain, $leafUser)
    }

    return $leafUser
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
            $rows = Import-Csv -LiteralPath $resolved.Path -Encoding UTF8
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
            Get-Content -LiteralPath $resolved.Path -Encoding UTF8 | ForEach-Object {
                $line = $_.Trim()
                if ($line -and -not $line.StartsWith('#')) {
                    $collected.Add($line)
                }
            }
        }
    }

    $normalized = @($collected |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
        ForEach-Object { Normalize-ActivityWatchUserId -UserId $_ -Domain $Domain } |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
        Sort-Object -Unique)

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

function Test-ActivityWatchScheduledTaskExistsExact {
    param([string]$TaskName)

    if ([string]::IsNullOrWhiteSpace($TaskName)) {
        return $false
    }

    try {
        & schtasks.exe /Query /TN $TaskName *> $null
        return ($LASTEXITCODE -eq 0)
    }
    catch {
        return $false
    }
}

function New-ActivityWatchUserTaskDefinitions {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Users
    )

    $result = foreach ($user in $Users) {
        $normalizedUser = Normalize-ActivityWatchUserId -UserId $user
        $token = Get-ActivityWatchTaskNameToken -UserId $normalizedUser
        [pscustomobject]@{
            UserId         = $normalizedUser
            LaunchTaskName = "ActivityWatch Launch [$token]"
        }
    }

    return @($result)
}

function Get-ActivityWatchLoggedOnUsers {
    $users = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)

    try {
        $lines = & quser.exe 2>$null
        foreach ($line in @($lines)) {
            $normalized = [string]$line
            if ([string]::IsNullOrWhiteSpace($normalized)) {
                continue
            }

            $normalized = $normalized.TrimStart(' ', '>')
            if ([string]::IsNullOrWhiteSpace($normalized)) {
                continue
            }

            if ($normalized -match '^(USERNAME|ПОЛЬЗОВАТЕЛЬ)\s+') {
                continue
            }

            $parts = $normalized -split '\s+'
            if ($parts.Count -lt 1) {
                continue
            }

            $user = [string]$parts[0]
            if ([string]::IsNullOrWhiteSpace($user)) {
                continue
            }

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

function Get-ActivityWatchSessionRecords {
    $sessions = New-Object System.Collections.Generic.List[object]

    try {
        $lines = & qwinsta.exe 2>$null
        foreach ($line in @($lines)) {
            $normalized = [string]$line
            if ([string]::IsNullOrWhiteSpace($normalized)) {
                continue
            }

            $normalized = $normalized.TrimStart(' ', '>')
            if ([string]::IsNullOrWhiteSpace($normalized)) {
                continue
            }

            if ($normalized -match '^(SESSIONNAME|ИМЯ СЕАНСА)\s+') {
                continue
            }

            $columns = @(
                (($normalized -replace '\s{2,}', '|') -split '\|') |
                    ForEach-Object { $_.Trim() } |
                    Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
            )
            if ($columns.Count -lt 3) {
                continue
            }

            $sessionName = [string]$columns[0]
            $userName = $null
            $sessionIdIndex = 1

            if ($columns[1] -notmatch '^\d+$') {
                $userName = [string]$columns[1]
                $sessionIdIndex = 2
            }

            if ($columns.Count -le $sessionIdIndex -or $columns[$sessionIdIndex] -notmatch '^\d+$') {
                continue
            }

            $sessionId = [int]$columns[$sessionIdIndex]
            $state = if ($columns.Count -gt ($sessionIdIndex + 1)) { [string]$columns[$sessionIdIndex + 1] } else { '' }
            $isLive = $state -match '^(Active|Conn|Активно|Подкл\w*)$'

            $sessions.Add([pscustomobject]@{
                    SessionName = $sessionName
                    UserName    = $userName
                    SessionId   = $sessionId
                    State       = $state
                    IsLive      = $isLive
                }) | Out-Null
        }
    }
    catch {
    }

    $explorerUsers = Get-ActivityWatchExplorerUsersBySession
    foreach ($session in @($sessions.ToArray())) {
        $sessionId = [int]$session.SessionId
        if ($explorerUsers.ContainsKey($sessionId)) {
            $session.UserName = [string]$explorerUsers[$sessionId]
        }
    }

    return @($sessions.ToArray())
}

function Resolve-ActivityWatchUserCandidates {
    param(
        [Parameter(Mandatory = $true)]
        [string]$UserId
    )

    $normalizedUserId = Normalize-ActivityWatchUserId -UserId $UserId
    $candidateIds = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    [void]$candidateIds.Add($normalizedUserId)

    $leafUser = $normalizedUserId
    if ($leafUser -match '^[^\\]+\\(.+)$') {
        $leafUser = $Matches[1]
        [void]$candidateIds.Add($leafUser)
    }

    [void]$candidateIds.Add(('{0}\{1}' -f $env:COMPUTERNAME, $leafUser))
    if (-not [string]::IsNullOrWhiteSpace($env:USERDOMAIN)) {
        [void]$candidateIds.Add(('{0}\{1}' -f $env:USERDOMAIN, $leafUser))
    }

    return @($candidateIds)
}

function Test-ActivityWatchUserHasSession {
    param(
        [Parameter(Mandatory = $true)]
        [string]$UserId,
        [string[]]$LoggedOnUsers
    )

    if ([string]::IsNullOrWhiteSpace($UserId)) {
        return $false
    }

    foreach ($candidate in @(Resolve-ActivityWatchUserCandidates -UserId $UserId)) {
        if ($LoggedOnUsers -contains $candidate) {
            return $true
        }
    }

    return $false
}

function Test-ActivityWatchUserHasLiveSession {
    param(
        [Parameter(Mandatory = $true)]
        [string]$UserId,
        [object[]]$SessionRecords
    )

    if ([string]::IsNullOrWhiteSpace($UserId)) {
        return $false
    }

    foreach ($candidate in @(Resolve-ActivityWatchUserCandidates -UserId $UserId)) {
        if (@($SessionRecords | Where-Object {
                    $_.IsLive -and
                    -not [string]::IsNullOrWhiteSpace([string]$_.UserName) -and
                    (
                        [string]$_.UserName -ieq $candidate -or
                        ('{0}\{1}' -f $env:COMPUTERNAME, [string]$_.UserName) -ieq $candidate -or
                        ((-not [string]::IsNullOrWhiteSpace($env:USERDOMAIN)) -and ('{0}\{1}' -f $env:USERDOMAIN, [string]$_.UserName) -ieq $candidate)
                    )
                }).Count -gt 0) {
            return $true
        }
    }

    return $false
}

function Test-ActivityWatchSessionMatchesUserId {
    param(
        [Parameter(Mandatory = $true)]
        [object]$SessionRecord,
        [Parameter(Mandatory = $true)]
        [string]$UserId
    )

    if ([string]::IsNullOrWhiteSpace($UserId) -or $null -eq $SessionRecord) {
        return $false
    }

    $sessionUser = [string]$SessionRecord.UserName
    if ([string]::IsNullOrWhiteSpace($sessionUser)) {
        return $false
    }

    foreach ($candidate in @(Resolve-ActivityWatchUserCandidates -UserId $UserId)) {
        if ($sessionUser -ieq $candidate -or
            ('{0}\{1}' -f $env:COMPUTERNAME, $sessionUser) -ieq $candidate -or
            ((-not [string]::IsNullOrWhiteSpace($env:USERDOMAIN)) -and ('{0}\{1}' -f $env:USERDOMAIN, $sessionUser) -ieq $candidate)) {
            return $true
        }
    }

    return $false
}

function Test-ActivityWatchUserHasManagedSession {
    param(
        [Parameter(Mandatory = $true)]
        [string]$UserId,
        [object[]]$SessionRecords,
        [switch]$IncludeLive,
        [switch]$IncludeDisconnected
    )

    foreach ($session in @($SessionRecords)) {
        if ([int]$session.SessionId -le 0) {
            continue
        }
        if ([string]::IsNullOrWhiteSpace([string]$session.UserName)) {
            continue
        }
        if ([bool]$session.IsLive -and -not $IncludeLive.IsPresent) {
            continue
        }
        if (-not [bool]$session.IsLive -and -not $IncludeDisconnected.IsPresent) {
            continue
        }
        if (Test-ActivityWatchSessionMatchesUserId -SessionRecord $session -UserId $UserId) {
            return $true
        }
    }

    return $false
}

function Get-ActivityWatchManagedInteractiveSessions {
    param(
        [pscustomobject[]]$TaskDefinitions,
        [object[]]$SessionRecords,
        [switch]$IncludeLive,
        [switch]$IncludeDisconnected
    )

    $result = New-Object System.Collections.Generic.List[object]
    $seen = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)

    foreach ($taskDef in @($TaskDefinitions)) {
        $userId = [string]$taskDef.userId
        $taskName = [string]$taskDef.taskName
        if ([string]::IsNullOrWhiteSpace($userId) -or [string]::IsNullOrWhiteSpace($taskName)) {
            continue
        }

        foreach ($session in @($SessionRecords)) {
            if ([int]$session.SessionId -le 0) {
                continue
            }
            if ([string]::IsNullOrWhiteSpace([string]$session.UserName)) {
                continue
            }
            if ([bool]$session.IsLive -and -not $IncludeLive.IsPresent) {
                continue
            }
            if (-not [bool]$session.IsLive -and -not $IncludeDisconnected.IsPresent) {
                continue
            }
            if (-not (Test-ActivityWatchSessionMatchesUserId -SessionRecord $session -UserId $userId)) {
                continue
            }

            $key = '{0}|{1}|{2}' -f $taskName, [int]$session.SessionId, $userId
            if (-not $seen.Add($key)) {
                continue
            }

            $result.Add([pscustomobject]@{
                    TaskName    = $taskName
                    UserId      = $userId
                    SessionName = [string]$session.SessionName
                    SessionId   = [int]$session.SessionId
                    State       = [string]$session.State
                    UserName    = [string]$session.UserName
                    IsLive      = [bool]$session.IsLive
                }) | Out-Null
        }
    }

    return @($result.ToArray())
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
        [string]$EvtxExportScriptSource,
        [string]$HayabusaUploadScriptSource,
        [string]$File1CTelemetryScriptSource,
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
    $evtxExportTarget = Join-Path $StateRoot 'export-evtx-for-hayabusa.ps1'
    $hayabusaUploadTarget = Join-Path $StateRoot 'export-upload-hayabusa-to-aw-server.ps1'
    $file1cTelemetryTarget = Join-Path $StateRoot 'export-upload-file-1c-telemetry.ps1'
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
    if ($EvtxExportScriptSource -and (Test-Path -LiteralPath $EvtxExportScriptSource)) {
        Copy-Item -LiteralPath $EvtxExportScriptSource -Destination $evtxExportTarget -Force
    }
    if ($HayabusaUploadScriptSource -and (Test-Path -LiteralPath $HayabusaUploadScriptSource)) {
        Copy-Item -LiteralPath $HayabusaUploadScriptSource -Destination $hayabusaUploadTarget -Force
    }
    if ($File1CTelemetryScriptSource -and (Test-Path -LiteralPath $File1CTelemetryScriptSource)) {
        Copy-Item -LiteralPath $File1CTelemetryScriptSource -Destination $file1cTelemetryTarget -Force
    }
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
        EvtxExportScript        = $evtxExportTarget
        HayabusaUploadScript    = $hayabusaUploadTarget
        File1CTelemetryScript   = $file1cTelemetryTarget
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
        [string]$EvtxExportScript,
        [string]$HayabusaUploadScript,
        [string]$File1CTelemetryScript,
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
        [string]$EvtxExportRoot,
        [int]$EvtxRetentionDays = 14,
        [string[]]$EvtxChannels = @(),
        [bool]$LogonMarkerEnabled = $true,
        [bool]$ProcessEventsEnabled = $false,
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
        [bool]$HayabusaAutoUploadEnabled = $true,
        [int]$HayabusaAutoUploadIntervalHours = 6,
        [int]$HayabusaAutoUploadHoursBack = 6,
        [string]$HayabusaAutoUploadMode = 'incident',
        [string]$HayabusaAutoUploadTaskName = 'ActivityWatch Hayabusa Upload',
        [bool]$File1CAutoUploadEnabled = $true,
        [int]$File1CAutoUploadIntervalHours = 6,
        [string]$File1CAutoUploadTaskName = 'ActivityWatch File1C Upload',
        [string]$File1CTargetHost,
        [string]$File1CTargetUser = 'igor',
        [string]$File1CRemoteRoot = '/opt/activitywatch/clickhouse-1c/landing',
        [string]$File1CRegistryWorkbookPath = 'E:\USER1\СПИСОК ПРЕДПРИЯТИЙ И ИХ РАСПРЕДЕЛЕНИЕ.xlsx',
        [switch]$IntegrationTestEnabled
    )

    $effectiveIncidentArtifactsRoot = if ($IncidentArtifactsRoot) { $IncidentArtifactsRoot } else { Join-Path $StateRoot 'incident-artifacts' }
    $effectiveEvtxExportRoot = if ($EvtxExportRoot) { $EvtxExportRoot } else { Join-Path $StateRoot 'forensics\evtx-exports' }
    $effectiveEvtxChannels = if ($EvtxChannels -and $EvtxChannels.Count -gt 0) {
        @($EvtxChannels)
    } else {
        @(
            'Security',
            'System',
            'Application',
            'Microsoft-Windows-PowerShell/Operational',
            'Microsoft-Windows-TerminalServices-LocalSessionManager/Operational',
            'Microsoft-Windows-TerminalServices-RemoteConnectionManager/Operational'
        )
    }
    $effectivePolicyEngineHost = if ([string]::IsNullOrWhiteSpace($PolicyEngineHost)) { $ServerHost } else { $PolicyEngineHost }
    $effectivePolicyCachePath = if ([string]::IsNullOrWhiteSpace($PolicyCachePath)) { Join-Path $StateRoot 'dlp-policy-cache.json' } else { $PolicyCachePath }
    if ($File1CAutoUploadEnabled -and [string]::IsNullOrWhiteSpace($File1CTargetHost)) {
        throw 'File1CTargetHost is required when File1CAutoUploadEnabled is true.'
    }

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
            evtxExportScript = $EvtxExportScript
            hayabusaUploadScript = $HayabusaUploadScript
            file1cTelemetryScript = $File1CTelemetryScript
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
            worktimeSessionEnabled = $true
        }
        logging = [pscustomobject]@{
            localAgentLogsEnabled = $LocalAgentLogsEnabled
        }
        incidentCapture = [pscustomobject]@{
            enabled           = $IncidentCaptureEnabled
            screenshotEnabled = $IncidentScreenshotEnabled
            artifactsRoot     = $effectiveIncidentArtifactsRoot
        }
        forensics = [pscustomobject]@{
            evtxExportRoot = $effectiveEvtxExportRoot
            retentionDays  = $EvtxRetentionDays
            evtxChannels   = @($effectiveEvtxChannels)
            hayabusaAutomation = [pscustomobject]@{
                enabled = [bool]$HayabusaAutoUploadEnabled
                intervalHours = $HayabusaAutoUploadIntervalHours
                hoursBack = $HayabusaAutoUploadHoursBack
                mode = $HayabusaAutoUploadMode
                taskName = $HayabusaAutoUploadTaskName
            }
        }
        analytics = [pscustomobject]@{
            file1cAutomation = [pscustomobject]@{
                enabled = [bool]$File1CAutoUploadEnabled
                intervalHours = $File1CAutoUploadIntervalHours
                taskName = $File1CAutoUploadTaskName
                targetHost = $File1CTargetHost
                targetUser = $File1CTargetUser
                remoteRoot = $File1CRemoteRoot
                registryWorkbookPath = $File1CRegistryWorkbookPath
            }
        }
        sessionEvents = [pscustomobject]@{
            logonEnabled        = $LogonMarkerEnabled
            processEventsEnabled = $ProcessEventsEnabled
            bucketPrefix        = 'aw-session-events'
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
`$script:MaxCollectorPowerShellProcesses = 48
`$script:CollectorProcessSnapshotLoaded = `$false
`$script:CollectorProcessSnapshot = @()

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

function Get-CollectorProcessSnapshot {
    if (`$script:CollectorProcessSnapshotLoaded) {
        return @(`$script:CollectorProcessSnapshot)
    }

    `$script:CollectorProcessSnapshotLoaded = `$true
    `$script:CollectorProcessSnapshot = @()
    `$job = `$null
    try {
        `$job = Start-Job -ScriptBlock {
            Get-CimInstance Win32_Process -Filter "Name = 'powershell.exe' OR Name = 'pwsh.exe'" -ErrorAction SilentlyContinue |
                Where-Object {
                    `$_.CommandLine -match 'AWatch-rus' -and
                    `$_.CommandLine -match '\.ps1'
                } |
                Select-Object ProcessId, SessionId, CommandLine
        }

        if (Wait-Job -Job `$job -Timeout 4) {
            `$script:CollectorProcessSnapshot = @(Receive-Job -Job `$job -ErrorAction SilentlyContinue)
        }
    }
    catch {
        `$script:CollectorProcessSnapshot = @()
    }
    finally {
        if (`$job) {
            Stop-Job -Job `$job -ErrorAction SilentlyContinue | Out-Null
            Remove-Job -Job `$job -Force -ErrorAction SilentlyContinue | Out-Null
        }
    }

    return @(`$script:CollectorProcessSnapshot)
}

function Test-CollectorRunning {
    param(
        [string]`$ScriptPath,
        [int]`$SessionId
    )

    `$escapedCollector = [Regex]::Escape(`$ScriptPath)
    `$processes = Get-CollectorProcessSnapshot |
        Where-Object {
            `$_.SessionId -eq `$SessionId -and
            `$_.CommandLine -match `$escapedCollector
        }

    return [bool](`$processes | Select-Object -First 1)
}

function Get-CollectorPowerShellProcessCount {
    return @(Get-CollectorProcessSnapshot).Count
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

function Get-SessionMarkerToken {
    param([int]`$SessionId)

    try {
        `$explorer = Get-Process -Name 'explorer' -ErrorAction SilentlyContinue |
            Where-Object { `$_.SessionId -eq `$SessionId } |
            Sort-Object StartTime |
            Select-Object -First 1
        if (`$explorer -and `$explorer.StartTime) {
            return `$explorer.StartTime.ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
        }
    }
    catch {
    }

    try {
        `$currentProcess = Get-Process -Id `$PID -ErrorAction Stop
        if (`$currentProcess.StartTime) {
            return `$currentProcess.StartTime.ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
        }
    }
    catch {
    }

    return [string]`$SessionId
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
    if (-not [string]::IsNullOrWhiteSpace(`$stateRoot)) {
        `$markerRoots.Add((Join-Path `$stateRoot 'markers'))
    }
    if (-not [string]::IsNullOrWhiteSpace(`$env:LOCALAPPDATA)) {
        `$markerRoots.Add((Join-Path `$env:LOCALAPPDATA 'AWatch-rus\markers'))
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

    `$sessionMarkerToken = Get-SessionMarkerToken -SessionId `$SessionId
    `$markerFile = Join-Path `$markerDir ("logon-{0}-{1}-{2}.marker" -f `$env:USERNAME, `$SessionId, `$sessionMarkerToken)
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

    $modulePath = Join-Path $PSScriptRoot 'ActivityWatch.Windows.Common.psm1'
    $content = @"
param(
    [string]`$ConfigPath = '$ConfigPath'
)

Set-StrictMode -Version Latest
`$ErrorActionPreference = 'Continue'
Import-Module '$modulePath' -Force
Invoke-ActivityWatchRecoveryLoop -ConfigPath `$ConfigPath
"@

    Set-Content -LiteralPath $Path -Value $content -Encoding UTF8
}

function Get-ActivityWatchRecoveryConfigPaths {
    param([string]$PrimaryConfigPath)

    $paths = New-Object System.Collections.Generic.List[string]
    if ($PrimaryConfigPath -and (Test-Path -LiteralPath $PrimaryConfigPath)) {
        $paths.Add((Resolve-Path -LiteralPath $PrimaryConfigPath).Path)
    }

    $searchRoot = $env:ProgramData
    if ($PrimaryConfigPath) {
        $stateRoot = Split-Path -Path $PrimaryConfigPath -Parent
        $candidateRoot = Split-Path -Path $stateRoot -Parent
        if ($candidateRoot -and (Test-Path -LiteralPath $candidateRoot)) {
            $searchRoot = $candidateRoot
        }
    }

    if (Test-Path -LiteralPath $searchRoot) {
        Get-ChildItem -LiteralPath $searchRoot -Directory -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -like 'ActivityWatch*' } |
            ForEach-Object {
                $candidate = Join-Path $_.FullName 'deployment-config.json'
                if (Test-Path -LiteralPath $candidate) {
                    $paths.Add($candidate)
                }
            }
    }

    return @($paths | Sort-Object -Unique)
}

function Get-ActivityWatchRecoveryTaskDefinitions {
    param([string[]]$ConfigPaths)

    $taskMap = [ordered]@{}
    foreach ($candidatePath in @($ConfigPaths)) {
        try {
            $config = Read-ActivityWatchDeploymentConfig -Path $candidatePath
            foreach ($task in @($config.userTasks)) {
                $taskName = [string]$task.launchTaskName
                $userId = Normalize-ActivityWatchUserId -UserId ([string]$task.userId)
                $canonicalTaskName = "ActivityWatch Launch [$((Get-ActivityWatchTaskNameToken -UserId $userId))]"
                if ($canonicalTaskName -ne $taskName -and (Test-ActivityWatchScheduledTaskExistsExact -TaskName $canonicalTaskName)) {
                    $taskName = $canonicalTaskName
                }
                if (-not [string]::IsNullOrWhiteSpace($taskName) -and -not $taskMap.Contains($taskName)) {
                    $taskMap[$taskName] = [pscustomobject]@{
                        taskName = $taskName
                        userId   = $userId
                    }
                }
            }
        }
        catch {
        }
    }

    return @($taskMap.Values)
}

function New-ActivityWatchRecoveryLock {
    param([string]$PrimaryConfigPath)

    $stateRoot = if ($PrimaryConfigPath) { Split-Path -Path $PrimaryConfigPath -Parent } else { Join-Path $env:ProgramData 'AWatch-rus' }
    if (-not (Test-Path -LiteralPath $stateRoot)) {
        New-Item -Path $stateRoot -ItemType Directory -Force | Out-Null
    }

    $lockPath = Join-Path $stateRoot 'recovery-loop.lock'
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
        pid       = $PID
        createdAt = (Get-Date).ToUniversalTime().ToString('o')
    } | ConvertTo-Json -Compress
    Set-Content -LiteralPath $lockPath -Value $payload -Encoding UTF8
    return $lockPath
}

function Test-ActivityWatchCollectorRunningGlobal {
    param([string]$ScriptPath)

    if ([string]::IsNullOrWhiteSpace($ScriptPath)) {
        return $false
    }

    return [bool]@(
        Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
            Where-Object {
                ($_.Name -ieq 'powershell.exe' -or $_.Name -ieq 'pwsh.exe') -and
                $_.CommandLine -match [Regex]::Escape($ScriptPath)
            }
    ).Count
}

function Start-ActivityWatchCollectorScriptGlobalIfNeeded {
    param(
        [string]$ScriptPath,
        [string]$ConfigPath
    )

    if ([string]::IsNullOrWhiteSpace($ScriptPath)) {
        return
    }

    if (-not (Test-Path -LiteralPath $ScriptPath)) {
        return
    }

    if (Test-ActivityWatchCollectorRunningGlobal -ScriptPath $ScriptPath) {
        return
    }

    $powershellExe = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $argumentList = @('-NoProfile', '-WindowStyle', 'Hidden', '-ExecutionPolicy', 'Bypass', '-File', $ScriptPath, '-ConfigPath', $ConfigPath)
    Start-Process -FilePath $powershellExe -ArgumentList $argumentList -WindowStyle Hidden
}

function Start-ActivityWatchTaskIfNotRunning {
    param(
        [string]$TaskName,
        [string]$UserId,
        [object[]]$SessionRecords
    )

    if ([string]::IsNullOrWhiteSpace($TaskName) -or [string]::IsNullOrWhiteSpace($UserId)) {
        return $false
    }

    if (-not (Test-ActivityWatchUserHasLiveSession -UserId $UserId -SessionRecords $SessionRecords)) {
        return $false
    }

    try {
        $task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
        if (-not $task) {
            return $false
        }
        if ([string]$task.State -eq 'Running') {
            return $true
        }
        Start-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
        return $true
    }
    catch {
        return $false
    }
}

function Get-ActivityWatchLiveInteractiveSessions {
    param([object[]]$SessionRecords)

    return @(
        $SessionRecords |
            Where-Object {
                $_.IsLive -and
                $_.SessionId -gt 0 -and
                -not [string]::IsNullOrWhiteSpace([string]$_.UserName)
            } |
            Sort-Object @{ Expression = { if ([string]$_.SessionName -ieq 'console') { 0 } else { 1 } } }, @{ Expression = { [int]$_.SessionId } }
    )
}

function Resolve-ActivityWatchLiveSessionUserId {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject]$SessionRecord,
        [pscustomobject[]]$TaskDefinitions
    )

    $rawUser = [string]$SessionRecord.UserName
    if ([string]::IsNullOrWhiteSpace($rawUser)) {
        return $null
    }

    foreach ($taskDef in @($TaskDefinitions)) {
        foreach ($candidate in @(Resolve-ActivityWatchUserCandidates -UserId [string]$taskDef.userId)) {
            if ($candidate -ieq $rawUser -or
                $candidate -ieq ('{0}\{1}' -f $env:COMPUTERNAME, $rawUser) -or
                ((-not [string]::IsNullOrWhiteSpace($env:USERDOMAIN)) -and $candidate -ieq ('{0}\{1}' -f $env:USERDOMAIN, $rawUser))) {
                return [string]$taskDef.userId
            }
        }
    }

    if ($rawUser -match '^[^\\]+\\') {
        return $rawUser
    }

    return ('{0}\{1}' -f $env:COMPUTERNAME, $rawUser)
}

function Get-ActivityWatchExplorerUsersBySession {
    $map = @{}

    try {
        Get-Process explorer -IncludeUserName -ErrorAction SilentlyContinue |
            Where-Object { $_.SessionId -gt 0 -and -not [string]::IsNullOrWhiteSpace([string]$_.UserName) } |
            Sort-Object SessionId, StartTime |
            ForEach-Object {
                if (-not $map.ContainsKey([int]$_.SessionId)) {
                    $map[[int]$_.SessionId] = [string]$_.UserName
                }
            }
    }
    catch {
    }

    return $map
}

function Get-ActivityWatchDisconnectedInteractiveSessions {
    param([object[]]$SessionRecords)

    $explorerUsers = Get-ActivityWatchExplorerUsersBySession
    $result = New-Object System.Collections.Generic.List[object]

    foreach ($session in @($SessionRecords | Where-Object { -not $_.IsLive -and $_.SessionId -gt 0 })) {
        $resolvedUser = [string]$session.UserName
        if ([string]::IsNullOrWhiteSpace($resolvedUser) -and $explorerUsers.ContainsKey([int]$session.SessionId)) {
            $resolvedUser = [string]$explorerUsers[[int]$session.SessionId]
        }

        if ([string]::IsNullOrWhiteSpace($resolvedUser)) {
            continue
        }

        $result.Add([pscustomobject]@{
                SessionName = [string]$session.SessionName
                SessionId   = [int]$session.SessionId
                State       = [string]$session.State
                UserName    = $resolvedUser
            }) | Out-Null
    }

    return @($result | Sort-Object SessionId -Unique)
}

function Get-ActivityWatchMarkerDirectories {
    param([string]$StateRoot)

    $roots = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)

    if (-not [string]::IsNullOrWhiteSpace($StateRoot)) {
        [void]$roots.Add((Join-Path $StateRoot 'markers'))
    }

    $usersRoot = Join-Path $env:SystemDrive 'Users'
    if (Test-Path -LiteralPath $usersRoot) {
        foreach ($dir in @(Get-ChildItem -LiteralPath $usersRoot -Directory -ErrorAction SilentlyContinue)) {
            [void]$roots.Add((Join-Path $dir.FullName 'AppData\Local\AWatch-rus\markers'))
        }
    }

    return @($roots)
}

function Remove-ActivityWatchLogonMarkersForSession {
    param(
        [Parameter(Mandatory = $true)]
        [string]$StateRoot,
        [int]$SessionId,
        [string]$UserName
    )

    $userCandidates = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    if (-not [string]::IsNullOrWhiteSpace($UserName)) {
        [void]$userCandidates.Add($UserName)
        if ($UserName -match '^[^\\]+\\(.+)$') {
            [void]$userCandidates.Add($Matches[1])
        }
    }

    foreach ($markerDir in @(Get-ActivityWatchMarkerDirectories -StateRoot $StateRoot)) {
        if (-not (Test-Path -LiteralPath $markerDir)) {
            continue
        }

        foreach ($marker in @(Get-ChildItem -LiteralPath $markerDir -Filter '*.marker' -File -ErrorAction SilentlyContinue)) {
            $name = [string]$marker.BaseName
            if ($name -notmatch '^logon-(.+)-(\d+)-') {
                continue
            }

            $markerUser = [string]$Matches[1]
            $markerSessionId = [int]$Matches[2]
            if ($markerSessionId -ne $SessionId) {
                continue
            }

            if ($userCandidates.Count -gt 0 -and -not $userCandidates.Contains($markerUser)) {
                continue
            }

            Remove-Item -LiteralPath $marker.FullName -Force -ErrorAction SilentlyContinue
        }
    }
}

function Stop-ActivityWatchProcessesInNonLiveSessions {
    param(
        [Parameter(Mandatory = $true)]
        [object[]]$SessionRecords,
        [Parameter(Mandatory = $true)]
        [pscustomobject]$Config,
        [pscustomobject[]]$TaskDefinitions = @(),
        [switch]$PreserveManagedSessions
    )

    $stateRoot = if ($Config.paths.PSObject.Properties.Name -contains 'stateRoot') { [string]$Config.paths.stateRoot } else { Join-Path $env:ProgramData 'AWatch-rus' }
    $preservedSessionIds = @()
    if ($PreserveManagedSessions.IsPresent) {
        $preservedSessionIds = @(
            Get-ActivityWatchManagedInteractiveSessions -TaskDefinitions $TaskDefinitions -SessionRecords $SessionRecords -IncludeDisconnected |
                ForEach-Object { [int]$_.SessionId } |
                Sort-Object -Unique
        )
    }

    $sessionIds = @(
        $SessionRecords |
            Where-Object { -not $_.IsLive -and $_.SessionId -gt 0 -and ($preservedSessionIds -notcontains [int]$_.SessionId) } |
            ForEach-Object { [int]$_.SessionId } |
            Sort-Object -Unique
    )

    if (-not $sessionIds -or $sessionIds.Count -eq 0) {
        return
    }

    $sessionScopedScripts = New-Object System.Collections.Generic.List[string]
    foreach ($propertyName in @('collectorScript', 'endpointCollectorScript', 'fileCollectorScript', 'emailCollectorScript', 'launchScript')) {
        if ($Config.paths.PSObject.Properties.Name -contains $propertyName) {
            $candidatePath = [string]$Config.paths.$propertyName
            if (-not [string]::IsNullOrWhiteSpace($candidatePath)) {
                $sessionScopedScripts.Add($candidatePath) | Out-Null
            }
        }
    }

    foreach ($session in @($SessionRecords | Where-Object { -not $_.IsLive -and $_.SessionId -gt 0 -and ($preservedSessionIds -notcontains [int]$_.SessionId) })) {
        Remove-ActivityWatchLogonMarkersForSession -StateRoot $stateRoot -SessionId ([int]$session.SessionId) -UserName ([string]$session.UserName)
    }

    Get-Process -Name 'aw-watcher-afk','aw-watcher-window' -ErrorAction SilentlyContinue |
        Where-Object { $sessionIds -contains [int]$_.SessionId } |
        ForEach-Object {
            Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
        }

    Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
        Where-Object {
            ($_.Name -ieq 'powershell.exe' -or $_.Name -ieq 'pwsh.exe') -and
            ($sessionIds -contains [int]$_.SessionId)
        } |
        ForEach-Object {
            $commandLine = [string]$_.CommandLine
            foreach ($scriptPath in $sessionScopedScripts) {
                if (-not [string]::IsNullOrWhiteSpace($scriptPath) -and $commandLine -match [Regex]::Escape($scriptPath)) {
                    Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue
                    break
                }
            }
        }
}

function Promote-ActivityWatchDisconnectedSessionToConsole {
    param(
        [pscustomobject[]]$TaskDefinitions,
        [object[]]$SessionRecords
    )

    $candidates = Get-ActivityWatchDisconnectedInteractiveSessions -SessionRecords $SessionRecords
    if (-not $candidates -or $candidates.Count -eq 0) {
        return $false
    }

    $selected = $null
    foreach ($taskDef in @($TaskDefinitions)) {
        foreach ($candidate in @($candidates)) {
            foreach ($knownUser in @(Resolve-ActivityWatchUserCandidates -UserId [string]$taskDef.userId)) {
                if ($knownUser -ieq [string]$candidate.UserName -or
                    $knownUser -ieq ('{0}\{1}' -f $env:COMPUTERNAME, [string]$candidate.UserName) -or
                    ((-not [string]::IsNullOrWhiteSpace($env:USERDOMAIN)) -and $knownUser -ieq ('{0}\{1}' -f $env:USERDOMAIN, [string]$candidate.UserName))) {
                    $selected = $candidate
                    break
                }
            }
            if ($selected) { break }
        }
        if ($selected) { break }
    }

    if (-not $selected) {
        $selected = $candidates | Select-Object -First 1
    }

    if (-not $selected) {
        return $false
    }

    try {
        & cmd.exe /c ("tscon {0} /dest:console" -f [int]$selected.SessionId) | Out-Null
        return ($LASTEXITCODE -eq 0)
    }
    catch {
        return $false
    }
}

function Ensure-ActivityWatchLaunchTaskForUser {
    param(
        [Parameter(Mandatory = $true)]
        [string]$UserId,
        [Parameter(Mandatory = $true)]
        [string]$LaunchScriptPath,
        [Parameter(Mandatory = $true)]
        [string]$ConfigPath
    )

    if ([string]::IsNullOrWhiteSpace($UserId) -or -not (Test-Path -LiteralPath $LaunchScriptPath)) {
        return $null
    }

    $taskName = "ActivityWatch Launch [$((Get-ActivityWatchTaskNameToken -UserId $UserId))]"
    $launcherPath = Get-ActivityWatchHiddenLauncherPath -ScriptPath $LaunchScriptPath
    Write-ActivityWatchHiddenPowerShellWrapper -Path $launcherPath -ScriptPath $LaunchScriptPath -ConfigPath $ConfigPath

    $wscriptExe = Join-Path $env:SystemRoot 'System32\wscript.exe'
    $action = New-ScheduledTaskAction -Execute $wscriptExe -Argument "//B //NoLogo `"$launcherPath`""
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $UserId
    $principal = New-ScheduledTaskPrincipal -UserId $UserId -LogonType Interactive -RunLevel Highest
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -StartWhenAvailable -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Hours 0)

    try {
        $existingTask = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
        if ($existingTask) {
            $existingUserId = [string]$existingTask.Principal.UserId
            $existingArgs = @($existingTask.Actions | ForEach-Object { [string]$_.Arguments }) -join ' '
            if ($existingUserId -ieq $UserId -and $existingArgs -like "*$launcherPath*") {
                return $taskName
            }

            Remove-ActivityWatchScheduledTask -TaskName $taskName
        }

        Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Principal $principal -Settings $settings | Out-Null
        return $taskName
    }
    catch {
        return $null
    }
}

function Start-ActivityWatchConsoleFallbackIfNeeded {
    param(
        [pscustomobject[]]$TaskDefinitions,
        [object[]]$SessionRecords,
        [pscustomobject]$Config,
        [string]$ConfigPath,
        [bool]$ConfiguredLiveTasksStarted
    )

    if ($ConfiguredLiveTasksStarted) {
        return
    }

    $liveSessions = Get-ActivityWatchLiveInteractiveSessions -SessionRecords $SessionRecords
    if (-not $liveSessions -or $liveSessions.Count -eq 0) {
        if (Promote-ActivityWatchDisconnectedSessionToConsole -TaskDefinitions $TaskDefinitions -SessionRecords $SessionRecords) {
            Start-Sleep -Seconds 3
            $SessionRecords = Get-ActivityWatchSessionRecords
            $liveSessions = Get-ActivityWatchLiveInteractiveSessions -SessionRecords $SessionRecords
        }
    }
    if (-not $liveSessions -or $liveSessions.Count -eq 0) {
        return
    }

    $launchScriptPath = if ($Config.paths.PSObject.Properties.Name -contains 'launchScript') { [string]$Config.paths.launchScript } else { $null }
    if ([string]::IsNullOrWhiteSpace($launchScriptPath)) {
        return
    }

    $preferredSession = $liveSessions | Select-Object -First 1
    $userId = Resolve-ActivityWatchLiveSessionUserId -SessionRecord $preferredSession -TaskDefinitions $TaskDefinitions
    if ([string]::IsNullOrWhiteSpace($userId)) {
        return
    }

    $taskName = Ensure-ActivityWatchLaunchTaskForUser -UserId $userId -LaunchScriptPath $launchScriptPath -ConfigPath $ConfigPath
    if ([string]::IsNullOrWhiteSpace($taskName)) {
        return
    }

    try {
        $task = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
        if ($task -and [string]$task.State -ne 'Running') {
            Start-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
        }
    }
    catch {
    }
}

function Invoke-ActivityWatchRecoveryLoop {
    param([string]$ConfigPath)

    $recoveryLockPath = New-ActivityWatchRecoveryLock -PrimaryConfigPath $ConfigPath
    if (-not $recoveryLockPath) {
        return
    }

    try {
        while ($true) {
            $sleepSeconds = 180
            try {
                $configPaths = Get-ActivityWatchRecoveryConfigPaths -PrimaryConfigPath $ConfigPath
                $config = Read-ActivityWatchDeploymentConfig -Path $ConfigPath
                $taskDefs = Get-ActivityWatchRecoveryTaskDefinitions -ConfigPaths $configPaths
                $sessionRecords = Get-ActivityWatchSessionRecords
                Stop-ActivityWatchProcessesInNonLiveSessions -SessionRecords $sessionRecords -Config $config -TaskDefinitions $taskDefs -PreserveManagedSessions
                $sessionRecords = Get-ActivityWatchSessionRecords
                $stateRoot = [string]$config.paths.stateRoot
                $sessionCollectorScript = if ($config.paths.PSObject.Properties.Name -contains 'sessionCollectorScript') { [string]$config.paths.sessionCollectorScript } else { Join-Path $stateRoot 'worktime-session-collector.ps1' }
                $worktimeSessionEnabled = if ($config.PSObject.Properties.Name -contains 'collectors' -and $config.collectors.PSObject.Properties.Name -contains 'worktimeSessionEnabled') { [bool]$config.collectors.worktimeSessionEnabled } else { $true }
                if ($worktimeSessionEnabled) {
                    Start-ActivityWatchCollectorScriptGlobalIfNeeded -ScriptPath $sessionCollectorScript -ConfigPath $ConfigPath
                }

                $configuredLiveTasksStarted = $false
                foreach ($taskDef in $taskDefs) {
                    if (Start-ActivityWatchTaskIfNotRunning -TaskName $taskDef.taskName -UserId $taskDef.userId -SessionRecords $sessionRecords) {
                        $configuredLiveTasksStarted = $true
                    }
                }

                Start-ActivityWatchConsoleFallbackIfNeeded -TaskDefinitions $taskDefs -SessionRecords $sessionRecords -Config $config -ConfigPath $ConfigPath -ConfiguredLiveTasksStarted $configuredLiveTasksStarted

                if ($config -and $config.recovery -and $config.recovery.intervalSeconds) {
                    $sleepSeconds = [Math]::Max([int]$config.recovery.intervalSeconds, 30)
                }
            }
            catch {
            }

            Start-Sleep -Seconds $sleepSeconds
        }
    }
    finally {
        if ($recoveryLockPath -and (Test-Path -LiteralPath $recoveryLockPath)) {
            Remove-Item -LiteralPath $recoveryLockPath -Force -ErrorAction SilentlyContinue
        }
    }
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
On Error Resume Next
Set shell = CreateObject("WScript.Shell")
q = Chr(34)
command = q & "$escapedPowerShellExe" & q & " -NoProfile -ExecutionPolicy Bypass -File " & q & "$escapedScriptPath" & q & " -ConfigPath " & q & "$escapedConfigPath" & q
shell.Run command, 0, False
If Err.Number <> 0 Then
    WScript.Quit 1
End If
WScript.Quit 0
"@

    Set-Content -LiteralPath $Path -Value $content -Encoding ASCII
}

function Remove-LegacyActivityWatchEntries {
    $legacyTaskNames = @(
        'ActivityWatch Watchers',
        'ActivityWatch Guard',
        'ActivityWatch Heal',
        'AWatchRusStandaloneAgent',
        'AWatch Worktime Collector',
        'AW DLP Endpoint ADMIN',
        'AW DLP Endpoint USER1'
    )

    foreach ($taskName in $legacyTaskNames) {
        Remove-ActivityWatchScheduledTask -TaskName $taskName
    }

    $legacyTaskPatterns = @(
        'browser-domains-native-collector.ps1',
        'file-operations-collector.ps1',
        'dlp-endpoint-signals-collector.ps1',
        'worktime-session-collector.ps1',
        'aw-standalone-service.ps1'
    )
    $managedTaskNames = @(
        'ActivityWatch Recovery',
        'ActivityWatch Hayabusa Upload',
        'ActivityWatch File1C Upload'
    )

    $scheduledTasks = @()
    try {
        $scheduledTasks = @(Get-ScheduledTask -ErrorAction Stop)
    }
    catch {
        $scheduledTasks = @()
    }

    foreach ($task in $scheduledTasks) {
        $taskName = [string]$task.TaskName
        if ([string]::IsNullOrWhiteSpace($taskName) -or $managedTaskNames -contains $taskName -or $taskName -like 'ActivityWatch Launch *') {
            continue
        }

        $isLegacyCollectorTask = $false
        foreach ($action in @($task.Actions)) {
            $execute = if ($action.PSObject.Properties.Name -contains 'Execute') { [string]$action.Execute } else { '' }
            $arguments = if ($action.PSObject.Properties.Name -contains 'Arguments') { [string]$action.Arguments } else { '' }
            $commandLine = ('{0} {1}' -f $execute, $arguments).Trim()
            if ([string]::IsNullOrWhiteSpace($commandLine)) {
                continue
            }
            foreach ($pattern in $legacyTaskPatterns) {
                if ($commandLine -match [Regex]::Escape($pattern)) {
                    $isLegacyCollectorTask = $true
                    break
                }
            }
            if ($isLegacyCollectorTask) {
                break
            }
        }

        if ($isLegacyCollectorTask) {
            Remove-ActivityWatchScheduledTask -TaskName $taskName
        }
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

    try {
        Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false -ErrorAction Stop
    }
    catch {
    }

    & cmd.exe /c "schtasks /Delete /TN `"$TaskName`" /F >nul 2>&1" | Out-Null
    if ($LASTEXITCODE -eq 0) {
        return
    }

    for ($attempt = 0; $attempt -lt 10; $attempt++) {
        $task = $null
        try {
            $task = Get-ScheduledTask -TaskName $TaskName -ErrorAction Stop
        }
        catch {
            & cmd.exe /c "schtasks /Query /TN `"$TaskName`" >nul 2>&1" | Out-Null
            if ($LASTEXITCODE -ne 0) {
                return
            }
        }
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
            Write-Host "skip task action update for $TaskName because the existing principal/action cannot be updated non-interactively: $($_.Exception.Message)"
            return $false
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
    $sessionRecords = @()
    try {
        $sessionRecords = @(Get-ActivityWatchSessionRecords)
    }
    catch {
        $sessionRecords = @()
    }
    $liveSession = @(Get-ActivityWatchLiveInteractiveSessions -SessionRecords $sessionRecords) | Select-Object -First 1
    $interactiveUserId = $null
    if ($liveSession -and -not [string]::IsNullOrWhiteSpace([string]$liveSession.UserName)) {
        $rawUser = [string]$liveSession.UserName
        $interactiveUserId = if ($rawUser -match '^[^\\]+\\') { $rawUser } else { ('{0}\{1}' -f $env:COMPUTERNAME, $rawUser) }
    }

    if ($interactiveUserId) {
        $trigger = New-ScheduledTaskTrigger -AtLogOn -User $interactiveUserId
        $principal = New-ScheduledTaskPrincipal -UserId $interactiveUserId -LogonType Interactive -RunLevel Highest
    }
    else {
        $trigger = New-ScheduledTaskTrigger -AtStartup
        $principal = New-ScheduledTaskPrincipal -UserId 'SYSTEM' -LogonType ServiceAccount -RunLevel Highest
    }
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -StartWhenAvailable -Hidden -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Hours 0)

    try {
        Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Principal $principal -Settings $settings -ErrorAction Stop | Out-Null
    }
    catch {
        $taskCommand = ('"{0}" {1}' -f $wscriptExe, $action.Arguments)
        if ($interactiveUserId) {
            & schtasks.exe /Create /TN $TaskName /SC ONLOGON /RU $interactiveUserId /IT /RL HIGHEST /F /TR $taskCommand | Out-Null
        }
        else {
            & schtasks.exe /Create /TN $TaskName /SC ONSTART /RU SYSTEM /RL HIGHEST /F /TR $taskCommand | Out-Null
        }
        if ($LASTEXITCODE -ne 0) {
            throw
        }
    }
}

function Register-ActivityWatchHayabusaAutoUploadTask {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ConfigPath
    )

    $config = Read-ActivityWatchDeploymentConfig -Path $ConfigPath
    $forensics = $config.forensics
    if ($null -eq $forensics -or $forensics.PSObject.Properties.Name -notcontains 'hayabusaAutomation') {
        return
    }

    $automation = $forensics.hayabusaAutomation
    $taskName = if ($automation.PSObject.Properties.Name -contains 'taskName' -and -not [string]::IsNullOrWhiteSpace([string]$automation.taskName)) {
        [string]$automation.taskName
    } else {
        'ActivityWatch Hayabusa Upload'
    }

    if (-not [bool]$automation.enabled) {
        Remove-ActivityWatchScheduledTask -TaskName $taskName
        return
    }

    $uploadScript = if ($config.paths.PSObject.Properties.Name -contains 'hayabusaUploadScript') { [string]$config.paths.hayabusaUploadScript } else { Join-Path $config.paths.stateRoot 'export-upload-hayabusa-to-aw-server.ps1' }
    if (-not (Test-Path -LiteralPath $uploadScript)) {
        throw "Не найден скрипт Hayabusa upload: $uploadScript"
    }

    $intervalHours = [Math]::Max(1, [int]$automation.intervalHours)
    $hoursBack = [Math]::Max(1, [int]$automation.hoursBack)
    $mode = if ($automation.PSObject.Properties.Name -contains 'mode' -and -not [string]::IsNullOrWhiteSpace([string]$automation.mode)) { [string]$automation.mode } else { 'incident' }
    $powerShellExe = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $taskCommand = "`"$powerShellExe`" -NoProfile -ExecutionPolicy Bypass -File `"$uploadScript`" -ConfigPath `"$ConfigPath`" -HoursBack $hoursBack -Mode `"$mode`""

    Remove-ActivityWatchScheduledTask -TaskName $taskName
    & schtasks.exe /Create /TN $taskName /TR $taskCommand /SC HOURLY /MO $intervalHours /ST 00:00 /RU SYSTEM /RL HIGHEST /F | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "Не удалось создать scheduled task $taskName через schtasks.exe"
    }
}

function Register-ActivityWatchFile1CAutoUploadTask {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ConfigPath
    )

    $config = Read-ActivityWatchDeploymentConfig -Path $ConfigPath
    if ($config.PSObject.Properties.Name -notcontains 'analytics' -or
        $config.analytics.PSObject.Properties.Name -notcontains 'file1cAutomation') {
        return
    }

    $automation = $config.analytics.file1cAutomation
    $taskName = if ($automation.PSObject.Properties.Name -contains 'taskName' -and -not [string]::IsNullOrWhiteSpace([string]$automation.taskName)) {
        [string]$automation.taskName
    } else {
        'ActivityWatch File1C Upload'
    }

    if (-not [bool]$automation.enabled) {
        Remove-ActivityWatchScheduledTask -TaskName $taskName
        return
    }

    $uploadScript = if ($config.paths.PSObject.Properties.Name -contains 'file1cTelemetryScript') { [string]$config.paths.file1cTelemetryScript } else { Join-Path $config.paths.stateRoot 'export-upload-file-1c-telemetry.ps1' }
    if (-not (Test-Path -LiteralPath $uploadScript)) {
        throw "Не найден скрипт file-1C telemetry upload: $uploadScript"
    }

    $intervalHours = [Math]::Max(1, [int]$automation.intervalHours)
    $powerShellExe = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $taskCommand = "`"$powerShellExe`" -NoProfile -ExecutionPolicy Bypass -File `"$uploadScript`" -ConfigPath `"$ConfigPath`""
    $runnerUserId = $null
    if ($automation.PSObject.Properties.Name -contains 'runAsUser' -and -not [string]::IsNullOrWhiteSpace([string]$automation.runAsUser)) {
        $runnerUserId = [string]$automation.runAsUser
    }
    if ([string]::IsNullOrWhiteSpace($runnerUserId) -and $config.PSObject.Properties.Name -contains 'userTasks') {
        $runnerUserId = @(
            @($config.userTasks | ForEach-Object { [string]$_.userId }) |
                Where-Object { $_ -match '(^|\\)(Администратор|Administrator)$' } |
                Select-Object -First 1
        ) | Select-Object -First 1
    }
    if ([string]::IsNullOrWhiteSpace($runnerUserId) -and $config.PSObject.Properties.Name -contains 'userTasks') {
        $runnerUserId = @($config.userTasks | ForEach-Object { [string]$_.userId } | Select-Object -First 1) | Select-Object -First 1
    }

    Remove-ActivityWatchScheduledTask -TaskName $taskName
    if (-not [string]::IsNullOrWhiteSpace($runnerUserId)) {
        & schtasks.exe /Create /TN $taskName /TR $taskCommand /SC HOURLY /MO $intervalHours /ST 00:00 /RU $runnerUserId /RL HIGHEST /F | Out-Null
    }
    else {
        & schtasks.exe /Create /TN $taskName /TR $taskCommand /SC HOURLY /MO $intervalHours /ST 00:00 /RU SYSTEM /RL HIGHEST /F | Out-Null
    }
    if ($LASTEXITCODE -ne 0) {
        throw "Не удалось создать scheduled task $taskName через schtasks.exe"
    }
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

    & icacls $StateRoot /inheritance:r /grant:r '*S-1-5-18:(OI)(CI)(F)' '*S-1-5-32-544:(OI)(CI)(F)' '*S-1-5-32-545:(OI)(CI)(M)' | Out-Null
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

    $sessionRecords = Get-ActivityWatchSessionRecords

    foreach ($definition in $TaskDefinitions) {
        if (Test-ActivityWatchUserHasLiveSession -UserId $definition.UserId -SessionRecords $sessionRecords) {
            Start-ScheduledTask -TaskName $definition.LaunchTaskName -ErrorAction SilentlyContinue
        }
    }

    Start-ScheduledTask -TaskName $RecoveryTaskName -ErrorAction SilentlyContinue
}

Export-ModuleMember -Function *-ActivityWatch*, Assert-Administrator, Normalize-ActivityWatchUsers, Get-ActivityWatchPackageUrl, Remove-LegacyActivityWatchEntries
