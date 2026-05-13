[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\AWatch-rus\deployment-config.json',
    [string]$ServerHost,
    [int]$ServerPort,
    [ValidateSet('http', 'https')]
    [string]$ServerScheme,
    [string]$PolicyEngineHost,
    [int]$PolicyEnginePort,
    [ValidateSet('http', 'https')]
    [string]$PolicyEngineScheme,
    [string]$PolicyPath,
    [ValidateSet('local', 'server')]
    [string]$PolicyMode,
    [int]$PolicyRefreshSeconds,
    [string]$PolicyCachePath,
    [string]$LogPath,
    [int]$PollSeconds
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Ensure HttpClient is available (Windows PowerShell 5 may not auto-load it)
try {
    Add-Type -AssemblyName System.Net.Http
}
catch {
}

$script:TransportQueuePath = $null
$script:TransportQueueLockPath = $null
$script:TransportMetrics = @{
    eventsEnqueued = 0
    eventsFlushed  = 0
    sendFailures   = 0
    queueDepth     = 0
}

$policyClientModulePath = Join-Path $PSScriptRoot 'dlp-policy-client.ps1'
if (Test-Path -LiteralPath $policyClientModulePath) {
    try {
        Import-Module $policyClientModulePath -Force -DisableNameChecking
        $script:PolicyClientAvailable = $true
    }
    catch {
        $script:PolicyClientAvailable = $false
    }
}
else {
    $script:PolicyClientAvailable = $false
}

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

    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Json)
        $req = [System.Net.HttpWebRequest]::Create($Uri)
        $req.Method = 'POST'
        $req.ContentType = 'application/json'
        $req.Accept = 'application/json'
        $req.KeepAlive = $false
        $req.Timeout = 15000
        $req.ReadWriteTimeout = 15000
        $req.ContentLength = $bytes.Length

        $stream = $req.GetRequestStream()
        try { $stream.Write($bytes, 0, $bytes.Length) } finally { $stream.Close() }

        $resp = $req.GetResponse()
        try {
            # read body for debugging, but discard on success
            $rs = $resp.GetResponseStream()
            if ($rs) { $sr = New-Object System.IO.StreamReader($rs); $null = $sr.ReadToEnd(); $sr.Close() }
        } finally {
            $resp.Close()
        }
        return
    }
    catch [System.Net.WebException] {
        $status = $null
        $body = ''
        try {
            if ($_.Exception.Response) {
                try { $status = [int]$_.Exception.Response.StatusCode } catch {}
                $rs = $_.Exception.Response.GetResponseStream()
                if ($rs) { $sr = New-Object System.IO.StreamReader($rs); $body = $sr.ReadToEnd(); $sr.Close() }
            }
        } catch {}

        # aw-server-rust may return 304 for idempotent bucket create. Treat it as OK.
        if ($status -eq 304) {
            Write-EndpointLog ("POST bucket exists (304): uri={0}" -f $Uri)
            return
        }

        Write-EndpointLog ("POST failed: uri={0} status={1} err={2} body={3}" -f $Uri, $status, $_.Exception.Message, $body)
        throw
    }
    catch {
        Write-EndpointLog ("POST error: uri={0} err={1}" -f $Uri, $_.Exception.Message)
        throw
    }
}

function Initialize-TransportQueue {
    param([Parameter(Mandatory = $true)][string]$StateRoot)
    $script:TransportQueuePath = Join-Path $StateRoot 'dlp-endpoint-signals-queue.jsonl'
    $script:TransportQueueLockPath = Join-Path $StateRoot 'dlp-endpoint-signals-queue.lock'
    if (-not (Test-Path -LiteralPath $script:TransportQueuePath)) {
        New-Item -Path $script:TransportQueuePath -ItemType File -Force | Out-Null
    }
}

function Get-TransportQueueLock {
    $tries = 0
    while ($tries -lt 50) {
        try {
            return [System.IO.File]::Open($script:TransportQueueLockPath, [System.IO.FileMode]::OpenOrCreate, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None)
        }
        catch {
            Start-Sleep -Milliseconds 50
            $tries++
        }
    }
    throw "Failed to acquire transport queue lock: $script:TransportQueueLockPath"
}

function Add-TransportQueueItem {
    param(
        [Parameter(Mandatory = $true)][string]$Uri,
        [Parameter(Mandatory = $true)][string]$Payload,
        [string]$Kind = 'endpoint'
    )
    $lock = Get-TransportQueueLock
    try {
        $line = @{
            ts      = (Get-Date).ToUniversalTime().ToString('o')
            uri     = $Uri
            payload = $Payload
            kind    = $Kind
        } | ConvertTo-Json -Compress
        Add-Content -LiteralPath $script:TransportQueuePath -Value $line -Encoding UTF8
        $script:TransportMetrics.eventsEnqueued++
    }
    finally {
        $lock.Dispose()
    }
}

function Read-TransportQueueItems {
    if (-not (Test-Path -LiteralPath $script:TransportQueuePath)) { return @() }
    $items = @()
    foreach ($line in @(Get-Content -LiteralPath $script:TransportQueuePath -ErrorAction SilentlyContinue)) {
        if ([string]::IsNullOrWhiteSpace($line)) { continue }
        try { $items += ($line | ConvertFrom-Json) } catch {}
    }
    return $items
}

function Flush-TransportQueue {
    param([int]$MaxItems = 200)
    if (-not (Test-Path -LiteralPath $script:TransportQueuePath)) { return }
    $lock = Get-TransportQueueLock
    try {
        $items = Read-TransportQueueItems
        $script:TransportMetrics.queueDepth = $items.Count
        if ($items.Count -eq 0) { return }
        $left = New-Object System.Collections.Generic.List[object]
        $sent = 0
        foreach ($item in $items) {
            if ($sent -ge $MaxItems) {
                $left.Add($item)
                continue
            }
            try {
                Invoke-AwJsonPost -Uri ([string]$item.uri) -Json ([string]$item.payload)
                $sent++
                $script:TransportMetrics.eventsFlushed++
            }
            catch {
                $script:TransportMetrics.sendFailures++
                $left.Add($item)
            }
        }
        foreach ($item in $items | Select-Object -Skip ($sent + $left.Count)) {
            $left.Add($item)
        }
        $lines = @($left | ForEach-Object { $_ | ConvertTo-Json -Compress })
        Set-Content -LiteralPath $script:TransportQueuePath -Value $lines -Encoding UTF8
        $script:TransportMetrics.queueDepth = $left.Count
    }
    finally {
        $lock.Dispose()
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

    if ($script:KnownBuckets.ContainsKey($BucketId)) {
        return
    }

    # Fast-path: if bucket already exists, don't POST.
    try {
        Invoke-RestMethod -Method Get -Uri "$($script:ApiBase)/buckets/$BucketId" -TimeoutSec 10 -DisableKeepAlive -ErrorAction Stop | Out-Null
        Write-EndpointLog ("bucket ok (GET): {0}" -f $BucketId)
        $script:KnownBuckets[$BucketId] = $true
        return
    }
    catch {
        Write-EndpointLog ("bucket GET failed: {0} err={1}" -f $BucketId, $_.Exception.Message)
    }

    $body = @{
        client   = $ClientName
        type     = $BucketType
        hostname = $script:Hostname
    } | ConvertTo-Json -Compress

    try {
        Invoke-AwJsonPost -Uri "$($script:ApiBase)/buckets/$BucketId" -Json $body
    }
    catch {
        # If create failed (race), verify it exists now.
        try {
            Invoke-RestMethod -Method Get -Uri "$($script:ApiBase)/buckets/$BucketId" -TimeoutSec 10 -DisableKeepAlive | Out-Null
        }
        catch {
            throw
        }
    }
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

    Add-TransportQueueItem -Uri "$($script:ApiBase)/buckets/$bucketId/heartbeat?pulsetime=$script:PulseSeconds" -Payload $payload -Kind 'endpoint_signal'
    Flush-TransportQueue -MaxItems 50
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

    Add-TransportQueueItem -Uri "$($script:ApiBase)/buckets/$bucketId/heartbeat?pulsetime=$script:PulseSeconds" -Payload $payload -Kind 'dlp_incident'
    Flush-TransportQueue -MaxItems 100
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

function Get-ClipboardTextSafe {
    [OutputType([string])]
    param()

    try {
        $v = Get-Clipboard -Raw -ErrorAction Stop
        if ($null -ne $v) { return [string]$v }
    }
    catch {
        Write-EndpointLog ("clipboard direct read failed: {0}" -f $_.Exception.Message)
    }

    # Clipboard is not reliably accessible from Session 0 (SYSTEM). Avoid noisy thread hacks there.
    if ($script:SessionId -eq 0) {
        return $null
    }

    # Fallback: read clipboard in a dedicated STA thread for RDP/user-session edge cases.
    try {
        Add-Type -AssemblyName System.Windows.Forms -ErrorAction SilentlyContinue | Out-Null
        $result = [string]::Empty
        $script:__aw_clip = $null
        $threadStart = [System.Threading.ThreadStart]{
            try {
                $script:__aw_clip = [System.Windows.Forms.Clipboard]::GetText()
            }
            catch {
                $script:__aw_clip = $null
            }
        }
        $thread = New-Object System.Threading.Thread($threadStart)
        $thread.SetApartmentState([System.Threading.ApartmentState]::STA)
        $thread.Start()
        $thread.Join(3000) | Out-Null
        if ($thread.IsAlive) {
            try { $thread.Abort() } catch {}
        }
        $result = [string]$script:__aw_clip
        Remove-Variable -Name __aw_clip -Scope Script -ErrorAction SilentlyContinue
        return $result
    }
    catch {
        Write-EndpointLog ("clipboard STA read failed: {0}" -f $_.Exception.Message)
        return $null
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
        contentAnalysis = [ordered]@{
            dictionaryPack = $null
            regexPack = $null
            ocrEnabled = $false
        }
    }

    $script:PolicySource = 'defaults'
    $script:PolicyVersion = $null
    $script:PolicyChecksum = $null

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
            $props = @()
            try { $props = @($raw.endpoint.PSObject.Properties.Name) } catch { $props = @() }
            if ($props -contains 'clipboard' -and $raw.endpoint.clipboard) { $script:Policy.endpoint.clipboard = @($raw.endpoint.clipboard) }
            if ($props -contains 'usb' -and $raw.endpoint.usb) { $script:Policy.endpoint.usb = @($raw.endpoint.usb) }
            if ($props -contains 'print' -and $raw.endpoint.print) { $script:Policy.endpoint.print = @($raw.endpoint.print) }
        }

        if ($raw.contentAnalysis) {
            if ($raw.contentAnalysis.PSObject.Properties.Name -contains 'dictionaryPack' -and $raw.contentAnalysis.dictionaryPack) {
                $script:Policy.contentAnalysis.dictionaryPack = [string]$raw.contentAnalysis.dictionaryPack
            }
            if ($raw.contentAnalysis.PSObject.Properties.Name -contains 'regexPack' -and $raw.contentAnalysis.regexPack) {
                $script:Policy.contentAnalysis.regexPack = [string]$raw.contentAnalysis.regexPack
            }
            if ($raw.contentAnalysis.PSObject.Properties.Name -contains 'ocrEnabled') {
                $script:Policy.contentAnalysis.ocrEnabled = [bool]$raw.contentAnalysis.ocrEnabled
            }
        }
        $script:PolicySource = 'local'
    }
    catch {
        Write-EndpointLog ("policy parse failed: {0}" -f $_.Exception.Message)
    }
}

function Test-ValidInn {
    param([string]$Value)
    $digits = ($Value -replace '\D', '')
    if ($digits.Length -eq 10) {
        $coef = @(2, 4, 10, 3, 5, 9, 4, 6, 8)
        $sum = 0
        for ($i = 0; $i -lt 9; $i++) { $sum += ([int][string]$digits[$i]) * $coef[$i] }
        $chk = ($sum % 11) % 10
        return $chk -eq ([int][string]$digits[9])
    }
    if ($digits.Length -eq 12) {
        $c11 = @(7, 2, 4, 10, 3, 5, 9, 4, 6, 8)
        $c12 = @(3, 7, 2, 4, 10, 3, 5, 9, 4, 6, 8)
        $sum11 = 0
        for ($i = 0; $i -lt 10; $i++) { $sum11 += ([int][string]$digits[$i]) * $c11[$i] }
        $sum12 = 0
        for ($i = 0; $i -lt 11; $i++) { $sum12 += ([int][string]$digits[$i]) * $c12[$i] }
        return ((($sum11 % 11) % 10) -eq ([int][string]$digits[10])) -and ((($sum12 % 11) % 10) -eq ([int][string]$digits[11]))
    }
    return $false
}

function Test-ValidSnils {
    param([string]$Value)
    $digits = ($Value -replace '\D', '')
    if ($digits.Length -ne 11) { return $false }
    $num = $digits.Substring(0, 9)
    $checksum = [int]$digits.Substring(9, 2)
    $sum = 0
    for ($i = 0; $i -lt 9; $i++) { $sum += ([int][string]$num[$i]) * (9 - $i) }
    if ($sum -lt 100) { $expected = $sum }
    elseif ($sum -eq 100 -or $sum -eq 101) { $expected = 0 }
    else {
        $expected = $sum % 101
        if ($expected -eq 100) { $expected = 0 }
    }
    return $checksum -eq $expected
}

function Test-ValidPassport {
    param([string]$Value)
    $digits = ($Value -replace '\D', '')
    if ($digits.Length -ne 10) { return $false }
    if ($digits -eq '0000000000') { return $false }
    return ($digits.ToCharArray() | Select-Object -Unique).Count -gt 1
}

function Get-AdvancedContentMatches {
    param(
        [string]$Text,
        [string]$DictionaryPack,
        [string]$RegexPack
    )

    $result = @{
        dictionaryMatches = @()
        regexMatches = @()
    }
    if ([string]::IsNullOrWhiteSpace($Text)) { return $result }

    if ($DictionaryPack -eq '152-fz-pdn') {
        $m = [regex]::Matches($Text, '\b\d{10}\b|\b\d{12}\b')
        foreach ($item in $m) {
            if (Test-ValidInn -Value $item.Value) {
                $result.dictionaryMatches += @{ name = 'inn'; value = $item.Value; severity = 'high' }
            }
        }
        $m = [regex]::Matches($Text, '\b\d{3}-\d{3}-\d{3}\s?\d{2}\b')
        foreach ($item in $m) {
            if (Test-ValidSnils -Value $item.Value) {
                $result.dictionaryMatches += @{ name = 'snils'; value = $item.Value; severity = 'high' }
            }
        }
        $m = [regex]::Matches($Text, '\b\d{4}\s?\d{6}\b')
        foreach ($item in $m) {
            if (Test-ValidPassport -Value $item.Value) {
                $result.dictionaryMatches += @{ name = 'passport'; value = $item.Value; severity = 'high' }
            }
        }
    }

    $regexRules = @()
    switch ($RegexPack) {
        'financial' {
            $regexRules = @(
                @{ id = 'card-pan'; regex = '\b(?:\d[ -]*?){13,19}\b'; severity = 'high' },
                @{ id = 'iban'; regex = '\b[A-Z]{2}\d{2}[A-Z0-9]{11,30}\b'; severity = 'medium' }
            )
        }
        'contacts' {
            $regexRules = @(
                @{ id = 'email'; regex = '[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}'; severity = 'low' },
                @{ id = 'phone-ru'; regex = '(?:\+7|8)\s*\(?\d{3}\)?\s*\d{3}[- ]?\d{2}[- ]?\d{2}'; severity = 'low' }
            )
        }
        'secrets' {
            $regexRules = @(
                @{ id = 'aws-access-key'; regex = 'AKIA[0-9A-Z]{16}'; severity = 'high' },
                @{ id = 'generic-password'; regex = '(?i)(password|пароль)\s*[:=]\s*\S{6,}'; severity = 'medium' }
            )
        }
    }
    foreach ($rule in $regexRules) {
        $m = [regex]::Matches($Text, [string]$rule.regex)
        foreach ($item in $m) {
            $result.regexMatches += @{ name = [string]$rule.id; value = $item.Value; severity = [string]$rule.severity }
        }
    }

    return $result
}

function Apply-PolicyFromBundle {
    param(
        [Parameter(Mandatory = $true)]$Bundle,
        [Parameter(Mandatory = $true)][string]$Source
    )

    if (-not $Bundle.policy) {
        throw 'Policy bundle has no policy payload.'
    }

    $tempPath = [System.IO.Path]::GetTempFileName()
    try {
        $Bundle.policy | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $tempPath -Encoding UTF8
        Load-DlpPolicy -Path $tempPath
        $script:PolicySource = $Source
        $script:PolicyVersion = if ($Bundle.PSObject.Properties.Name -contains 'version') { [string]$Bundle.version } else { $null }
        $script:PolicyChecksum = if ($Bundle.PSObject.Properties.Name -contains 'checksum') { [string]$Bundle.checksum } else { $null }
    }
    finally {
        Remove-Item -LiteralPath $tempPath -Force -ErrorAction SilentlyContinue
    }
}

function Refresh-DlpPolicyFromServer {
    if (-not $script:PolicyEngineEnabled) {
        return $false
    }
    if (-not $script:PolicyClientAvailable) {
        Write-EndpointLog 'policy client module unavailable, cannot use server mode'
        return $false
    }

    try {
        $bundle = Get-RemoteDlpPolicyBundle -ApiBase $script:PolicyApiBase -TimeoutSec 10
        Save-CachedDlpPolicyBundle -Bundle $bundle -CachePath $script:PolicyCachePath
        Apply-PolicyFromBundle -Bundle $bundle -Source 'server'
        $script:LastPolicyRefreshAt = (Get-Date).ToUniversalTime()
        Write-EndpointLog ("policy refreshed from server version={0} checksum={1}" -f $script:PolicyVersion, $script:PolicyChecksum)
        return $true
    }
    catch {
        Write-EndpointLog ("policy refresh failed: {0}" -f $_.Exception.Message)
        return $false
    }
}

function Sync-DlpPolicyDesiredState {
    if (-not $script:PolicyEngineEnabled -or -not $script:PolicyClientAvailable) {
        return $false
    }
    if (-not $script:PolicyAgentId) {
        return $false
    }

    try {
        [void](Send-DlpPolicyAgentHeartbeat -ApiBase $script:PolicyApiBase -AgentId $script:PolicyAgentId -Hostname $script:Hostname -Version $script:PolicyVersion -Checksum $script:PolicyChecksum -TimeoutSec 10)
        $desired = Get-RemoteDlpPolicyDesired -ApiBase $script:PolicyApiBase -AgentId $script:PolicyAgentId -TimeoutSec 10
        if ($desired -and $desired.refreshNow -eq $true) {
            Write-EndpointLog ("policy desired refresh requested: reason={0}" -f $desired.reason)
            return (Refresh-DlpPolicyFromServer)
        }
        return $true
    }
    catch {
        Write-EndpointLog ("policy desired sync failed: {0}" -f $_.Exception.Message)
        return $false
    }
}

function Initialize-DlpPolicy {
    if ($script:PolicyMode -eq 'server') {
        if (Refresh-DlpPolicyFromServer) {
            return
        }

        if ($script:PolicyClientAvailable) {
            $cached = Read-CachedDlpPolicyBundle -CachePath $script:PolicyCachePath
            if ($cached) {
                try {
                    Apply-PolicyFromBundle -Bundle $cached -Source 'cache'
                    Write-EndpointLog ("policy loaded from cache version={0} checksum={1}" -f $script:PolicyVersion, $script:PolicyChecksum)
                    return
                }
                catch {
                    Write-EndpointLog ("cached policy load failed: {0}" -f $_.Exception.Message)
                }
            }
        }

        Load-DlpPolicy -Path $script:LocalPolicyPath
        $script:PolicySource = 'local-fallback'
        return
    }

    Load-DlpPolicy -Path $script:LocalPolicyPath
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
        $dictionaryPack = if ($rule.dictionaryPack) { [string]$rule.dictionaryPack } elseif ($script:Policy.contentAnalysis.dictionaryPack) { [string]$script:Policy.contentAnalysis.dictionaryPack } else { $null }
        $regexPack = if ($rule.regexPack) { [string]$rule.regexPack } elseif ($script:Policy.contentAnalysis.regexPack) { [string]$script:Policy.contentAnalysis.regexPack } else { $null }
        $ocrEnabled = if ($rule.PSObject.Properties.Name -contains 'ocrEnabled') { [bool]$rule.ocrEnabled } else { [bool]$script:Policy.contentAnalysis.ocrEnabled }
        if ($ClipboardText.Length -lt $minLength) { continue }

        $matched = $false
        foreach ($pattern in $regexPatterns) {
            if ($ClipboardText -match [string]$pattern) {
                $matched = $true
                break
            }
        }
        $advanced = Get-AdvancedContentMatches -Text $ClipboardText -DictionaryPack $dictionaryPack -RegexPack $regexPack
        $advancedMatched = (@($advanced.dictionaryMatches).Count -gt 0) -or (@($advanced.regexMatches).Count -gt 0)
        if ($advancedMatched) { $matched = $true }

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
            dictionaryPack = $dictionaryPack
            regexPack = $regexPack
            dictionaryMatches = @($advanced.dictionaryMatches)
            regexMatches = @($advanced.regexMatches)
            ocrRequested = $ocrEnabled
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
        $dictionaryPack = if ($rule.dictionaryPack) { [string]$rule.dictionaryPack } elseif ($script:Policy.contentAnalysis.dictionaryPack) { [string]$script:Policy.contentAnalysis.dictionaryPack } else { $null }
        $regexPack = if ($rule.regexPack) { [string]$rule.regexPack } elseif ($script:Policy.contentAnalysis.regexPack) { [string]$script:Policy.contentAnalysis.regexPack } else { $null }
        $ocrEnabled = if ($rule.PSObject.Properties.Name -contains 'ocrEnabled') { [bool]$rule.ocrEnabled } else { [bool]$script:Policy.contentAnalysis.ocrEnabled }
        $advanced = Get-AdvancedContentMatches -Text $DocumentName -DictionaryPack $dictionaryPack -RegexPack $regexPack
        $advancedMatched = (@($advanced.dictionaryMatches).Count -gt 0) -or (@($advanced.regexMatches).Count -gt 0)
        if ($advancedMatched) { $match = $true }
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
            dictionaryPack = $dictionaryPack
            regexPack = $regexPack
            dictionaryMatches = @($advanced.dictionaryMatches)
            regexMatches = @($advanced.regexMatches)
            ocrRequested = $ocrEnabled
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
$resolvedStateRoot = if ($deploymentConfig -and $deploymentConfig.paths.PSObject.Properties.Name -contains 'stateRoot') { [string]$deploymentConfig.paths.stateRoot } else { Split-Path -Path $resolvedPolicyPath -Parent }
$resolvedPollSeconds = if ($PSBoundParameters.ContainsKey('PollSeconds')) { $PollSeconds } elseif ($deploymentConfig) { [int]$deploymentConfig.collector.pollSeconds } else { 5 }
$resolvedLogsRoot = if ($deploymentConfig) { [string]$deploymentConfig.paths.logsRoot } else { 'C:\ProgramData\AWatch-rus\logs' }
$resolvedLogPath = if ($LogPath) { $LogPath } else { Join-Path $resolvedLogsRoot ("endpoint-signals-{0}.log" -f $env:USERNAME) }
$resolvedLocalAgentLogsEnabled = if ($deploymentConfig -and $deploymentConfig.PSObject.Properties.Name -contains 'logging' -and $deploymentConfig.logging.PSObject.Properties.Name -contains 'localAgentLogsEnabled') { [bool]$deploymentConfig.logging.localAgentLogsEnabled } else { $true }
$resolvedIncidentArtifactsRoot = if ($deploymentConfig -and $deploymentConfig.PSObject.Properties.Name -contains 'incidentCapture' -and $deploymentConfig.incidentCapture.PSObject.Properties.Name -contains 'artifactsRoot') { [string]$deploymentConfig.incidentCapture.artifactsRoot } else { Join-Path $env:LOCALAPPDATA 'AWatch-rus\\incident-artifacts' }
$resolvedIncidentScreenshotEnabled = if ($deploymentConfig -and $deploymentConfig.PSObject.Properties.Name -contains 'incidentCapture' -and $deploymentConfig.incidentCapture.PSObject.Properties.Name -contains 'screenshotEnabled') { [bool]$deploymentConfig.incidentCapture.screenshotEnabled } else { $true }
$resolvedHostname = if ($deploymentConfig -and $deploymentConfig.PSObject.Properties.Name -contains 'awHostname' -and -not [string]::IsNullOrWhiteSpace([string]$deploymentConfig.awHostname)) { [string]$deploymentConfig.awHostname } else { [string]$env:COMPUTERNAME }
$resolvedPolicyMode = if ($PolicyMode) { [string]$PolicyMode } elseif ($deploymentConfig -and $deploymentConfig.PSObject.Properties.Name -contains 'policyEngine' -and $deploymentConfig.policyEngine.PSObject.Properties.Name -contains 'mode') { [string]$deploymentConfig.policyEngine.mode } else { 'local' }
$resolvedPolicyEngineEnabled = if ($deploymentConfig -and $deploymentConfig.PSObject.Properties.Name -contains 'policyEngine' -and $deploymentConfig.policyEngine.PSObject.Properties.Name -contains 'enabled') { [bool]$deploymentConfig.policyEngine.enabled } else { $false }
$resolvedPolicyEngineHost = if ($PolicyEngineHost) { [string]$PolicyEngineHost } elseif ($deploymentConfig -and $deploymentConfig.PSObject.Properties.Name -contains 'policyEngine' -and $deploymentConfig.policyEngine.PSObject.Properties.Name -contains 'host') { [string]$deploymentConfig.policyEngine.host } else { $resolvedServerHost }
$resolvedPolicyEnginePort = if ($PSBoundParameters.ContainsKey('PolicyEnginePort')) { $PolicyEnginePort } elseif ($deploymentConfig -and $deploymentConfig.PSObject.Properties.Name -contains 'policyEngine' -and $deploymentConfig.policyEngine.PSObject.Properties.Name -contains 'port') { [int]$deploymentConfig.policyEngine.port } else { $resolvedServerPort }
$resolvedPolicyEngineScheme = if ($PolicyEngineScheme) { [string]$PolicyEngineScheme } elseif ($deploymentConfig -and $deploymentConfig.PSObject.Properties.Name -contains 'policyEngine' -and $deploymentConfig.policyEngine.PSObject.Properties.Name -contains 'scheme') { [string]$deploymentConfig.policyEngine.scheme } else { $resolvedServerScheme }
$resolvedPolicyRefreshSeconds = if ($PSBoundParameters.ContainsKey('PolicyRefreshSeconds')) { $PolicyRefreshSeconds } elseif ($deploymentConfig -and $deploymentConfig.PSObject.Properties.Name -contains 'policyEngine' -and $deploymentConfig.policyEngine.PSObject.Properties.Name -contains 'refreshSeconds') { [int]$deploymentConfig.policyEngine.refreshSeconds } else { 300 }
$resolvedPolicyCachePath = if ($PolicyCachePath) { [string]$PolicyCachePath } elseif ($deploymentConfig -and $deploymentConfig.PSObject.Properties.Name -contains 'policyEngine' -and $deploymentConfig.policyEngine.PSObject.Properties.Name -contains 'cachePath') { [string]$deploymentConfig.policyEngine.cachePath } else { Join-Path $resolvedStateRoot 'dlp-policy-cache.json' }

if ($resolvedLocalAgentLogsEnabled -and -not (Test-Path -LiteralPath $resolvedLogsRoot)) {
    New-Item -Path $resolvedLogsRoot -ItemType Directory -Force | Out-Null
}

$script:ApiBase = '{0}://{1}:{2}/api/0' -f $resolvedServerScheme, $resolvedServerHost, $resolvedServerPort
$script:PolicyApiBase = '{0}://{1}:{2}/api/0' -f $resolvedPolicyEngineScheme, $resolvedPolicyEngineHost, $resolvedPolicyEnginePort
$script:Hostname = $resolvedHostname
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
$script:PolicyMode = $resolvedPolicyMode
$script:PolicyEngineEnabled = $resolvedPolicyEngineEnabled
$script:PolicyRefreshSeconds = [Math]::Max($resolvedPolicyRefreshSeconds, 60)
$script:PolicyCachePath = $resolvedPolicyCachePath
$script:LocalPolicyPath = $resolvedPolicyPath
$script:LastPolicyRefreshAt = [datetime]::MinValue
$script:PolicyAgentId = $resolvedHostname
$script:TransportBackoffSeconds = 1
# Integration test flag (backward compatible - defaults to false)
$script:IntegrationTestEnabled = if ($deploymentConfig -and $deploymentConfig.PSObject.Properties.Name -contains 'integrationTestEnabled') { [bool]$deploymentConfig.integrationTestEnabled } else { $false }

# Integration metadata tracking (backward compatible)
$script:TotalEventsProcessed = 0
$script:LastEventTime = $null

Initialize-TransportQueue -StateRoot $resolvedStateRoot
Initialize-DlpPolicy
Write-EndpointLog ("endpoint collector started against {0}" -f $script:ApiBase)

while ($true) {
    try {
        try {
            Flush-TransportQueue -MaxItems 200
            $script:TransportBackoffSeconds = 1
        }
        catch {
            $script:TransportBackoffSeconds = [Math]::Min($script:TransportBackoffSeconds * 2, 60)
            Write-EndpointLog ("transport flush failed, backoff={0}s err={1}" -f $script:TransportBackoffSeconds, $_.Exception.Message)
        }

        if ($script:PolicyMode -eq 'server') {
            $policyAge = ((Get-Date).ToUniversalTime() - $script:LastPolicyRefreshAt).TotalSeconds
            if ($policyAge -ge $script:PolicyRefreshSeconds) {
                [void](Refresh-DlpPolicyFromServer)
            }
            else {
                [void](Sync-DlpPolicyDesiredState)
            }
        }

        $nowUtc = (Get-Date).ToUniversalTime()
        if (($nowUtc - $script:LastSelfTestAt).TotalSeconds -ge $script:SelfTestIntervalSeconds) {
            Send-EndpointSignalHeartbeat -SignalType 'self_test' -Data @{
                collector = 'dlp-endpoint-signals'
                policyEnabled = [bool]$script:Policy.defaults.enabled
                policyMode = $script:PolicyMode
                policySource = $script:PolicySource
                policyVersion = $script:PolicyVersion
                policyChecksum = $script:PolicyChecksum
                queueDepth = [int]$script:TransportMetrics.queueDepth
                eventsEnqueued = [int]$script:TransportMetrics.eventsEnqueued
                eventsFlushed = [int]$script:TransportMetrics.eventsFlushed
                sendFailures = [int]$script:TransportMetrics.sendFailures
            }
            $script:LastSelfTestAt = $nowUtc
        }

        if (-not $script:Policy.defaults.enabled) {
            Start-Sleep -Seconds $resolvedPollSeconds
            continue
        }

        try {
            $clipboardText = Get-ClipboardTextSafe
            if ($clipboardText) {
                $clipboardHash = Get-StringHash -Value $clipboardText
                if ($clipboardHash -and $clipboardHash -ne $script:LastClipboardHash) {
                    $script:LastClipboardHash = $clipboardHash
                    Send-EndpointSignalHeartbeat -SignalType 'clipboard_change' -Data @{
                        clipboardHash = $clipboardHash
                        clipboardLength = $clipboardText.Length
                    }
                    $script:TotalEventsProcessed++
                    $script:LastEventTime = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
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
                    $script:TotalEventsProcessed++
                    $script:LastEventTime = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
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
                $script:TotalEventsProcessed++
                $script:LastEventTime = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
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
                $script:TotalEventsProcessed++
                $script:LastEventTime = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
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

    # Integration metadata self-test (backward compatible)
    if ($script:IntegrationTestEnabled -and (Get-Date).Minute -eq 0) {
        try {
            $testMetadata = @{
                timestamp = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
                collector = 'dlp-endpoint-signals'
                version = '1.0.0'
                hostname = $env:COMPUTERNAME
                username = $env:USERNAME
                status = 'healthy'
                checks = @{
                    eventsProcessed = $script:TotalEventsProcessed
                    lastEventTime = $script:LastEventTime
                    iocRulesLoaded = if ($script:IocRules) { @($script:IocRules).Count } else { 0 }
                    policyRulesLoaded = if ($script:Policy -and $script:Policy.endpoint) { (@($script:Policy.endpoint.clipboard).Count + @($script:Policy.endpoint.usb).Count + @($script:Policy.endpoint.print).Count) } else { 0 }
                }
            }
            Send-EndpointSignalHeartbeat -SignalType 'integration_test' -Data $testMetadata
            Write-EndpointLog "Integration metadata test sent"
        }
        catch {
            Write-EndpointLog "Integration test failed: $($_.Exception.Message)"
        }
    }

    if ($script:TransportBackoffSeconds -gt $resolvedPollSeconds) {
        Start-Sleep -Seconds $script:TransportBackoffSeconds
    }
    else {
        Start-Sleep -Seconds $resolvedPollSeconds
    }
}
