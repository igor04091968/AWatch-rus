[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\AWatch-rus\deployment-config.json',
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
    if (-not $script:LocalAgentLogsEnabled) {
        return
    }
    try {
        Add-Content -LiteralPath $script:LogPath -Value ('{0} {1}' -f (Get-Date -Format s), $Message)
    }
    catch {
    }
}

function Invoke-AwJsonPost {
    param(
        [Parameter(Mandatory = $true)][string]$Uri,
        [Parameter(Mandatory = $true)][string]$Json
    )

    $bytes = [Text.Encoding]::UTF8.GetBytes($Json)
    Invoke-RestMethod -Method Post -Uri $Uri -ContentType 'application/json; charset=utf-8' -Body $bytes | Out-Null
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

    Invoke-AwJsonPost -Uri "$($script:ApiBase)/buckets/$BucketId" -Json $body
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

    Invoke-AwJsonPost -Uri "$($script:ApiBase)/buckets/$bucketId/heartbeat?pulsetime=$script:PulseSeconds" -Json $payload
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

    $captureData = @{}
    if ($script:IncidentScreenshotEnabled) {
        try {
            $captureData = Capture-IncidentScreenshot -RuleId $RuleId -SignalType $SignalType
        }
        catch {
        }
    }

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
        } + $Data + $captureData
    } | ConvertTo-Json -Depth 7 -Compress

    Invoke-AwJsonPost -Uri "$($script:ApiBase)/buckets/$bucketId/heartbeat?pulsetime=$script:PulseSeconds" -Json $payload
}

function Get-FileSha256Hex {
    param([Parameter(Mandatory = $true)][string]$Path)
    try {
        $sha = [Security.Cryptography.SHA256]::Create()
        $stream = [IO.File]::OpenRead($Path)
        try {
            ($sha.ComputeHash($stream) | ForEach-Object { $_.ToString('x2') }) -join ''
        }
        finally {
            $stream.Dispose()
            $sha.Dispose()
        }
    }
    catch {
        return $null
    }
}

function Ensure-Directory {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -Path $Path -ItemType Directory -Force | Out-Null
    }
}

function Get-IncidentScreenshotPath {
    param(
        [Parameter(Mandatory = $true)][string]$RuleId,
        [Parameter(Mandatory = $true)][string]$SignalType
    )

    $safeUser = ($env:USERNAME -replace '[^A-Za-z0-9_.-]', '_')
    $safeRule = ($RuleId -replace '[^A-Za-z0-9_.-]', '_')
    $safeType = ($SignalType -replace '[^A-Za-z0-9_.-]', '_')
    $stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMdd_HHmmss_fff')
    $file = '{0}_{1}_sid{2}_{3}_{4}.png' -f $script:Hostname, $safeUser, $script:SessionId, $safeType, $safeRule
    $file = '{0}_{1}' -f $stamp, $file
    return (Join-Path $script:IncidentArtifactsRoot $file)
}

function Ensure-ScreenshotTypesLoaded {
    if ($script:ScreenshotTypesLoaded) {
        return
    }
    Add-Type -AssemblyName System.Windows.Forms | Out-Null
    Add-Type -AssemblyName System.Drawing | Out-Null
    $script:ScreenshotTypesLoaded = $true
}

function Capture-IncidentScreenshot {
    param(
        [Parameter(Mandatory = $true)][string]$RuleId,
        [Parameter(Mandatory = $true)][string]$SignalType
    )

    try {
        Ensure-Directory -Path $script:IncidentArtifactsRoot
        Ensure-ScreenshotTypesLoaded

        $vs = [System.Windows.Forms.SystemInformation]::VirtualScreen
        $bmp = New-Object System.Drawing.Bitmap ([int]$vs.Width), ([int]$vs.Height)
        $gfx = [System.Drawing.Graphics]::FromImage($bmp)
        try {
            $gfx.CopyFromScreen([int]$vs.Left, [int]$vs.Top, 0, 0, $bmp.Size)
            $path = Get-IncidentScreenshotPath -RuleId $RuleId -SignalType $SignalType
            $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
        }
        finally {
            $gfx.Dispose()
            $bmp.Dispose()
        }

        return @{
            screenshotPath   = $path
            screenshotFormat = 'png'
            screenshotWidth  = [int]$vs.Width
            screenshotHeight = [int]$vs.Height
            screenshotSha256 = (Get-FileSha256Hex -Path $path)
        }
    }
    catch {
        Write-EndpointLog ("screenshot capture failed: {0}" -f $_.Exception.Message)
        return @{}
    }
}

# ---------------------------------------------------------------------------
# Enforcement functions (action = "block")
# ---------------------------------------------------------------------------

function Show-EnforcementNotification {
    param(
        [Parameter(Mandatory = $true)][string]$Title,
        [Parameter(Mandatory = $true)][string]$Body
    )
    try {
        Add-Type -AssemblyName System.Windows.Forms -ErrorAction SilentlyContinue
        $icon = New-Object System.Windows.Forms.NotifyIcon
        $icon.Icon = [System.Drawing.SystemIcons]::Warning
        $icon.BalloonTipTitle = $Title
        $icon.BalloonTipText = $Body
        $icon.BalloonTipIcon = [System.Windows.Forms.ToolTipIcon]::Warning
        $icon.Visible = $true
        $icon.ShowBalloonTip(5000)
        Start-Sleep -Milliseconds 200
        $icon.Dispose()
    }
    catch {
        Write-EndpointLog ("notification failed: {0}" -f $_.Exception.Message)
    }
}

function Invoke-ClipboardEnforcement {
    [OutputType([bool])]
    param()
    try {
        Set-Clipboard -Value $null -ErrorAction Stop
        Write-EndpointLog "enforcement: clipboard cleared"
        return $true
    }
    catch {
        Write-EndpointLog ("enforcement: clipboard clear failed: {0}" -f $_.Exception.Message)
        return $false
    }
}

function Invoke-UsbWriteBlockEnforcement {
    [OutputType([bool])]
    param(
        [Parameter(Mandatory = $true)][string]$DriveLetter
    )
    try {
        $partition = Get-Partition -DriveLetter ($DriveLetter.TrimEnd(':')) -ErrorAction Stop
        $disk = Get-Disk -Number $partition.DiskNumber -ErrorAction Stop
        if ($disk.BusType -ne 'USB') {
            Write-EndpointLog ("enforcement: skip non-USB disk {0} bus={1}" -f $disk.Number, $disk.BusType)
            return $false
        }
        if (-not $disk.IsReadOnly) {
            Set-Disk -Number $disk.Number -IsReadOnly $true -ErrorAction Stop
            Write-EndpointLog ("enforcement: USB disk {0} ({1}) set read-only" -f $disk.Number, $DriveLetter)
        }
        return $true
    }
    catch {
        Write-EndpointLog ("enforcement: USB write-block failed drive={0}: {1}" -f $DriveLetter, $_.Exception.Message)
        return $false
    }
}

function Invoke-PrintJobEnforcement {
    [OutputType([bool])]
    param(
        [Parameter(Mandatory = $true)][string]$PrinterName,
        [string]$DocumentName,
        [string]$Owner
    )
    $cancelled = $false
    try {
        $jobs = Get-CimInstance Win32_PrintJob -ErrorAction SilentlyContinue
        foreach ($job in @($jobs)) {
            $jobPrinter = [string]$job.Name
            $jobOwner = [string]$job.Owner
            $jobDoc = [string]$job.Document
            $matchPrinter = ($jobPrinter -like "*$PrinterName*")
            $matchOwner = (-not $Owner) -or ($jobOwner -like "*$Owner*") -or ($jobOwner -like "*$env:USERNAME*")
            if ($matchPrinter -and $matchOwner) {
                Remove-CimInstance -InputObject $job -ErrorAction Stop
                Write-EndpointLog ("enforcement: print job cancelled id={0} printer={1} doc={2}" -f $job.JobId, $jobPrinter, $jobDoc)
                $cancelled = $true
            }
        }
    }
    catch {
        Write-EndpointLog ("enforcement: print cancel failed printer={0}: {1}" -f $PrinterName, $_.Exception.Message)
    }
    return $cancelled
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

        $enforced = $false
        if ($action -eq 'block') {
            $enforced = Invoke-ClipboardEnforcement
            Show-EnforcementNotification -Title 'DLP: буфер обмена очищен' -Body $message
        }

        Send-DlpIncidentHeartbeat -RuleId $ruleId -Action $action -Severity $severity -Message $message -SignalType 'clipboard' -Data @{
            clipboardHash = $ClipboardHash
            clipboardLength = $ClipboardText.Length
            enforced = $enforced
        }
        Write-EndpointLog ("incident clipboard rule={0} action={1} severity={2} enforced={3}" -f $ruleId, $action, $severity, $enforced)
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

        $enforced = $false
        if ($action -eq 'block') {
            $enforced = Invoke-UsbWriteBlockEnforcement -DriveLetter $DriveLetter
            Show-EnforcementNotification -Title 'DLP: USB заблокирован для записи' -Body $message
        }

        Send-DlpIncidentHeartbeat -RuleId $ruleId -Action $action -Severity $severity -Message $message -SignalType 'usb_insert' -Data @{
            driveLetter = $DriveLetter
            volumeName  = $VolumeName
            enforced = $enforced
        }
        Write-EndpointLog ("incident usb rule={0} action={1} severity={2} drive={3} enforced={4}" -f $ruleId, $action, $severity, $DriveLetter, $enforced)
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

        $enforced = $false
        if ($action -eq 'block') {
            $enforced = Invoke-PrintJobEnforcement -PrinterName $PrinterName -DocumentName $DocumentName -Owner $Owner
            Show-EnforcementNotification -Title 'DLP: печать заблокирована' -Body $message
        }

        Send-DlpIncidentHeartbeat -RuleId $ruleId -Action $action -Severity $severity -Message $message -SignalType 'print_job' -Data @{
            printerName  = $PrinterName
            documentName = $DocumentName
            owner        = $Owner
            enforced = $enforced
        }
        Write-EndpointLog ("incident print rule={0} action={1} severity={2} printer={3} enforced={4}" -f $ruleId, $action, $severity, $PrinterName, $enforced)
    }
}

function Test-LooksLikeMojibakeQuestionMarks {
    param([AllowNull()][string]$Value)
    if ([string]::IsNullOrWhiteSpace($Value)) { return $true }
    return $Value -match '\?{2,}'
}

function Normalize-OwnerForMatch {
    param([AllowNull()][string]$Value)
    if ([string]::IsNullOrWhiteSpace($Value)) { return '' }
    $normalized = $Value.Trim().ToLowerInvariant()
    if ($normalized -match '[\\/]') {
        $parts = $normalized -split '[\\/]'
        if ($parts.Count -gt 0) {
            $normalized = [string]$parts[$parts.Count - 1]
        }
    }
    if ($normalized -match '@') {
        $parts = $normalized -split '@'
        if ($parts.Count -gt 0) {
            $normalized = [string]$parts[0]
        }
    }
    return $normalized
}

function Test-OwnerLooseMatch {
    param(
        [string]$Expected,
        [string]$Actual
    )
    $expectedNorm = Normalize-OwnerForMatch -Value $Expected
    $actualNorm = Normalize-OwnerForMatch -Value $Actual
    if ([string]::IsNullOrWhiteSpace($expectedNorm) -or [string]::IsNullOrWhiteSpace($actualNorm)) {
        return $false
    }
    return ($actualNorm -eq $expectedNorm) -or $actualNorm.Contains($expectedNorm) -or $expectedNorm.Contains($actualNorm)
}

function Normalize-PrinterForMatch {
    param([AllowNull()][string]$Value)
    if ([string]::IsNullOrWhiteSpace($Value)) { return '' }
    $normalized = $Value.Trim().ToLowerInvariant()
    if ($normalized.Contains(',')) {
        $normalized = ($normalized -split ',', 2)[0].Trim()
    }
    if ($normalized -match '\son\s') {
        $normalized = ($normalized -split '\son\s', 2)[0].Trim()
    }
    return $normalized
}

function Test-PrinterLooseMatch {
    param(
        [string]$Expected,
        [string]$Actual
    )
    $expectedNorm = Normalize-PrinterForMatch -Value $Expected
    $actualNorm = Normalize-PrinterForMatch -Value $Actual
    if ([string]::IsNullOrWhiteSpace($expectedNorm) -or [string]::IsNullOrWhiteSpace($actualNorm)) {
        return $false
    }
    return ($actualNorm -eq $expectedNorm) -or $actualNorm.Contains($expectedNorm) -or $expectedNorm.Contains($actualNorm)
}

function Get-PrintServiceEventSummary {
    param([Parameter(Mandatory = $true)]$Event)

    $props = @($Event.Properties)
    $propertyValues = @()
    foreach ($prop in $props) {
        $propertyValues += [string]$prop.Value
    }

    [pscustomobject]@{
        RecordId      = [string]$Event.RecordId
        TimeCreated   = if ($Event.TimeCreated) { $Event.TimeCreated.ToString('o') } else { '' }
        PropertyCount  = $props.Count
        DocumentName   = if ($props.Count -ge 1) { [string]$props[0].Value } else { '' }
        Owner         = if ($props.Count -ge 2) { [string]$props[1].Value } else { '' }
        PrinterName   = if ($props.Count -ge 4) { [string]$props[3].Value } else { '' }
        PropertyValues = $propertyValues
    }
}

function Get-PrintServiceDocumentFallback {
    param(
        [Parameter(Mandatory = $true)]$EventSummary,
        [string]$Owner,
        [string]$PrinterName
    )

    $preferred = [string]$EventSummary.DocumentName
    if (-not (Test-LooksLikeMojibakeQuestionMarks -Value $preferred) -and $preferred -notmatch '^[0-9]+$') {
        return $preferred
    }

    $pathCandidates = New-Object System.Collections.Generic.List[string]
    $textCandidates = New-Object System.Collections.Generic.List[string]

    foreach ($value in @($EventSummary.PropertyValues)) {
        $candidate = [string]$value
        if ([string]::IsNullOrWhiteSpace($candidate)) { continue }
        if ($candidate -eq $preferred) { continue }
        if ($Owner -and $candidate -like "*$Owner*") { continue }
        if ($PrinterName -and $candidate -like "*$PrinterName*") { continue }
        if (Test-LooksLikeMojibakeQuestionMarks -Value $candidate) { continue }

        if ($candidate -match '[\\/:]' -and $candidate -match '\.[A-Za-z0-9]{1,8}$') {
            $pathCandidates.Add($candidate)
            continue
        }

        if ($candidate -match '^[0-9]+$') {
            continue
        }

        $textCandidates.Add($candidate)
    }

    foreach ($candidate in @($pathCandidates)) {
        $leaf = Split-Path -Path $candidate -Leaf
        if (-not [string]::IsNullOrWhiteSpace($leaf)) {
            return $leaf
        }
        return $candidate
    }

    foreach ($candidate in @($textCandidates)) {
        return $candidate
    }

    return $null
}

function Write-PrintServiceEventTrace {
    param(
        [Parameter(Mandatory = $true)]$EventSummary,
        [string]$Phase,
        [string]$MatchReason,
        [string]$ResolvedDocument
    )

    $properties = if ($EventSummary.PropertyValues) {
        ($EventSummary.PropertyValues -join ' | ')
    }
    else {
        ''
    }

    Write-EndpointLog (
        'printservice-307 phase={0} recordId={1} time={2} owner={3} printer={4} document={5} resolved={6} properties=[{7}] reason={8}' -f
        $Phase,
        $EventSummary.RecordId,
        $EventSummary.TimeCreated,
        $EventSummary.Owner,
        $EventSummary.PrinterName,
        $EventSummary.DocumentName,
        $ResolvedDocument,
        $properties,
        $MatchReason
    )
}

function Get-BetterDocumentNameFromPrintServiceEvents {
    param(
        [string]$Owner,
        [string]$PrinterName
    )

    try {
        $startTime = (Get-Date).AddMinutes(-15)
        $events = Get-WinEvent -FilterHashtable @{
            LogName   = 'Microsoft-Windows-PrintService/Operational'
            Id        = 307
            StartTime = $startTime
        } -MaxEvents 200 -ErrorAction Stop

        foreach ($pass in @('strict', 'relaxed')) {
            foreach ($event in @($events)) {
                $summary = Get-PrintServiceEventSummary -Event $event
                $resolvedDocument = Get-PrintServiceDocumentFallback -EventSummary $summary -Owner $Owner -PrinterName $PrinterName

                $ownerMatches = if ($Owner) { Test-OwnerLooseMatch -Expected $Owner -Actual $summary.Owner } else { $true }
                $printerMatches = if ($PrinterName) { Test-PrinterLooseMatch -Expected $PrinterName -Actual $summary.PrinterName } else { $true }

                if ($pass -eq 'strict') {
                    if ($Owner -and -not $ownerMatches) {
                        Write-PrintServiceEventTrace -EventSummary $summary -Phase 'scan' -MatchReason 'owner-mismatch-strict' -ResolvedDocument $resolvedDocument
                        continue
                    }
                    if ($PrinterName -and -not $printerMatches) {
                        Write-PrintServiceEventTrace -EventSummary $summary -Phase 'scan' -MatchReason 'printer-mismatch-strict' -ResolvedDocument $resolvedDocument
                        continue
                    }
                }
                else {
                    if ($Owner -and $PrinterName -and -not $ownerMatches -and -not $printerMatches) {
                        Write-PrintServiceEventTrace -EventSummary $summary -Phase 'scan' -MatchReason 'owner-and-printer-mismatch-relaxed' -ResolvedDocument $resolvedDocument
                        continue
                    }
                }

                if ([string]::IsNullOrWhiteSpace($resolvedDocument)) {
                    Write-PrintServiceEventTrace -EventSummary $summary -Phase 'scan' -MatchReason ('no-document-candidate-' + $pass) -ResolvedDocument ''
                    continue
                }

                $matchReasonBase = if (Test-LooksLikeMojibakeQuestionMarks -Value $summary.DocumentName) { 'fallback-used' } else { 'direct' }
                Write-PrintServiceEventTrace -EventSummary $summary -Phase 'selected' -MatchReason ($matchReasonBase + '-' + $pass) -ResolvedDocument $resolvedDocument
                return $resolvedDocument
            }
        }
    }
    catch {
    }

    return $null
}

$deploymentConfig = Get-DeploymentConfig -Path $ConfigPath
$resolvedServerHost = if ($ServerHost) { $ServerHost } elseif ($deploymentConfig) { [string]$deploymentConfig.server.host } else { throw 'ServerHost is required.' }
$resolvedServerPort = if ($PSBoundParameters.ContainsKey('ServerPort')) { $ServerPort } elseif ($deploymentConfig) { [int]$deploymentConfig.server.port } else { 5600 }
$resolvedServerScheme = if ($ServerScheme) { $ServerScheme } elseif ($deploymentConfig) { [string]$deploymentConfig.server.scheme } else { 'http' }
$resolvedPolicyPath = if ($PolicyPath) { $PolicyPath } elseif ($deploymentConfig -and $deploymentConfig.paths.PSObject.Properties.Name -contains 'policyPath') { [string]$deploymentConfig.paths.policyPath } else { 'C:\ProgramData\AWatch-rus\dlp-policy.json' }
$resolvedPollSeconds = if ($PSBoundParameters.ContainsKey('PollSeconds')) { $PollSeconds } elseif ($deploymentConfig) { [int]$deploymentConfig.collector.pollSeconds } else { 5 }
$resolvedLogsRoot = if ($deploymentConfig) { [string]$deploymentConfig.paths.logsRoot } else { 'C:\ProgramData\AWatch-rus\logs' }
$resolvedLogPath = if ($LogPath) { $LogPath } else { Join-Path $resolvedLogsRoot ("endpoint-signals-{0}.log" -f $env:USERNAME) }
$resolvedLocalAgentLogsEnabled = if ($deploymentConfig -and $deploymentConfig.PSObject.Properties.Name -contains 'logging' -and $deploymentConfig.logging.PSObject.Properties.Name -contains 'localAgentLogsEnabled') { [bool]$deploymentConfig.logging.localAgentLogsEnabled } else { $true }
$resolvedIncidentArtifactsRoot = if ($deploymentConfig -and $deploymentConfig.PSObject.Properties.Name -contains 'incidentCapture' -and $deploymentConfig.incidentCapture.PSObject.Properties.Name -contains 'artifactsRoot') { [string]$deploymentConfig.incidentCapture.artifactsRoot } else { Join-Path $env:LOCALAPPDATA 'AWatch-rus\\incident-artifacts' }
$resolvedIncidentScreenshotEnabled = if ($deploymentConfig -and $deploymentConfig.PSObject.Properties.Name -contains 'incidentCapture' -and $deploymentConfig.incidentCapture.PSObject.Properties.Name -contains 'screenshotEnabled') { [bool]$deploymentConfig.incidentCapture.screenshotEnabled } else { $true }

if ($resolvedLocalAgentLogsEnabled -and -not (Test-Path -LiteralPath $resolvedLogsRoot)) {
    New-Item -Path $resolvedLogsRoot -ItemType Directory -Force | Out-Null
}

$script:ApiBase = '{0}://{1}:{2}/api/0' -f $resolvedServerScheme, $resolvedServerHost, $resolvedServerPort
$script:Hostname = $env:COMPUTERNAME
$script:SessionId = (Get-Process -Id $PID).SessionId
$script:KnownBuckets = @{}
$script:Cooldown = @{}
$script:SeenUsb = @{}
$script:SeenPrintJob = @{}
$script:SeenPrintEvent = @{}
$script:LastClipboardHash = $null
$script:PulseSeconds = [Math]::Max($resolvedPollSeconds * 3, 30)
$script:SelfTestIntervalSeconds = [Math]::Max($resolvedPollSeconds * 10, 60)
$script:LastSelfTestAt = [datetime]::MinValue
$script:LocalAgentLogsEnabled = $resolvedLocalAgentLogsEnabled
$script:LogPath = $resolvedLogPath
$script:IncidentArtifactsRoot = $resolvedIncidentArtifactsRoot
$script:IncidentScreenshotEnabled = $resolvedIncidentScreenshotEnabled
$script:ScreenshotTypesLoaded = $false

Load-DlpPolicy -Path $resolvedPolicyPath
Write-EndpointLog ("endpoint collector started against {0}" -f $script:ApiBase)

while ($true) {
    try {
        $nowUtc = (Get-Date).ToUniversalTime()
        if (($nowUtc - $script:LastSelfTestAt).TotalSeconds -ge $script:SelfTestIntervalSeconds) {
            Send-EndpointSignalHeartbeat -SignalType 'self_test' -Data @{
                collector = 'dlp-endpoint-signals'
                policyEnabled = [bool]$script:Policy.defaults.enabled
            }
            $script:LastSelfTestAt = $nowUtc
        }

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
                $documentNameOriginal = $documentName

                if (Test-LooksLikeMojibakeQuestionMarks -Value $documentName) {
                    $eventDocumentName = Get-BetterDocumentNameFromPrintServiceEvents -Owner $owner -PrinterName $printerName
                    if ($eventDocumentName) {
                        $documentName = $eventDocumentName
                    }
                }

                Send-EndpointSignalHeartbeat -SignalType 'print_job' -Data @{
                    printerName  = $printerName
                    documentName = $documentName
                    documentNameOriginal = $documentNameOriginal
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

        try {
            $printEvents = Get-WinEvent -FilterHashtable @{
                LogName = 'Microsoft-Windows-PrintService/Operational'
                Id = 307
                StartTime = (Get-Date).AddMinutes(-20)
            } -MaxEvents 200 -ErrorAction SilentlyContinue

            foreach ($event in @($printEvents)) {
                $recordId = [string]$event.RecordId
                if (-not $recordId) { continue }
                if ($script:SeenPrintEvent.ContainsKey($recordId)) { continue }
                $script:SeenPrintEvent[$recordId] = (Get-Date).ToUniversalTime()

                $summary = Get-PrintServiceEventSummary -Event $event
                $documentName = [string]$summary.DocumentName
                $owner = [string]$summary.Owner
                $printerName = [string]$summary.PrinterName
                $resolvedDocument = Get-PrintServiceDocumentFallback -EventSummary $summary -Owner $owner -PrinterName $printerName

                Write-PrintServiceEventTrace -EventSummary $summary -Phase 'emit' -MatchReason 'raw-scan' -ResolvedDocument $resolvedDocument

                if (-not [string]::IsNullOrWhiteSpace($owner) -and $owner -notlike "*$env:USERNAME*") {
                    continue
                }

                Send-EndpointSignalHeartbeat -SignalType 'print_job' -Data @{
                    printerName  = $printerName
                    documentName = if ($resolvedDocument) { $resolvedDocument } else { $documentName }
                    documentNameOriginal = $documentName
                    owner        = $owner
                    eventRecordId = $recordId
                    eventSource = 'printservice-307'
                }
                Evaluate-PrintRules -PrinterName $printerName -DocumentName (if ($resolvedDocument) { $resolvedDocument } else { $documentName }) -Owner $owner
            }

            $cleanupBeforeEvent = (Get-Date).ToUniversalTime().AddHours(-8)
            foreach ($k in @($script:SeenPrintEvent.Keys)) {
                $ts = [datetime]$script:SeenPrintEvent[$k]
                if ($ts -lt $cleanupBeforeEvent) {
                    $script:SeenPrintEvent.Remove($k)
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
