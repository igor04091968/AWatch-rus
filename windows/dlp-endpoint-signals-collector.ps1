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

[System.Net.ServicePointManager]::SecurityProtocol = [System.Net.SecurityProtocolType]::Tls12
Add-Type -AssemblyName System.Net.Http

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

    $httpClient = New-Object System.Net.Http.HttpClient
    try {
        $content = New-Object System.Net.Http.StringContent($Json, [System.Text.Encoding]::UTF8, 'application/json')
        $response = $httpClient.PostAsync($Uri, $content).Result
        if (-not $response.IsSuccessStatusCode) {
            Write-EndpointLog ("POST {0} returned {1}" -f $Uri, [int]$response.StatusCode)
        }
    }
    catch {
        Write-EndpointLog ("POST error {0}: {1}" -f $Uri, $_.Exception.Message)
    }
    finally {
        $httpClient.Dispose()
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
            source     = 'endpoint-signals-awatch-rus'
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
            source    = 'endpoint-signals-awatch-rus'
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
        Write-EndpointLog ("не удалось сделать снимок инцидента: {0}" -f $_.Exception.Message)
        return @{}
    }
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
        Write-EndpointLog ("DLP-политика не найдена, используются значения по умолчанию: {0}" -f $Path)
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
        Write-EndpointLog ("не удалось разобрать DLP-политику: {0}" -f $_.Exception.Message)
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
        $message = if ($rule.message) { [string]$rule.message } else { "Сработало правило буфера обмена: $ruleId" }

        Send-DlpIncidentHeartbeat -RuleId $ruleId -Action $action -Severity $severity -Message $message -SignalType 'clipboard' -Data @{
            clipboardHash = $ClipboardHash
            clipboardLength = $ClipboardText.Length
        }
        Write-EndpointLog ("инцидент буфера обмена правило={0} действие={1} важность={2}" -f $ruleId, $action, $severity)
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
        $message = if ($rule.message) { [string]$rule.message } else { "Сработало правило USB-носителя: $ruleId" }

        Send-DlpIncidentHeartbeat -RuleId $ruleId -Action $action -Severity $severity -Message $message -SignalType 'usb_insert' -Data @{
            driveLetter = $DriveLetter
            volumeName  = $VolumeName
        }
        Write-EndpointLog ("инцидент USB правило={0} действие={1} важность={2} диск={3}" -f $ruleId, $action, $severity, $DriveLetter)
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
        $message = if ($rule.message) { [string]$rule.message } else { "Сработало правило печати: $ruleId" }

        Send-DlpIncidentHeartbeat -RuleId $ruleId -Action $action -Severity $severity -Message $message -SignalType 'print_job' -Data @{
            printerName  = $PrinterName
            documentName = $DocumentName
            owner        = $Owner
        }
        Write-EndpointLog ("инцидент печати правило={0} действие={1} важность={2} принтер={3}" -f $ruleId, $action, $severity, $PrinterName)
    }
}

function Test-LooksLikeMojibakeQuestionMarks {
    param([AllowNull()][string]$Value)
    if ([string]::IsNullOrWhiteSpace($Value)) { return $true }
    return $Value -match '\?{2,}'
}

function Test-DocumentNameNeedsFallback {
    param([AllowNull()][string]$Value)

    if ([string]::IsNullOrWhiteSpace($Value)) { return $true }
    $trimmed = $Value.Trim()
    if (Test-LooksLikeMojibakeQuestionMarks -Value $trimmed) { return $true }
    if ($trimmed -match '^[0-9]+$') { return $true }
    if ($trimmed -match '^(?i)(print document|document|local downlevel document|печать документа)$') { return $true }
    return $false
}

function Get-EventXmlValue {
    param(
        [Parameter(Mandatory = $true)][xml]$EventXml,
        [Parameter(Mandatory = $true)][string]$Name
    )

    $node = $EventXml.Event.UserData.DocumentPrinted.$Name
    if ($null -ne $node) {
        return [string]$node
    }

    return ''
}

function Get-PrintJobPrinterName {
    param(
        [AllowNull()][string]$JobName,
        [AllowNull()][string]$FallbackPrinterName
    )

    if ([string]::IsNullOrWhiteSpace($JobName)) {
        if (-not [string]::IsNullOrWhiteSpace($FallbackPrinterName)) {
            return $FallbackPrinterName.Trim()
        }
        return ''
    }

    $parts = $JobName -split ',', 2
    if ($parts.Count -gt 0 -and -not [string]::IsNullOrWhiteSpace($parts[0])) {
        return $parts[0].Trim()
    }

    return $JobName.Trim()
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

    $xml = $null
    try {
        $xml = [xml]$Event.ToXml()
    }
    catch {
    }

    $jobId = ''
    $documentName = ''
    $owner = ''
    $portName = ''
    $printerName = ''
    $sizeBytes = ''
    $pageCount = ''

    if ($xml) {
        $jobId = Get-EventXmlValue -EventXml $xml -Name 'Param1'
        $documentName = Get-EventXmlValue -EventXml $xml -Name 'Param2'
        $owner = Get-EventXmlValue -EventXml $xml -Name 'Param3'
        $portName = Get-EventXmlValue -EventXml $xml -Name 'Param4'
        $printerName = Get-EventXmlValue -EventXml $xml -Name 'Param5'
        $sizeBytes = Get-EventXmlValue -EventXml $xml -Name 'Param7'
        $pageCount = Get-EventXmlValue -EventXml $xml -Name 'Param8'
    }

    if ([string]::IsNullOrWhiteSpace($jobId) -and $props.Count -ge 1) { $jobId = [string]$props[0].Value }
    if ([string]::IsNullOrWhiteSpace($documentName) -and $props.Count -ge 2) { $documentName = [string]$props[1].Value }
    if ([string]::IsNullOrWhiteSpace($owner) -and $props.Count -ge 3) { $owner = [string]$props[2].Value }
    if ([string]::IsNullOrWhiteSpace($portName) -and $props.Count -ge 4) { $portName = [string]$props[3].Value }
    if ([string]::IsNullOrWhiteSpace($printerName) -and $props.Count -ge 5) { $printerName = [string]$props[4].Value }
    if ([string]::IsNullOrWhiteSpace($sizeBytes) -and $props.Count -ge 7) { $sizeBytes = [string]$props[6].Value }
    if ([string]::IsNullOrWhiteSpace($pageCount) -and $props.Count -ge 8) { $pageCount = [string]$props[7].Value }

    [pscustomobject]@{
        RecordId      = [string]$Event.RecordId
        TimeCreated   = if ($Event.TimeCreated) { $Event.TimeCreated.ToString('o') } else { '' }
        PropertyCount  = $props.Count
        JobId          = $jobId
        DocumentName   = $documentName
        Owner          = $owner
        PortName       = $portName
        PrinterName    = $printerName
        SizeBytes      = $sizeBytes
        PageCount      = $pageCount
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
    if (-not (Test-DocumentNameNeedsFallback -Value $preferred)) {
        return $preferred
    }

    $pathCandidates = New-Object System.Collections.Generic.List[string]
    $textCandidates = New-Object System.Collections.Generic.List[string]

    foreach ($value in @($EventSummary.PropertyValues)) {
        $candidate = [string]$value
        if ([string]::IsNullOrWhiteSpace($candidate)) { continue }
        if ($candidate -eq $preferred) { continue }
        if ($EventSummary.JobId -and $candidate -eq [string]$EventSummary.JobId) { continue }
        if ($Owner -and $candidate -like "*$Owner*") { continue }
        if ($PrinterName -and $candidate -like "*$PrinterName*") { continue }
        if (Test-DocumentNameNeedsFallback -Value $candidate) { continue }

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
        'printservice-307 этап={0} recordId={1} время={2} владелец={3} принтер={4} документ={5} итоговыйДокумент={6} свойства=[{7}] причина={8}' -f
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
        [string]$JobId,
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

                $jobMatches = if ($JobId) { [string]$summary.JobId -eq [string]$JobId } else { $true }
                $ownerMatches = if ($Owner) { Test-OwnerLooseMatch -Expected $Owner -Actual $summary.Owner } else { $true }
                $printerMatches = if ($PrinterName) { Test-PrinterLooseMatch -Expected $PrinterName -Actual $summary.PrinterName } else { $true }

                if ($pass -eq 'strict') {
                    if ($JobId -and -not $jobMatches) {
                        Write-PrintServiceEventTrace -EventSummary $summary -Phase 'scan' -MatchReason 'несовпадение-jobid-strict' -ResolvedDocument $resolvedDocument
                        continue
                    }
                    if ($Owner -and -not $ownerMatches) {
                        Write-PrintServiceEventTrace -EventSummary $summary -Phase 'scan' -MatchReason 'несовпадение-владельца-strict' -ResolvedDocument $resolvedDocument
                        continue
                    }
                    if ($PrinterName -and -not $printerMatches) {
                        Write-PrintServiceEventTrace -EventSummary $summary -Phase 'scan' -MatchReason 'несовпадение-принтера-strict' -ResolvedDocument $resolvedDocument
                        continue
                    }
                }
                else {
                    if ($JobId -and (-not $jobMatches) -and $Owner -and $PrinterName -and -not $ownerMatches -and -not $printerMatches) {
                        Write-PrintServiceEventTrace -EventSummary $summary -Phase 'scan' -MatchReason 'несовпадение-владельца-и-принтера-relaxed' -ResolvedDocument $resolvedDocument
                        continue
                    }
                    if ((-not $JobId) -and $Owner -and $PrinterName -and -not $ownerMatches -and -not $printerMatches) {
                        Write-PrintServiceEventTrace -EventSummary $summary -Phase 'scan' -MatchReason 'несовпадение-владельца-и-принтера-relaxed' -ResolvedDocument $resolvedDocument
                        continue
                    }
                }

                if ([string]::IsNullOrWhiteSpace($resolvedDocument)) {
                    Write-PrintServiceEventTrace -EventSummary $summary -Phase 'scan' -MatchReason ('нет-кандидата-документа-' + $pass) -ResolvedDocument ''
                    continue
                }

                $matchReasonBase = if (Test-DocumentNameNeedsFallback -Value $summary.DocumentName) { 'использован-резервный-вариант' } else { 'напрямую' }
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
$resolvedServerHost = if ($ServerHost) { $ServerHost } elseif ($deploymentConfig) { [string]$deploymentConfig.server.host } else { throw 'Укажите ServerHost или подготовьте deployment-config.json.' }
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
$script:LocalAgentLogsEnabled = $resolvedLocalAgentLogsEnabled
$script:LogPath = $resolvedLogPath
$script:IncidentArtifactsRoot = $resolvedIncidentArtifactsRoot
$script:IncidentScreenshotEnabled = $resolvedIncidentScreenshotEnabled
$script:ScreenshotTypesLoaded = $false

Load-DlpPolicy -Path $resolvedPolicyPath
Write-EndpointLog ("endpoint-коллектор запущен для {0}" -f $script:ApiBase)

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

                $printerName = Get-PrintJobPrinterName -JobName ([string]$job.Name) -FallbackPrinterName ([string]$job.DriverName)
                $documentName = [string]$job.Document
                $owner = [string]$job.Owner
                $documentNameOriginal = $documentName
                $documentNameSource = 'win32-printjob'

                if (Test-DocumentNameNeedsFallback -Value $documentName) {
                    $eventDocumentName = Get-BetterDocumentNameFromPrintServiceEvents -JobId $jobId -Owner $owner -PrinterName $printerName
                    if ($eventDocumentName) {
                        $documentName = $eventDocumentName
                        $documentNameSource = 'printservice-307-fallback'
                    }
                }

                Send-EndpointSignalHeartbeat -SignalType 'print_job' -Data @{
                    printerName  = $printerName
                    documentName = $documentName
                    documentNameOriginal = $documentNameOriginal
                    documentNameSource = $documentNameSource
                    owner        = $owner
                    printJobId   = $jobId
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
                    documentNameSource = if ($resolvedDocument) { 'printservice-307-fallback' } else { 'printservice-307' }
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
        Write-EndpointLog ("ошибка коллектора: {0}" -f $_.Exception.Message)
    }

    Start-Sleep -Seconds $resolvedPollSeconds
}
