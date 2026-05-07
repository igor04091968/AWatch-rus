<#
.SYNOPSIS
    DLP email outbound collector for AWatch-rus (Phase 2.5).
    Monitors outgoing email via Outlook COM Sent Items polling
    and/or SMTP network connection detection.

.DESCRIPTION
    Two collection modes (configurable, can run simultaneously):
      - outlook : Polls Outlook Sent Items via COM for new messages.
      - smtp    : Monitors SMTP connections (ports 25/587/465) via
                  Get-NetTCPConnection for any process sending mail.

    Sends heartbeats to AW bucket `aw-email-monitor_<host>`.
    Evaluates DLP policy rules from `endpoint.email[]` section.
    Supports enforcement: action="block" moves the email to Drafts
    (Outlook mode) or logs with enforced=false (SMTP mode).
#>
[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\ActivityWatch\deployment-config.json',
    [string]$ServerHost,
    [int]$ServerPort,
    [ValidateSet('http', 'https')]
    [string]$ServerScheme,
    [string]$PolicyPath,
    [string]$LogPath,
    [int]$PollSeconds,
    [ValidateSet('outlook', 'smtp', 'both')]
    [string]$Mode = 'both'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# ---------------------------------------------------------------------------
# Shared infrastructure (mirrors other collectors)
# ---------------------------------------------------------------------------

function Get-DeploymentConfig {
    param([string]$Path)
    if ($Path -and (Test-Path -LiteralPath $Path)) {
        return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    }
    return $null
}

function Write-CollectorLog {
    param([string]$Message)
    if (-not $script:LocalAgentLogsEnabled) { return }
    try {
        Add-Content -LiteralPath $script:LogPath -Value ('{0} {1}' -f (Get-Date -Format s), $Message)
    }
    catch { }
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
    if ($script:KnownBuckets.ContainsKey($BucketId)) { return }
    $body = @{
        client   = $ClientName
        type     = $BucketType
        hostname = $script:Hostname
    } | ConvertTo-Json -Compress
    Invoke-AwJsonPost -Uri "$($script:ApiBase)/buckets/$BucketId" -Json $body
    $script:KnownBuckets[$BucketId] = $true
}

function Get-StringHash {
    param([AllowNull()][string]$Value)
    if ($null -eq $Value) { return $null }
    $bytes = [Text.Encoding]::UTF8.GetBytes($Value)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        ($sha.ComputeHash($bytes) | ForEach-Object { $_.ToString('x2') }) -join ''
    }
    finally { $sha.Dispose() }
}

function Send-EmailHeartbeat {
    param(
        [string]$SignalType,
        [hashtable]$Data
    )
    $bucketId = 'aw-email-monitor_' + $script:Hostname
    Ensure-Bucket -BucketId $bucketId -ClientName 'aw-email-monitor' -BucketType 'aw.dlp.email'
    $payload = @{
        timestamp = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
        duration  = 0
        data      = @{
            signalType = $SignalType
            username   = $env:USERNAME
            sessionId  = $script:SessionId
            hostname   = $script:Hostname
            source     = 'email-outbound-collector'
        } + $Data
    } | ConvertTo-Json -Depth 6 -Compress
    Invoke-AwJsonPost -Uri "$($script:ApiBase)/buckets/$bucketId/heartbeat?pulsetime=$script:PulseSeconds" -Json $payload
}

function Send-EmailIncidentHeartbeat {
    param(
        [string]$RuleId,
        [string]$Action,
        [string]$Severity,
        [string]$Message,
        [hashtable]$Data
    )
    $bucketId = 'aw-dlp-incidents_' + $script:Hostname
    Ensure-Bucket -BucketId $bucketId -ClientName 'aw-dlp-incidents' -BucketType 'aw.dlp.incident'
    $payload = @{
        timestamp = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
        duration  = 0
        data      = @{
            ruleId     = $RuleId
            action     = $Action
            severity   = $Severity
            message    = $Message
            signalType = 'email_outbound'
            username   = $env:USERNAME
            sessionId  = $script:SessionId
            hostname   = $script:Hostname
            source     = 'email-outbound-collector'
        } + $Data
    } | ConvertTo-Json -Depth 7 -Compress
    Invoke-AwJsonPost -Uri "$($script:ApiBase)/buckets/$bucketId/heartbeat?pulsetime=$script:PulseSeconds" -Json $payload
}

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
    catch { }
}

# ---------------------------------------------------------------------------
# DLP Policy
# ---------------------------------------------------------------------------

function Load-EmailPolicy {
    param([string]$Path)

    $script:Policy = [ordered]@{
        defaults = [ordered]@{
            enabled         = $true
            cooldownSeconds = 300
            action          = 'alert'
            severity        = 'medium'
        }
        endpoint = [ordered]@{
            email = @()
        }
    }

    if (-not $Path -or -not (Test-Path -LiteralPath $Path)) {
        Write-CollectorLog ("policy not found, using defaults: {0}" -f $Path)
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
        if ($raw.endpoint -and $raw.endpoint.email) {
            $script:Policy.endpoint.email = @($raw.endpoint.email)
        }
    }
    catch {
        Write-CollectorLog ("policy parse failed: {0}" -f $_.Exception.Message)
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

# ---------------------------------------------------------------------------
# Email DLP rule evaluation
# ---------------------------------------------------------------------------

function Evaluate-EmailRules {
    param(
        [string]$Subject,
        [string]$RecipientsJoined,
        [string]$SenderAddress,
        [int]$AttachmentCount,
        [string]$AttachmentNames,
        [int]$BodyLength,
        [string]$MessageId,
        $OutlookMailItem
    )

    foreach ($rule in @($script:Policy.endpoint.email)) {
        if (-not $rule) { continue }
        if ($rule.PSObject.Properties.Name -contains 'enabled' -and -not [bool]$rule.enabled) { continue }
        $ruleId = [string]$rule.id
        if (-not $ruleId) { continue }

        $matched = $true

        if ($rule.subjectRegex) {
            $matched = $matched -and ($Subject -match [string]$rule.subjectRegex)
        }
        if ($rule.recipientRegex) {
            $matched = $matched -and ($RecipientsJoined -match [string]$rule.recipientRegex)
        }
        if ($rule.senderRegex) {
            $matched = $matched -and ($SenderAddress -match [string]$rule.senderRegex)
        }
        if ($rule.attachmentRegex -and $AttachmentNames) {
            $matched = $matched -and ($AttachmentNames -match [string]$rule.attachmentRegex)
        }
        if ($rule.minAttachments) {
            $matched = $matched -and ($AttachmentCount -ge [int]$rule.minAttachments)
        }
        if ($rule.minBodyLength) {
            $matched = $matched -and ($BodyLength -ge [int]$rule.minBodyLength)
        }
        if ($rule.externalOnly -and [bool]$rule.externalOnly) {
            $internalDomain = if ($rule.internalDomain) { [string]$rule.internalDomain } else { '' }
            if ($internalDomain -and $RecipientsJoined -notmatch [regex]::Escape($internalDomain)) {
                # all recipients are external — continue matching
            }
            elseif ($internalDomain) {
                $matched = $false
            }
        }

        if (-not $matched) { continue }

        $cooldown = if ($rule.cooldownSeconds) { [int]$rule.cooldownSeconds } else { [int]$script:Policy.defaults.cooldownSeconds }
        $fingerprint = "email|$ruleId|$MessageId|$env:USERNAME"
        if (-not (Should-EmitByCooldown -Fingerprint $fingerprint -CooldownSeconds ([Math]::Max($cooldown, 30)))) { continue }

        $action = if ($rule.action) { [string]$rule.action } else { [string]$script:Policy.defaults.action }
        $severity = if ($rule.severity) { [string]$rule.severity } else { [string]$script:Policy.defaults.severity }
        $message = if ($rule.message) { [string]$rule.message } else { "Email rule matched: $ruleId" }

        $enforced = $false
        if ($action -eq 'block' -and $null -ne $OutlookMailItem) {
            $enforced = Invoke-EmailEnforcement -MailItem $OutlookMailItem -RuleId $ruleId
            Show-EnforcementNotification -Title 'DLP: письмо перемещено в черновики' -Body $message
        }
        elseif ($action -eq 'block') {
            Show-EnforcementNotification -Title 'DLP: обнаружена отправка письма' -Body $message
        }

        Send-EmailIncidentHeartbeat -RuleId $ruleId -Action $action -Severity $severity -Message $message -Data @{
            subject         = (Get-StringHash -Value $Subject)
            recipients      = (Get-StringHash -Value $RecipientsJoined)
            sender          = $SenderAddress
            attachmentCount = $AttachmentCount
            attachmentNames = $AttachmentNames
            bodyLength      = $BodyLength
            enforced        = $enforced
        }
        Write-CollectorLog ("incident email rule={0} action={1} severity={2} enforced={3} subject_hash={4}" -f $ruleId, $action, $severity, $enforced, (Get-StringHash -Value $Subject))
    }
}

function Invoke-EmailEnforcement {
    [OutputType([bool])]
    param(
        [Parameter(Mandatory = $true)]$MailItem,
        [string]$RuleId
    )
    try {
        $draftsFolder = $script:OutlookNamespace.GetDefaultFolder(16) # olFolderDrafts
        $MailItem.Move($draftsFolder) | Out-Null
        Write-CollectorLog ("enforcement: email moved to Drafts rule={0} subject_hash={1}" -f $RuleId, (Get-StringHash -Value $MailItem.Subject))
        return $true
    }
    catch {
        Write-CollectorLog ("enforcement: email move to Drafts failed rule={0}: {1}" -f $RuleId, $_.Exception.Message)
        return $false
    }
}

# ---------------------------------------------------------------------------
# Outlook Sent Items polling
# ---------------------------------------------------------------------------

function Initialize-OutlookCom {
    try {
        $script:OutlookApp = New-Object -ComObject Outlook.Application
        $script:OutlookNamespace = $script:OutlookApp.GetNamespace('MAPI')
        $script:SentFolder = $script:OutlookNamespace.GetDefaultFolder(5) # olFolderSentMail
        Write-CollectorLog "Outlook COM initialized, Sent Items folder opened"
        return $true
    }
    catch {
        Write-CollectorLog ("Outlook COM init failed: {0}" -f $_.Exception.Message)
        return $false
    }
}

function Get-OutlookSentItems {
    param([datetime]$Since)

    $results = @()
    try {
        $items = $script:SentFolder.Items
        $items.Sort('[SentOn]', $true)

        $filter = "[SentOn] >= '{0}'" -f $Since.ToString('MM/dd/yyyy HH:mm')
        $restricted = $items.Restrict($filter)

        foreach ($item in $restricted) {
            try {
                if ($item.Class -ne 43) { continue } # olMail = 43

                $recipients = @()
                for ($i = 1; $i -le $item.Recipients.Count; $i++) {
                    $recip = $item.Recipients.Item($i)
                    $recipients += [string]$recip.Address
                }

                $attachmentNames = @()
                for ($i = 1; $i -le $item.Attachments.Count; $i++) {
                    $attachmentNames += [string]$item.Attachments.Item($i).FileName
                }

                $results += [pscustomobject]@{
                    EntryID        = [string]$item.EntryID
                    Subject        = [string]$item.Subject
                    SenderAddress  = [string]$item.SenderEmailAddress
                    SenderName     = [string]$item.SenderName
                    Recipients     = $recipients
                    RecipientsJoined = ($recipients -join '; ')
                    AttachmentCount = [int]$item.Attachments.Count
                    AttachmentNames = ($attachmentNames -join '; ')
                    BodyLength     = if ($item.Body) { $item.Body.Length } else { 0 }
                    SentOn         = $item.SentOn
                    MailItem       = $item
                }
            }
            catch { }
        }
    }
    catch {
        Write-CollectorLog ("Outlook Sent Items scan failed: {0}" -f $_.Exception.Message)
    }
    return $results
}

function Poll-OutlookSentItems {
    $items = Get-OutlookSentItems -Since $script:OutlookLastPoll

    foreach ($item in $items) {
        $entryId = $item.EntryID
        if ($script:SeenEntryIds.ContainsKey($entryId)) { continue }
        $script:SeenEntryIds[$entryId] = (Get-Date).ToUniversalTime()

        $subjectHash = Get-StringHash -Value $item.Subject

        Send-EmailHeartbeat -SignalType 'email_sent' -Data @{
            subject         = $subjectHash
            sender          = [string]$item.SenderAddress
            senderName      = [string]$item.SenderName
            recipientCount  = $item.Recipients.Count
            recipients      = (Get-StringHash -Value $item.RecipientsJoined)
            attachmentCount = [int]$item.AttachmentCount
            attachmentNames = [string]$item.AttachmentNames
            bodyLength      = [int]$item.BodyLength
            sentOn          = if ($item.SentOn) { $item.SentOn.ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ') } else { '' }
            collectionMode  = 'outlook'
        }
        Write-CollectorLog ("email_sent outlook subject_hash={0} to={1} attachments={2}" -f $subjectHash, $item.Recipients.Count, $item.AttachmentCount)

        Evaluate-EmailRules `
            -Subject $item.Subject `
            -RecipientsJoined $item.RecipientsJoined `
            -SenderAddress $item.SenderAddress `
            -AttachmentCount $item.AttachmentCount `
            -AttachmentNames $item.AttachmentNames `
            -BodyLength $item.BodyLength `
            -MessageId $entryId `
            -OutlookMailItem $item.MailItem
    }

    $script:OutlookLastPoll = (Get-Date).AddSeconds(-10)

    # Cleanup old entry IDs (keep last 24h)
    $cleanupBefore = (Get-Date).ToUniversalTime().AddHours(-24)
    foreach ($k in @($script:SeenEntryIds.Keys)) {
        if ([datetime]$script:SeenEntryIds[$k] -lt $cleanupBefore) {
            $script:SeenEntryIds.Remove($k)
        }
    }
}

# ---------------------------------------------------------------------------
# SMTP network connection monitoring
# ---------------------------------------------------------------------------

function Poll-SmtpConnections {
    try {
        $smtpPorts = @(25, 587, 465, 2525)
        $connections = Get-NetTCPConnection -State Established -ErrorAction SilentlyContinue |
            Where-Object { $smtpPorts -contains $_.RemotePort }

        foreach ($conn in @($connections)) {
            $processId = [int]$conn.OwningProcess
            $remoteAddr = [string]$conn.RemoteAddress
            $remotePort = [int]$conn.RemotePort
            $fingerprint = "{0}:{1}:{2}" -f $processId, $remoteAddr, $remotePort
            if ($script:SeenSmtpConnections.ContainsKey($fingerprint)) { continue }
            $script:SeenSmtpConnections[$fingerprint] = (Get-Date).ToUniversalTime()

            $processName = ''
            try {
                $proc = Get-Process -Id $processId -ErrorAction SilentlyContinue
                $processName = [string]$proc.ProcessName
            }
            catch { }

            Send-EmailHeartbeat -SignalType 'smtp_connection' -Data @{
                remoteAddress = $remoteAddr
                remotePort    = $remotePort
                processId     = $processId
                processName   = $processName
                localPort     = [int]$conn.LocalPort
                collectionMode = 'smtp'
            }
            Write-CollectorLog ("smtp_connection process={0}({1}) remote={2}:{3}" -f $processName, $processId, $remoteAddr, $remotePort)

            Evaluate-EmailRules `
                -Subject '' `
                -RecipientsJoined $remoteAddr `
                -SenderAddress $env:USERNAME `
                -AttachmentCount 0 `
                -AttachmentNames '' `
                -BodyLength 0 `
                -MessageId $fingerprint `
                -OutlookMailItem $null
        }

        # Cleanup old SMTP connections (keep last 8h)
        $cleanupBefore = (Get-Date).ToUniversalTime().AddHours(-8)
        foreach ($k in @($script:SeenSmtpConnections.Keys)) {
            if ([datetime]$script:SeenSmtpConnections[$k] -lt $cleanupBefore) {
                $script:SeenSmtpConnections.Remove($k)
            }
        }
    }
    catch {
        Write-CollectorLog ("SMTP poll error: {0}" -f $_.Exception.Message)
    }
}

# ---------------------------------------------------------------------------
# Initialization
# ---------------------------------------------------------------------------

$deploymentConfig = Get-DeploymentConfig -Path $ConfigPath
$resolvedServerHost = if ($ServerHost) { $ServerHost } elseif ($deploymentConfig) { [string]$deploymentConfig.server.host } else { throw 'ServerHost is required.' }
$resolvedServerPort = if ($PSBoundParameters.ContainsKey('ServerPort')) { $ServerPort } elseif ($deploymentConfig) { [int]$deploymentConfig.server.port } else { 5600 }
$resolvedServerScheme = if ($ServerScheme) { $ServerScheme } elseif ($deploymentConfig) { [string]$deploymentConfig.server.scheme } else { 'http' }
$resolvedPolicyPath = if ($PolicyPath) { $PolicyPath } elseif ($deploymentConfig -and $deploymentConfig.paths.PSObject.Properties.Name -contains 'policyPath') { [string]$deploymentConfig.paths.policyPath } else { 'C:\ProgramData\ActivityWatch\dlp-policy.json' }
$resolvedPollSeconds = if ($PSBoundParameters.ContainsKey('PollSeconds')) { $PollSeconds } elseif ($deploymentConfig) { [int]$deploymentConfig.collector.pollSeconds } else { 10 }
$resolvedLogsRoot = if ($deploymentConfig) { [string]$deploymentConfig.paths.logsRoot } else { 'C:\ProgramData\ActivityWatch\logs' }
$resolvedLogPath = if ($LogPath) { $LogPath } else { Join-Path $resolvedLogsRoot ("email-outbound-{0}.log" -f $env:USERNAME) }
$resolvedLocalAgentLogsEnabled = if ($deploymentConfig -and $deploymentConfig.PSObject.Properties.Name -contains 'logging' -and $deploymentConfig.logging.PSObject.Properties.Name -contains 'localAgentLogsEnabled') { [bool]$deploymentConfig.logging.localAgentLogsEnabled } else { $true }

if ($resolvedLocalAgentLogsEnabled -and -not (Test-Path -LiteralPath $resolvedLogsRoot)) {
    New-Item -Path $resolvedLogsRoot -ItemType Directory -Force | Out-Null
}

$script:ApiBase = '{0}://{1}:{2}/api/0' -f $resolvedServerScheme, $resolvedServerHost, $resolvedServerPort
$script:Hostname = $env:COMPUTERNAME
$script:SessionId = (Get-Process -Id $PID).SessionId
$script:KnownBuckets = @{}
$script:Cooldown = @{}
$script:SeenEntryIds = @{}
$script:SeenSmtpConnections = @{}
$script:PulseSeconds = [Math]::Max($resolvedPollSeconds * 3, 30)
$script:LocalAgentLogsEnabled = $resolvedLocalAgentLogsEnabled
$script:LogPath = $resolvedLogPath
$script:OutlookApp = $null
$script:OutlookNamespace = $null
$script:SentFolder = $null
$script:OutlookLastPoll = (Get-Date).AddMinutes(-5)

Load-EmailPolicy -Path $resolvedPolicyPath
Write-CollectorLog ("email collector started mode={0} against {1}" -f $Mode, $script:ApiBase)

$useOutlook = ($Mode -eq 'outlook' -or $Mode -eq 'both')
$useSmtp = ($Mode -eq 'smtp' -or $Mode -eq 'both')
$outlookReady = $false

if ($useOutlook) {
    $outlookReady = Initialize-OutlookCom
    if (-not $outlookReady -and $Mode -eq 'outlook') {
        Write-CollectorLog "Outlook COM not available, collector will retry"
    }
}

# ---------------------------------------------------------------------------
# Main loop
# ---------------------------------------------------------------------------

while ($true) {
    try {
        if (-not $script:Policy.defaults.enabled) {
            Start-Sleep -Seconds $resolvedPollSeconds
            continue
        }

        if ($useOutlook) {
            if (-not $outlookReady) {
                $outlookReady = Initialize-OutlookCom
            }
            if ($outlookReady) {
                try {
                    Poll-OutlookSentItems
                }
                catch {
                    Write-CollectorLog ("outlook poll error: {0}" -f $_.Exception.Message)
                    $outlookReady = $false
                    $script:OutlookApp = $null
                    $script:OutlookNamespace = $null
                    $script:SentFolder = $null
                }
            }
        }

        if ($useSmtp) {
            try {
                Poll-SmtpConnections
            }
            catch {
                Write-CollectorLog ("smtp poll error: {0}" -f $_.Exception.Message)
            }
        }
    }
    catch {
        Write-CollectorLog ("collector error: {0}" -f $_.Exception.Message)
    }

    Start-Sleep -Seconds $resolvedPollSeconds
}
