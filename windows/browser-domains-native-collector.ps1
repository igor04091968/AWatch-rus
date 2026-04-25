[CmdletBinding()]
param(
    [string]$ConfigPath = 'C:\ProgramData\ActivityWatch\deployment-config.json',
    [string]$ServerHost,
    [int]$ServerPort,
    [ValidateSet('http', 'https')]
    [string]$ServerScheme,
    [string]$RulesPath,
    [string]$LogPath,
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
$resolvedServerHost = if ($ServerHost) { $ServerHost } elseif ($deploymentConfig) { [string]$deploymentConfig.server.host } else { throw 'ServerHost is required.' }
$resolvedServerPort = if ($PSBoundParameters.ContainsKey('ServerPort')) { $ServerPort } elseif ($deploymentConfig) { [int]$deploymentConfig.server.port } else { 5600 }
$resolvedServerScheme = if ($ServerScheme) { $ServerScheme } elseif ($deploymentConfig) { [string]$deploymentConfig.server.scheme } else { 'http' }
$resolvedRulesPath = if ($RulesPath) { $RulesPath } elseif ($deploymentConfig) { [string]$deploymentConfig.paths.rulesPath } else { 'C:\ProgramData\ActivityWatch\web-category-rules.json' }
$resolvedPollSeconds = if ($PSBoundParameters.ContainsKey('PollSeconds')) { $PollSeconds } elseif ($deploymentConfig) { [int]$deploymentConfig.collector.pollSeconds } else { 5 }
$resolvedPulseSeconds = if ($PSBoundParameters.ContainsKey('PulseSeconds')) { $PulseSeconds } elseif ($deploymentConfig) { [int]$deploymentConfig.collector.pulseSeconds } else { 30 }
$resolvedLogsRoot = if ($deploymentConfig) { [string]$deploymentConfig.paths.logsRoot } else { 'C:\ProgramData\ActivityWatch\logs' }
$resolvedLogPath = if ($LogPath) { $LogPath } else { Join-Path $resolvedLogsRoot ("browser-domains-{0}.log" -f $env:USERNAME) }

if (-not (Test-Path -LiteralPath $resolvedLogsRoot)) {
    New-Item -Path $resolvedLogsRoot -ItemType Directory -Force | Out-Null
}

$script:ApiBase = '{0}://{1}:{2}/api/0' -f $resolvedServerScheme, $resolvedServerHost, $resolvedServerPort
$script:Hostname = $env:COMPUTERNAME
$script:SessionId = (Get-Process -Id $PID).SessionId
$script:KnownBuckets = @{}
$script:LogPath = $resolvedLogPath
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

    try {
        Add-Content -LiteralPath $script:LogPath -Value ('{0} {1}' -f (Get-Date -Format s), $Message)
    }
    catch {
    }
}

function Test-DomainMatch {
    param(
        [string]$Host,
        [string]$RuleDomain
    )

    if ([string]::IsNullOrWhiteSpace($Host) -or [string]::IsNullOrWhiteSpace($RuleDomain)) {
        return $false
    }

    $left = $Host.ToLowerInvariant()
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
        $host = $uri.Host.ToLowerInvariant()
        if ($host.StartsWith('www.')) {
            return $host.Substring(4)
        }

        return $host
    }
    catch {
        return $null
    }
}

function Get-RootDomain {
    param([string]$Host)

    if ([string]::IsNullOrWhiteSpace($Host)) {
        return $null
    }

    $parts = $Host.Split('.')
    if ($parts.Count -le 2) {
        return $Host
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
            Write-CollectorLog ("custom rules loaded: {0}" -f $rules.Count)
        }
    }
    catch {
        Write-CollectorLog ("custom rules load failed: {0}" -f $_.Exception.Message)
    }
}

function Get-WebCategory {
    param([string]$Host)

    foreach ($rule in $script:CategoryRules) {
        foreach ($domain in $rule.Domains) {
            if (Test-DomainMatch -Host $Host -RuleDomain $domain) {
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

    $bucketId = 'aw-watcher-web-category_' + $script:Hostname
    Ensure-Bucket -BucketId $bucketId -ClientName 'aw-watcher-web-category' -BucketType 'aw.web.category'

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
Write-CollectorLog ("collector started against {0}" -f $script:ApiBase)

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

                $rootDomain = Get-RootDomain -Host $domain
                if (-not $rootDomain) {
                    $rootDomain = $domain
                }

                $category = Get-WebCategory -Host $domain
                $bucketId = 'aw-watcher-web-{0}_{1}' -f $browserKey, $script:Hostname
                Ensure-Bucket -BucketId $bucketId -ClientName ('aw-watcher-web-' + $browserKey)
                Send-Heartbeat -BucketId $bucketId -Url $url -Title $context.Title -BrowserKey $browserKey -ProcessName $context.ProcessName
                Send-CategoryHeartbeat -Url $url -Title $context.Title -BrowserKey $browserKey -ProcessName $context.ProcessName -Domain $domain -RootDomain $rootDomain -Category $category.Name -CategoryGroup $category.Group -CategoryRule $category.Rule
            }
        }
    }
    catch {
        Write-CollectorLog ("collector error: {0}" -f $_.Exception.Message)
    }

    Start-Sleep -Seconds $resolvedPollSeconds
}
