[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\ActivityWatch-Phase2\deployment-config.json',
    [string]$ServerHost,
    [int]$ServerPort,
    [ValidateSet('http', 'https')]
    [string]$ServerScheme,
    [string]$RulesPath,
    [string]$PolicyPath,
    [string]$LogPath,
    [string]$IncidentLogPath,
    [int]$PollSeconds,
    [int]$PulseSeconds
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class NativeAwMethods {
    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

    [DllImport("user32.dll")]
    public static extern int GetWindowTextLength(IntPtr hWnd);
}
"@

function Get-DeploymentConfig {
    param([string]$Path)
    if ($Path -and (Test-Path -LiteralPath $Path)) {
        return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    }

    return $null
}

$deploymentConfig = Get-DeploymentConfig -Path $ConfigPath
$resolvedServerHost = if ($ServerHost) { $ServerHost } elseif ($deploymentConfig) { [string]$deploymentConfig.server.host } else { throw 'Укажите ServerHost или подготовьте deployment-config.json.' }
$resolvedServerPort = if ($PSBoundParameters.ContainsKey('ServerPort')) { $ServerPort } elseif ($deploymentConfig) { [int]$deploymentConfig.server.port } else { 5600 }
$resolvedServerScheme = if ($ServerScheme) { $ServerScheme } elseif ($deploymentConfig) { [string]$deploymentConfig.server.scheme } else { 'http' }
$resolvedRulesPath = if ($RulesPath) { $RulesPath } elseif ($deploymentConfig) { [string]$deploymentConfig.paths.rulesPath } else { 'C:\ProgramData\ActivityWatch-Phase2\web-category-rules.json' }
$resolvedPolicyPath = if ($PolicyPath) { $PolicyPath } elseif ($deploymentConfig) { [string]$deploymentConfig.paths.policyPath } else { 'C:\ProgramData\ActivityWatch-Phase2\dlp-policy.json' }
$resolvedPollSeconds = if ($PSBoundParameters.ContainsKey('PollSeconds')) { $PollSeconds } elseif ($deploymentConfig) { [int]$deploymentConfig.collector.pollSeconds } else { 5 }
$resolvedPulseSeconds = if ($PSBoundParameters.ContainsKey('PulseSeconds')) { $PulseSeconds } elseif ($deploymentConfig) { [int]$deploymentConfig.collector.pulseSeconds } else { 30 }
$resolvedLogsRoot = if ($deploymentConfig) { [string]$deploymentConfig.paths.logsRoot } else { 'C:\ProgramData\ActivityWatch-Phase2\logs' }
$resolvedLogPath = if ($LogPath) { $LogPath } else { Join-Path $resolvedLogsRoot ("browser-domains-{0}.log" -f $env:USERNAME) }
$resolvedIncidentLogPath = if ($IncidentLogPath) { $IncidentLogPath } else { Join-Path $resolvedLogsRoot ("dlp-incidents-{0}.log" -f $env:USERNAME) }
$resolvedLocalAgentLogsEnabled = if ($deploymentConfig -and $deploymentConfig.PSObject.Properties.Name -contains 'logging' -and $deploymentConfig.logging.PSObject.Properties.Name -contains 'localAgentLogsEnabled') { [bool]$deploymentConfig.logging.localAgentLogsEnabled } else { $true }
$resolvedIncidentArtifactsRoot = if ($deploymentConfig -and $deploymentConfig.PSObject.Properties.Name -contains 'incidentCapture' -and $deploymentConfig.incidentCapture.PSObject.Properties.Name -contains 'artifactsRoot') { [string]$deploymentConfig.incidentCapture.artifactsRoot } else { Join-Path $env:LOCALAPPDATA 'ActivityWatch-Phase2\\incident-artifacts' }
$resolvedIncidentScreenshotEnabled = if ($deploymentConfig -and $deploymentConfig.PSObject.Properties.Name -contains 'incidentCapture' -and $deploymentConfig.incidentCapture.PSObject.Properties.Name -contains 'screenshotEnabled') { [bool]$deploymentConfig.incidentCapture.screenshotEnabled } else { $true }

if ($resolvedLocalAgentLogsEnabled -and -not (Test-Path -LiteralPath $resolvedLogsRoot)) {
    New-Item -Path $resolvedLogsRoot -ItemType Directory -Force | Out-Null
}

$script:ApiBase = '{0}://{1}:{2}/api/0' -f $resolvedServerScheme, $resolvedServerHost, $resolvedServerPort
$script:Hostname = $env:COMPUTERNAME
$script:SessionId = (Get-Process -Id $PID).SessionId
$script:KnownBuckets = @{}
$script:LocalAgentLogsEnabled = $resolvedLocalAgentLogsEnabled
$script:LogPath = $resolvedLogPath
$script:IncidentLogPath = $resolvedIncidentLogPath
$script:IncidentArtifactsRoot = $resolvedIncidentArtifactsRoot
$script:IncidentScreenshotEnabled = $resolvedIncidentScreenshotEnabled
$script:ScreenshotTypesLoaded = $false
$script:IncidentState = @{}
$script:DlpRules = @()
$script:DlpDefaults = [ordered]@{
    enabled = $false
    cooldownSeconds = 300
    action = 'log'
    severity = 'low'
}
$script:BrowserMap = @{
    msedge  = 'edge'
    chrome  = 'chrome'
    brave   = 'brave'
    vivaldi = 'vivaldi'
    opera   = 'opera'
    firefox = 'firefox'
}
$script:CategoryRules = @(
    @{ Name = 'work_business_systems'; Group = 'work'; Domains = @('bitrix24.ru', '1c.ru', 'sbis.ru', 'kontur.ru', 'diadoc.ru', 'nalog.gov.ru', 'gosuslugi.ru') }
    @{ Name = 'work_docs_collab'; Group = 'work'; Domains = @('office.com', 'sharepoint.com', 'docs.google.com', 'drive.google.com', 'notion.so', 'miro.com') }
    @{ Name = 'work_dev'; Group = 'work'; Domains = @('github.com', 'gitlab.com', 'bitbucket.org', 'youtrack.cloud', 'atlassian.net') }
    @{ Name = 'work_communication'; Group = 'work'; Domains = @('teams.microsoft.com', 'outlook.office.com', 'web.telegram.org', 'slack.com', 'zoom.us') }
    @{ Name = 'neutral_search_reference'; Group = 'neutral'; Domains = @('google.com', 'google.ru', 'yandex.ru', 'bing.com', 'duckduckgo.com', 'wikipedia.org') }
    @{ Name = 'neutral_news'; Group = 'neutral'; Domains = @('rbc.ru', 'tass.ru', 'ria.ru', 'kommersant.ru', 'vedomosti.ru') }
    @{ Name = 'personal_social'; Group = 'personal'; Domains = @('vk.com', 'ok.ru', 'facebook.com', 'instagram.com', 'tiktok.com', 'x.com', 'twitter.com') }
    @{ Name = 'personal_video'; Group = 'personal'; Domains = @('youtube.com', 'youtu.be', 'rutube.ru', 'twitch.tv', 'kinopoisk.ru') }
    @{ Name = 'personal_marketplace'; Group = 'personal'; Domains = @('ozon.ru', 'wildberries.ru', 'avito.ru', 'aliexpress.com', 'market.yandex.ru') }
    @{ Name = 'personal_entertainment'; Group = 'personal'; Domains = @('dzen.ru', 'pikabu.ru', 'dtf.ru', 'playground.ru') }
)

function Write-CollectorLog {
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

function Write-DlpIncidentLog {
    param([string]$Message)

    if (-not $script:LocalAgentLogsEnabled) {
        return
    }

    try {
        Add-Content -LiteralPath $script:IncidentLogPath -Value ('{0} {1}' -f (Get-Date -Format s), $Message)
    }
    catch {
    }
}

function Test-DomainMatch {
    param(
        [string]$DomainHost,
        [string]$RuleDomain
    )

    if ([string]::IsNullOrWhiteSpace($DomainHost) -or [string]::IsNullOrWhiteSpace($RuleDomain)) {
        return $false
    }

    $left = $DomainHost.ToLowerInvariant()
    $right = $RuleDomain.ToLowerInvariant()
    return $left -eq $right -or $left.EndsWith('.' + $right)
}

function Get-HostFromUrl {
    param([string]$Url)

    if ([string]::IsNullOrWhiteSpace($Url)) {
        return $null
    }

    try {
        $uri = [Uri]$Url
        $uriHost = $uri.Host.ToLowerInvariant()
        if ($uriHost.StartsWith('www.')) {
            return $uriHost.Substring(4)
        }

        return $uriHost
    }
    catch {
        return $null
    }
}

function Get-RootDomain {
    param([string]$DomainHost)

    if ([string]::IsNullOrWhiteSpace($DomainHost)) {
        return $null
    }

    $parts = $DomainHost.Split('.')
    if ($parts.Count -le 2) {
        return $DomainHost
    }

    $suffix = ('{0}.{1}' -f $parts[$parts.Count - 2], $parts[$parts.Count - 1]).ToLowerInvariant()
    $compoundTlds = @('co.uk', 'com.au', 'co.jp', 'com.br', 'co.in', 'com.tr', 'com.cn')
    if (($compoundTlds -contains $suffix) -and $parts.Count -ge 3) {
        return ('{0}.{1}' -f $parts[$parts.Count - 3], $suffix).ToLowerInvariant()
    }

    return $suffix
}

function ConvertTo-NormalizedUrl {
    param([AllowNull()][string]$Value)

    if ([string]::IsNullOrWhiteSpace($Value)) {
        return $null
    }

    $candidate = $Value.Trim()
    if ($candidate.Length -lt 4) {
        return $null
    }

    if ($candidate -match '^(?i)(search|find|address and search|search with|новая вкладка|new tab)') {
        return $null
    }

    if ($candidate -match '^(?i)(https?|file|ftp|chrome|edge|about|view-source)://') {
        return $candidate
    }

    if ($candidate -match '^(?i)localhost([/:]|$)') {
        return "http://$candidate"
    }

    if ($candidate -match '^[a-z0-9.-]+\.[a-z]{2,}([/:?#].*)?$') {
        return "https://$candidate"
    }

    return $null
}

function Load-CustomCategoryRules {
    param([string]$Path)

    if (-not $Path -or -not (Test-Path -LiteralPath $Path)) {
        return
    }

    try {
        $parsed = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
        $rules = @()

        if ($parsed.rules) {
            $sourceRules = @($parsed.rules)
        }
        elseif ($parsed -is [System.Collections.IEnumerable]) {
            $sourceRules = @($parsed)
        }
        else {
            $sourceRules = @()
        }

        foreach ($rule in $sourceRules) {
            if (-not $rule) {
                continue
            }

            $name = [string]$rule.name
            $group = [string]$rule.group
            $domains = @($rule.domains | ForEach-Object { ([string]$_).Trim().ToLowerInvariant() } | Where-Object { $_ })

            if ($name -and $group -and $domains.Count -gt 0) {
                $rules += @{
                    Name    = $name
                    Group   = $group
                    Domains = $domains
                }
            }
        }

        if ($rules.Count -gt 0) {
            $script:CategoryRules = @($rules) + @($script:CategoryRules)
            Write-CollectorLog ("пользовательские правила загружены: {0}" -f $rules.Count)
        }
    }
    catch {
        Write-CollectorLog ("не удалось загрузить пользовательские правила: {0}" -f $_.Exception.Message)
    }
}

function Get-WebCategory {
    param([string]$DomainHost)

    foreach ($rule in $script:CategoryRules) {
        foreach ($domain in $rule.Domains) {
            if (Test-DomainMatch -DomainHost $DomainHost -RuleDomain $domain) {
                return [pscustomobject]@{
                    Name  = [string]$rule.Name
                    Group = [string]$rule.Group
                    Rule  = [string]$domain
                }
            }
        }
    }

    return [pscustomobject]@{
        Name  = 'uncategorized'
        Group = 'neutral'
        Rule  = 'none'
    }
}

function Test-DomainListMatch {
    param(
        [string]$DomainHost,
        [string[]]$Domains
    )

    if (-not $Domains -or $Domains.Count -eq 0) {
        return $false
    }

    foreach ($domain in $Domains) {
        if (Test-DomainMatch -DomainHost $DomainHost -RuleDomain $domain) {
            return $true
        }
    }

    return $false
}

function Test-DlpRuleTimeWindow {
    param(
        [int]$CurrentHour,
        [AllowNull()][int]$HourFrom,
        [AllowNull()][int]$HourTo
    )

    if ($null -eq $HourFrom -or $null -eq $HourTo) {
        return $true
    }

    if ($HourFrom -eq $HourTo) {
        return $true
    }

    if ($HourFrom -lt $HourTo) {
        return ($CurrentHour -ge $HourFrom -and $CurrentHour -lt $HourTo)
    }

    return ($CurrentHour -ge $HourFrom -or $CurrentHour -lt $HourTo)
}

function Load-DlpPolicy {
    param([string]$Path)

    if (-not $Path -or -not (Test-Path -LiteralPath $Path)) {
        Write-CollectorLog ("DLP-политика не найдена, DLP отключен: {0}" -f $Path)
        return
    }

    try {
        $parsed = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
        $defaults = $parsed.defaults
        if ($defaults) {
            if ($defaults.PSObject.Properties.Name -contains 'enabled') {
                $script:DlpDefaults.enabled = [bool]$defaults.enabled
            }
            if ($defaults.cooldownSeconds) {
                $script:DlpDefaults.cooldownSeconds = [int]$defaults.cooldownSeconds
            }
            if ($defaults.action) {
                $script:DlpDefaults.action = [string]$defaults.action
            }
            if ($defaults.severity) {
                $script:DlpDefaults.severity = [string]$defaults.severity
            }
        }

        $loaded = @()
        foreach ($rule in @($parsed.rules)) {
            if (-not $rule) { continue }
            $when = $rule.when
            if (-not $when) {
                $when = [pscustomobject]@{}
            }
            $loaded += [pscustomobject]@{
                id = [string]$rule.id
                enabled = if ($rule.PSObject.Properties.Name -contains 'enabled') { [bool]$rule.enabled } else { $true }
                action = if ($rule.action) { [string]$rule.action } else { [string]$script:DlpDefaults.action }
                severity = if ($rule.severity) { [string]$rule.severity } else { [string]$script:DlpDefaults.severity }
                message = if ($rule.message) { [string]$rule.message } else { "Сработало DLP-правило: $($rule.id)" }
                cooldownSeconds = if ($rule.cooldownSeconds) { [int]$rule.cooldownSeconds } else { [int]$script:DlpDefaults.cooldownSeconds }
                when = [pscustomobject]@{
                    domains = if ($when.PSObject.Properties.Name -contains 'domains') { @($when.domains | ForEach-Object { ([string]$_).Trim().ToLowerInvariant() } | Where-Object { $_ }) } else { @() }
                    categoryGroups = if ($when.PSObject.Properties.Name -contains 'categoryGroups') { @($when.categoryGroups | ForEach-Object { ([string]$_).Trim().ToLowerInvariant() } | Where-Object { $_ }) } else { @() }
                    categories = if ($when.PSObject.Properties.Name -contains 'categories') { @($when.categories | ForEach-Object { ([string]$_).Trim().ToLowerInvariant() } | Where-Object { $_ }) } else { @() }
                    browsers = if ($when.PSObject.Properties.Name -contains 'browsers') { @($when.browsers | ForEach-Object { ([string]$_).Trim().ToLowerInvariant() } | Where-Object { $_ }) } else { @() }
                    urlRegex = if ($when.PSObject.Properties.Name -contains 'urlRegex' -and $when.urlRegex) { [string]$when.urlRegex } else { $null }
                    titleRegex = if ($when.PSObject.Properties.Name -contains 'titleRegex' -and $when.titleRegex) { [string]$when.titleRegex } else { $null }
                    hourFrom = if ($when.PSObject.Properties.Name -contains 'hourFrom') { [int]$when.hourFrom } else { $null }
                    hourTo = if ($when.PSObject.Properties.Name -contains 'hourTo') { [int]$when.hourTo } else { $null }
                }
            }
        }

        $script:DlpRules = @($loaded)
        Write-CollectorLog ("DLP-политика загружена: включена={0}, правил={1}" -f $script:DlpDefaults.enabled, $script:DlpRules.Count)
    }
    catch {
        Write-CollectorLog ("не удалось разобрать DLP-политику: {0}" -f $_.Exception.Message)
    }
}

function Test-DlpRuleMatch {
    param(
        [pscustomobject]$Rule,
        [string]$Domain,
        [string]$RootDomain,
        [string]$Url,
        [string]$Title,
        [string]$BrowserKey,
        [string]$Category,
        [string]$CategoryGroup
    )

    if (-not $Rule.enabled) {
        return $false
    }

    $when = $Rule.when
    $currentHour = (Get-Date).Hour
    if (-not (Test-DlpRuleTimeWindow -CurrentHour $currentHour -HourFrom $when.hourFrom -HourTo $when.hourTo)) {
        return $false
    }

    if ($when.domains.Count -gt 0) {
        $domainMatched = (Test-DomainListMatch -DomainHost $Domain -Domains $when.domains) -or (Test-DomainListMatch -DomainHost $RootDomain -Domains $when.domains)
        if (-not $domainMatched) {
            return $false
        }
    }

    if ($when.categoryGroups.Count -gt 0 -and ($when.categoryGroups -notcontains $CategoryGroup.ToLowerInvariant())) {
        return $false
    }

    if ($when.categories.Count -gt 0 -and ($when.categories -notcontains $Category.ToLowerInvariant())) {
        return $false
    }

    if ($when.browsers.Count -gt 0 -and ($when.browsers -notcontains $BrowserKey.ToLowerInvariant())) {
        return $false
    }

    if ($when.urlRegex) {
        if (-not ($Url -match $when.urlRegex)) {
            return $false
        }
    }

    if ($when.titleRegex) {
        if (-not ($Title -match $when.titleRegex)) {
            return $false
        }
    }

    return $true
}

function Get-DlpDecision {
    param(
        [string]$Domain,
        [string]$RootDomain,
        [string]$Url,
        [string]$Title,
        [string]$BrowserKey,
        [string]$Category,
        [string]$CategoryGroup
    )

    if (-not $script:DlpDefaults.enabled) {
        return $null
    }

    foreach ($rule in $script:DlpRules) {
        if (Test-DlpRuleMatch -Rule $rule -Domain $Domain -RootDomain $RootDomain -Url $Url -Title $Title -BrowserKey $BrowserKey -Category $Category -CategoryGroup $CategoryGroup) {
            return $rule
        }
    }

    return $null
}

function Should-EmitIncident {
    param(
        [string]$Fingerprint,
        [int]$CooldownSeconds
    )

    $now = (Get-Date).ToUniversalTime()
    if ($script:IncidentState.ContainsKey($Fingerprint)) {
        $last = [datetime]$script:IncidentState[$Fingerprint]
        if ((New-TimeSpan -Start $last -End $now).TotalSeconds -lt $CooldownSeconds) {
            return $false
        }
    }

    $script:IncidentState[$Fingerprint] = $now
    return $true
}

function Send-DlpIncidentHeartbeat {
    param(
        [pscustomobject]$Decision,
        [string]$Url,
        [string]$Title,
        [string]$BrowserKey,
        [string]$ProcessName,
        [string]$Domain,
        [string]$RootDomain,
        [string]$Category,
        [string]$CategoryGroup
    )

    $bucketId = 'aw-dlp-incidents_' + $script:Hostname
    Ensure-Bucket -BucketId $bucketId -ClientName 'aw-dlp-incidents' -BucketType 'aw.dlp.incident'

    $captureData = @{}
    if ($script:IncidentScreenshotEnabled) {
        try {
            $captureData = Capture-IncidentScreenshot -RuleId ([string]$Decision.id) -SignalType 'web'
        }
        catch {
        }
    }

    $event = @{
        timestamp = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
        duration  = 0
        data      = @{
            ruleId         = [string]$Decision.id
            action         = [string]$Decision.action
            severity       = [string]$Decision.severity
            message        = [string]$Decision.message
            url            = $Url
            title          = $Title
            browser        = $BrowserKey
            app            = "$ProcessName.exe"
            domain         = $Domain
            rootDomain     = $RootDomain
            category       = $Category
            categoryGroup  = $CategoryGroup
            username       = $env:USERNAME
            hostname       = $script:Hostname
            sessionId      = $script:SessionId
            source         = 'uia-native-dlp'
        } + $captureData
    } | ConvertTo-Json -Depth 5 -Compress

    Invoke-RestMethod -Method Post -Uri "$($script:ApiBase)/buckets/$bucketId/heartbeat?pulsetime=$resolvedPulseSeconds" -ContentType 'application/json' -Body $event | Out-Null
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
        Write-CollectorLog ("не удалось сделать снимок инцидента: {0}" -f $_.Exception.Message)
        return @{}
    }
}

function Get-ForegroundWindowContext {
    $handle = [NativeAwMethods]::GetForegroundWindow()
    if ($handle -eq [IntPtr]::Zero) {
        return $null
    }

    $processId = [uint32]0
    [void][NativeAwMethods]::GetWindowThreadProcessId($handle, [ref]$processId)
    if (-not $processId) {
        return $null
    }

    $process = Get-Process -Id ([int]$processId) -ErrorAction SilentlyContinue
    if (-not $process) {
        return $null
    }

    $textLength = [NativeAwMethods]::GetWindowTextLength($handle)
    $builder = [Text.StringBuilder]::new([Math]::Max($textLength + 1, 260))
    [void][NativeAwMethods]::GetWindowText($handle, $builder, $builder.Capacity)

    return [pscustomobject]@{
        Handle      = $handle
        ProcessName = $process.ProcessName.ToLowerInvariant()
        Title       = $builder.ToString()
    }
}

function Get-BrowserUrlFromWindow {
    param([IntPtr]$Handle)

    $root = [System.Windows.Automation.AutomationElement]::FromHandle($Handle)
    if (-not $root) {
        return $null
    }

    $editCondition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Edit
    )

    $edits = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $editCondition)
    foreach ($edit in $edits) {
        $valuePattern = $null
        if ($edit.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$valuePattern)) {
            $candidate = ConvertTo-NormalizedUrl -Value $valuePattern.Current.Value
            if ($candidate) {
                return $candidate
            }
        }

        $candidateFromName = ConvertTo-NormalizedUrl -Value $edit.Current.Name
        if ($candidateFromName) {
            return $candidateFromName
        }
    }

    return $null
}

function Ensure-Bucket {
    param(
        [string]$BucketId,
        [string]$ClientName,
        [string]$BucketType = 'web.tab.current'
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

function Send-Heartbeat {
    param(
        [string]$BucketId,
        [string]$Url,
        [string]$Title,
        [string]$BrowserKey,
        [string]$ProcessName
    )

    $event = @{
        timestamp = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
        duration  = 0
        data      = @{
            url       = $Url
            title     = $Title
            browser   = $BrowserKey
            app       = "$ProcessName.exe"
            source    = 'uia-native'
            sessionId = $script:SessionId
        }
    } | ConvertTo-Json -Depth 4 -Compress

    Invoke-RestMethod -Method Post -Uri "$($script:ApiBase)/buckets/$BucketId/heartbeat?pulsetime=$resolvedPulseSeconds" -ContentType 'application/json' -Body $event | Out-Null
}

function Send-CategoryHeartbeat {
    param(
        [string]$Url,
        [string]$Title,
        [string]$BrowserKey,
        [string]$ProcessName,
        [string]$Domain,
        [string]$RootDomain,
        [string]$Category,
        [string]$CategoryGroup,
        [string]$CategoryRule
    )

    $bucketId = 'aw-detmir-web-category_' + $script:Hostname
    Ensure-Bucket -BucketId $bucketId -ClientName 'aw-detmir-web-category' -BucketType 'aw.web.category'

    $event = @{
        timestamp = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
        duration  = 0
        data      = @{
            url           = $Url
            title         = $Title
            browser       = $BrowserKey
            app           = "$ProcessName.exe"
            domain        = $Domain
            rootDomain    = $RootDomain
            category      = $Category
            categoryGroup = $CategoryGroup
            categoryRule  = $CategoryRule
            source        = 'uia-native'
            sessionId     = $script:SessionId
        }
    } | ConvertTo-Json -Depth 4 -Compress

    Invoke-RestMethod -Method Post -Uri "$($script:ApiBase)/buckets/$bucketId/heartbeat?pulsetime=$resolvedPulseSeconds" -ContentType 'application/json' -Body $event | Out-Null
}

Load-CustomCategoryRules -Path $resolvedRulesPath
Load-DlpPolicy -Path $resolvedPolicyPath
Write-CollectorLog ("коллектор запущен для {0}" -f $script:ApiBase)

while ($true) {
    try {
        $context = Get-ForegroundWindowContext
        if ($context -and $script:BrowserMap.ContainsKey($context.ProcessName)) {
            $url = Get-BrowserUrlFromWindow -Handle $context.Handle
            if ($url) {
                $browserKey = $script:BrowserMap[$context.ProcessName]
                $domain = Get-HostFromUrl -Url $url
                if (-not $domain) {
                    $domain = 'unknown'
                }

                $rootDomain = Get-RootDomain -DomainHost $domain
                if (-not $rootDomain) {
                    $rootDomain = $domain
                }

                $category = Get-WebCategory -DomainHost $domain
                $bucketId = 'aw-watcher-web-{0}_{1}' -f $browserKey, $script:Hostname
                Ensure-Bucket -BucketId $bucketId -ClientName ('aw-watcher-web-' + $browserKey)
                Send-Heartbeat -BucketId $bucketId -Url $url -Title $context.Title -BrowserKey $browserKey -ProcessName $context.ProcessName
                Send-CategoryHeartbeat -Url $url -Title $context.Title -BrowserKey $browserKey -ProcessName $context.ProcessName -Domain $domain -RootDomain $rootDomain -Category $category.Name -CategoryGroup $category.Group -CategoryRule $category.Rule

                $decision = Get-DlpDecision -Domain $domain -RootDomain $rootDomain -Url $url -Title $context.Title -BrowserKey $browserKey -Category $category.Name -CategoryGroup $category.Group
                if ($decision) {
                    $fingerprint = '{0}|{1}|{2}|{3}' -f $decision.id, $browserKey, $rootDomain, $env:USERNAME
                    $cooldown = [Math]::Max([int]$decision.cooldownSeconds, 30)
                    if (Should-EmitIncident -Fingerprint $fingerprint -CooldownSeconds $cooldown) {
                        Write-DlpIncidentLog ("{0} {1} {2} {3}" -f $decision.severity, $decision.action, $decision.id, $url)
                        if (@('alert', 'block', 'quarantine') -contains ([string]$decision.action).ToLowerInvariant()) {
                            Send-DlpIncidentHeartbeat -Decision $decision -Url $url -Title $context.Title -BrowserKey $browserKey -ProcessName $context.ProcessName -Domain $domain -RootDomain $rootDomain -Category $category.Name -CategoryGroup $category.Group
                        }
                    }
                }
            }
        }
    }
    catch {
        Write-CollectorLog ("ошибка коллектора: {0}" -f $_.Exception.Message)
    }

    Start-Sleep -Seconds $resolvedPollSeconds
}
