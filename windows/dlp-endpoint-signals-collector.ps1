[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\ActivityWatch\deployment-config.json',
    [string]$ServerHost,
    [int]$ServerPort,
    [ValidateSet('http', 'https')]
    [string]$ServerScheme,
    [string]$PolicyPath,
    [string]$LogPath,
    [int]$PollSeconds
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-DeploymentConfig {
    param([string]$Path)
    if ($Path -and (Test-Path -LiteralPath $Path)) {
        return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    }
    return $null
}

function Write-EndpointLog {
    param([string]$Message)
    try {
        Add-Content -LiteralPath $script:LogPath -Value ('{0} {1}' -f (Get-Date -Format s), $Message)
    }
    catch {
    }
}

function Ensure-Bucket {
    param(
        [string]$BucketId,
        [string]$ClientName,
        [string]$BucketType
    )

    if ($script:KnownBuckets.ContainsKey($BucketId)) {
        return
    }

    $body = @{
        client   = $ClientName
        type     = $BucketType
        hostname = $script:Hostname
    } | ConvertTo-Json -Compress

    Invoke-RestMethod -Method Post -Uri "$($script:ApiBase)/buckets/$BucketId" -ContentType 'application/json' -Body $body | Out-Null
    $script:KnownBuckets[$BucketId] = $true
}

function Send-EndpointSignalHeartbeat {
    param(
        [string]$SignalType,
        [hashtable]$Data
    )

    $bucketId = 'aw-dlp-endpoint-signals_' + $script:Hostname
    Ensure-Bucket -BucketId $bucketId -ClientName 'aw-dlp-endpoint-signals' -BucketType 'aw.dlp.endpoint.signal'

    $payload = @{
        timestamp = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
        duration  = 0
        data      = @{
            signalType = $SignalType
            username   = $env:USERNAME
            sessionId  = $script:SessionId
            hostname   = $script:Hostname
            source     = 'endpoint-signals-phase2'
        } + $Data
    } | ConvertTo-Json -Depth 6 -Compress

    Invoke-RestMethod -Method Post -Uri "$($script:ApiBase)/buckets/$bucketId/heartbeat?pulsetime=$script:PulseSeconds" -ContentType 'application/json' -Body $payload | Out-Null
}

function Send-DlpIncidentHeartbeat {
    param(
        [string]$RuleId,
        [string]$Action,
        [string]$Severity,
        [string]$Message,
        [string]$SignalType,
        [hashtable]$Data
    )

    $bucketId = 'aw-dlp-incidents_' + $script:Hostname
    Ensure-Bucket -BucketId $bucketId -ClientName 'aw-dlp-incidents' -BucketType 'aw.dlp.incident'

    $payload = @{
        timestamp = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
        duration  = 0
        data      = @{
            ruleId    = $RuleId
            action    = $Action
            severity  = $Severity
            message   = $Message
            signalType = $SignalType
            username  = $env:USERNAME
            sessionId = $script:SessionId
            hostname  = $script:Hostname
            source    = 'endpoint-signals-phase2'
        } + $Data
    } | ConvertTo-Json -Depth 7 -Compress

    Invoke-RestMethod -Method Post -Uri "$($script:ApiBase)/buckets/$bucketId/heartbeat?pulsetime=$script:PulseSeconds" -ContentType 'application/json' -Body $payload | Out-Null
}

function Get-StringHash {
    param([AllowNull()][string]$Value)
    if ($null -eq $Value) { return $null }
    $bytes = [Text.Encoding]::UTF8.GetBytes($Value)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        ($sha.ComputeHash($bytes) | ForEach-Object { $_.ToString('x2') }) -join ''
    }
    finally {
        $sha.Dispose()
    }
}

function Load-DlpPolicy {
    param([string]$Path)

    $script:Policy = [ordered]@{
        defaults = [ordered]@{
            enabled = $true
            cooldownSeconds = 300
            action = 'alert'
            severity = 'medium'
        }
        endpoint = [ordered]@{
            clipboard = @()
            usb = @()
            print = @()
        }
    }

    if (-not $Path -or -not (Test-Path -LiteralPath $Path)) {
        Write-EndpointLog ("policy not found, using defaults: {0}" -f $Path)
        return
    }

    try {
        $raw = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
        if ($raw.defaults) {
            if ($raw.defaults.PSObject.Properties.Name -contains 'enabled') { $script:Policy.defaults.enabled = [bool]$raw.defaults.enabled }
            if ($raw.defaults.cooldownSeconds) { $script:Policy.defaults.cooldownSeconds = [int]$raw.defaults.cooldownSeconds }
            if ($raw.defaults.action) { $script:Policy.defaults.action = [string]$raw.defaults.action }
            if ($raw.defaults.severity) { $script:Policy.defaults.severity = [string]$raw.defaults.severity }
        }

        if ($raw.endpoint) {
            if ($raw.endpoint.clipboard) { $script:Policy.endpoint.clipboard = @($raw.endpoint.clipboard) }
            if ($raw.endpoint.usb) { $script:Policy.endpoint.usb = @($raw.endpoint.usb) }
            if ($raw.endpoint.print) { $script:Policy.endpoint.print = @($raw.endpoint.print) }
        }
    }
    catch {
        Write-EndpointLog ("policy parse failed: {0}" -f $_.Exception.Message)
    }
}

function Should-EmitByCooldown {
    param(
        [string]$Fingerprint,
        [int]$CooldownSeconds
    )

    $now = (Get-Date).ToUniversalTime()
    if ($script:Cooldown.ContainsKey($Fingerprint)) {
        $last = [datetime]$script:Cooldown[$Fingerprint]
        if ((New-TimeSpan -Start $last -End $now).TotalSeconds -lt $CooldownSeconds) {
            return $false
        }
    }

    $script:Cooldown[$Fingerprint] = $now
    return $true
}

function Evaluate-ClipboardRules {
    param(
        [string]$ClipboardText,
        [string]$ClipboardHash
    )

    foreach ($rule in @($script:Policy.endpoint.clipboard)) {
        if (-not $rule) { continue }
        if ($rule.PSObject.Properties.Name -contains 'enabled' -and -not [bool]$rule.enabled) { continue }
        $ruleId = [string]$rule.id
        if (-not $ruleId) { continue }
        $minLength = if ($rule.minLength) { [int]$rule.minLength } else { 0 }
        $regexPatterns = if ($rule.regexPatterns) { @($rule.regexPatterns) } else { @() }
        if ($ClipboardText.Length -lt $minLength) { continue }

        $matched = $false
        foreach ($pattern in $regexPatterns) {
            if ($ClipboardText -match [string]$pattern) {
                $matched = $true
                break
            }
        }

        if (-not $matched) { continue }

        $cooldown = if ($rule.cooldownSeconds) { [int]$rule.cooldownSeconds } else { [int]$script:Policy.defaults.cooldownSeconds }
        $fingerprint = "clipboard|$ruleId|$ClipboardHash|$env:USERNAME"
        if (-not (Should-EmitByCooldown -Fingerprint $fingerprint -CooldownSeconds ([Math]::Max($cooldown, 30)))) { continue }

        $action = if ($rule.action) { [string]$rule.action } else { [string]$script:Policy.defaults.action }
        $severity = if ($rule.severity) { [string]$rule.severity } else { [string]$script:Policy.defaults.severity }
        $message = if ($rule.message) { [string]$rule.message } else { "Clipboard rule matched: $ruleId" }

        Send-DlpIncidentHeartbeat -RuleId $ruleId -Action $action -Severity $severity -Message $message -SignalType 'clipboard' -Data @{
            clipboardHash = $ClipboardHash
            clipboardLength = $ClipboardText.Length
        }
        Write-EndpointLog ("incident clipboard rule={0} action={1} severity={2}" -f $ruleId, $action, $severity)
    }
}

function Evaluate-UsbRules {
    param(
        [string]$DriveLetter,
        [string]$VolumeName
    )

    foreach ($rule in @($script:Policy.endpoint.usb)) {
        if (-not $rule) { continue }
        if ($rule.PSObject.Properties.Name -contains 'enabled' -and -not [bool]$rule.enabled) { continue }
        $ruleId = [string]$rule.id
        if (-not $ruleId) { continue }

        $cooldown = if ($rule.cooldownSeconds) { [int]$rule.cooldownSeconds } else { [int]$script:Policy.defaults.cooldownSeconds }
        $fingerprint = "usb|$ruleId|$DriveLetter|$env:USERNAME"
        if (-not (Should-EmitByCooldown -Fingerprint $fingerprint -CooldownSeconds ([Math]::Max($cooldown, 30)))) { continue }

        $action = if ($rule.action) { [string]$rule.action } else { [string]$script:Policy.defaults.action }
        $severity = if ($rule.severity) { [string]$rule.severity } else { [string]$script:Policy.defaults.severity }
        $message = if ($rule.message) { [string]$rule.message } else { "USB rule matched: $ruleId" }

        Send-DlpIncidentHeartbeat -RuleId $ruleId -Action $action -Severity $severity -Message $message -SignalType 'usb_insert' -Data @{
            driveLetter = $DriveLetter
            volumeName  = $VolumeName
        }
        Write-EndpointLog ("incident usb rule={0} action={1} severity={2} drive={3}" -f $ruleId, $action, $severity, $DriveLetter)
    }
}

function Evaluate-PrintRules {
    param(
        [string]$PrinterName,
        [string]$DocumentName,
        [string]$Owner
    )

    foreach ($rule in @($script:Policy.endpoint.print)) {
        if (-not $rule) { continue }
        if ($rule.PSObject.Properties.Name -contains 'enabled' -and -not [bool]$rule.enabled) { continue }
        $ruleId = [string]$rule.id
        if (-not $ruleId) { continue }

        $match = $true
        if ($rule.printerRegex) {
            $match = $match -and ($PrinterName -match [string]$rule.printerRegex)
        }
        if ($rule.documentRegex) {
            $match = $match -and ($DocumentName -match [string]$rule.documentRegex)
        }
        if (-not $match) { continue }

        $cooldown = if ($rule.cooldownSeconds) { [int]$rule.cooldownSeconds } else { [int]$script:Policy.defaults.cooldownSeconds }
        $fingerprint = "print|$ruleId|$PrinterName|$Owner|$env:USERNAME"
        if (-not (Should-EmitByCooldown -Fingerprint $fingerprint -CooldownSeconds ([Math]::Max($cooldown, 30)))) { continue }

        $action = if ($rule.action) { [string]$rule.action } else { [string]$script:Policy.defaults.action }
        $severity = if ($rule.severity) { [string]$rule.severity } else { [string]$script:Policy.defaults.severity }
        $message = if ($rule.message) { [string]$rule.message } else { "Print rule matched: $ruleId" }

        Send-DlpIncidentHeartbeat -RuleId $ruleId -Action $action -Severity $severity -Message $message -SignalType 'print_job' -Data @{
            printerName  = $PrinterName
            documentName = $DocumentName
            owner        = $Owner
        }
        Write-EndpointLog ("incident print rule={0} action={1} severity={2} printer={3}" -f $ruleId, $action, $severity, $PrinterName)
    }
}

$deploymentConfig = Get-DeploymentConfig -Path $ConfigPath
$resolvedServerHost = if ($ServerHost) { $ServerHost } elseif ($deploymentConfig) { [string]$deploymentConfig.server.host } else { throw 'ServerHost is required.' }
$resolvedServerPort = if ($PSBoundParameters.ContainsKey('ServerPort')) { $ServerPort } elseif ($deploymentConfig) { [int]$deploymentConfig.server.port } else { 5600 }
$resolvedServerScheme = if ($ServerScheme) { $ServerScheme } elseif ($deploymentConfig) { [string]$deploymentConfig.server.scheme } else { 'http' }
$resolvedPolicyPath = if ($PolicyPath) { $PolicyPath } elseif ($deploymentConfig -and $deploymentConfig.paths.PSObject.Properties.Name -contains 'policyPath') { [string]$deploymentConfig.paths.policyPath } else { 'C:\ProgramData\ActivityWatch\dlp-policy.json' }
$resolvedPollSeconds = if ($PSBoundParameters.ContainsKey('PollSeconds')) { $PollSeconds } elseif ($deploymentConfig) { [int]$deploymentConfig.collector.pollSeconds } else { 5 }
$resolvedLogsRoot = if ($deploymentConfig) { [string]$deploymentConfig.paths.logsRoot } else { 'C:\ProgramData\ActivityWatch\logs' }
$resolvedLogPath = if ($LogPath) { $LogPath } else { Join-Path $resolvedLogsRoot ("endpoint-signals-{0}.log" -f $env:USERNAME) }

if (-not (Test-Path -LiteralPath $resolvedLogsRoot)) {
    New-Item -Path $resolvedLogsRoot -ItemType Directory -Force | Out-Null
}

$script:ApiBase = '{0}://{1}:{2}/api/0' -f $resolvedServerScheme, $resolvedServerHost, $resolvedServerPort
$script:Hostname = $env:COMPUTERNAME
$script:SessionId = (Get-Process -Id $PID).SessionId
$script:KnownBuckets = @{}
$script:Cooldown = @{}
$script:SeenUsb = @{}
$script:SeenPrintJob = @{}
$script:LastClipboardHash = $null
$script:PulseSeconds = [Math]::Max($resolvedPollSeconds * 3, 30)
$script:LogPath = $resolvedLogPath

Load-DlpPolicy -Path $resolvedPolicyPath
Write-EndpointLog ("endpoint collector started against {0}" -f $script:ApiBase)

while ($true) {
    try {
        if (-not $script:Policy.defaults.enabled) {
            Start-Sleep -Seconds $resolvedPollSeconds
            continue
        }

        try {
            $clipboardText = Get-Clipboard -Raw -ErrorAction SilentlyContinue
            if ($clipboardText) {
                $clipboardHash = Get-StringHash -Value $clipboardText
                if ($clipboardHash -and $clipboardHash -ne $script:LastClipboardHash) {
                    $script:LastClipboardHash = $clipboardHash
                    Send-EndpointSignalHeartbeat -SignalType 'clipboard_change' -Data @{
                        clipboardHash = $clipboardHash
                        clipboardLength = $clipboardText.Length
                    }
                    Evaluate-ClipboardRules -ClipboardText $clipboardText -ClipboardHash $clipboardHash
                }
            }
        }
        catch {
        }

        try {
            $usbDrives = Get-CimInstance Win32_LogicalDisk -Filter "DriveType=2" -ErrorAction SilentlyContinue
            $currentUsb = @{}
            foreach ($drive in @($usbDrives)) {
                $deviceId = [string]$drive.DeviceID
                if (-not $deviceId) { continue }
                $currentUsb[$deviceId] = $true
                if (-not $script:SeenUsb.ContainsKey($deviceId)) {
                    $script:SeenUsb[$deviceId] = (Get-Date).ToUniversalTime()
                    $volumeName = [string]$drive.VolumeName
                    Send-EndpointSignalHeartbeat -SignalType 'usb_insert' -Data @{
                        driveLetter = $deviceId
                        volumeName  = $volumeName
                    }
                    Evaluate-UsbRules -DriveLetter $deviceId -VolumeName $volumeName
                }
            }

            foreach ($known in @($script:SeenUsb.Keys)) {
                if (-not $currentUsb.ContainsKey($known)) {
                    $script:SeenUsb.Remove($known)
                }
            }
        }
        catch {
        }

        try {
            $printJobs = Get-CimInstance Win32_PrintJob -ErrorAction SilentlyContinue
            foreach ($job in @($printJobs)) {
                $jobId = [string]$job.JobId
                if (-not $jobId) { continue }
                if ($script:SeenPrintJob.ContainsKey($jobId)) { continue }
                $script:SeenPrintJob[$jobId] = (Get-Date).ToUniversalTime()

                $printerName = [string]$job.Name
                $documentName = [string]$job.Document
                $owner = [string]$job.Owner

                Send-EndpointSignalHeartbeat -SignalType 'print_job' -Data @{
                    printerName  = $printerName
                    documentName = $documentName
                    owner        = $owner
                }
                Evaluate-PrintRules -PrinterName $printerName -DocumentName $documentName -Owner $owner
            }

            $cleanupBefore = (Get-Date).ToUniversalTime().AddHours(-8)
            foreach ($k in @($script:SeenPrintJob.Keys)) {
                $ts = [datetime]$script:SeenPrintJob[$k]
                if ($ts -lt $cleanupBefore) {
                    $script:SeenPrintJob.Remove($k)
                }
            }
        }
        catch {
        }
    }
    catch {
        Write-EndpointLog ("collector error: {0}" -f $_.Exception.Message)
    }

    Start-Sleep -Seconds $resolvedPollSeconds
}
