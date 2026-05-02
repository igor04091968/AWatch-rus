param(
    [string]$ConfigPath = 'C:\ProgramData\AWatch-rus\deployment-config.json',
    [string]$Hostname,
    [int]$PollSeconds = 30
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-Config {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        throw "Конфигурация не найдена: $Path"
    }

    Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
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
        [Parameter(Mandatory = $true)][string]$ApiBase,
        [Parameter(Mandatory = $true)][string]$BucketId,
        [Parameter(Mandatory = $true)][string]$HostnameValue
    )

    try {
        Invoke-RestMethod -Method Get -Uri "$ApiBase/buckets/$BucketId" | Out-Null
        return
    }
    catch {
    }

    $body = @{
        client   = 'aw-worktime-session-collector'
        type     = 'aw.worktime.session'
        hostname = $HostnameValue
    } | ConvertTo-Json -Compress

    Invoke-AwJsonPost -Uri "$ApiBase/buckets/$BucketId" -Json $body
}

function Get-SessionRecords {
    $records = @()

    try {
        $lines = quser 2>$null
        if (-not $lines) {
            return @()
        }

        foreach ($line in ($lines | Select-Object -Skip 1)) {
            $clean = ($line -replace '^\s*>?', '').Trim()
            if (-not $clean) {
                continue
            }

            $parts = $clean -split '\s+'
            if ($parts.Count -lt 4) {
                continue
            }

            $sessionName = ''
            $sessionIdIndex = 2
            if ($parts[1] -match '^\d+$') {
                $sessionIdIndex = 1
            }
            else {
                $sessionName = $parts[1]
            }

            $sessionId = 0
            if ($parts[$sessionIdIndex] -match '^\d+$') {
                $sessionId = [int]$parts[$sessionIdIndex]
            }

            $records += [pscustomobject]@{
                username    = $parts[0]
                sessionName = $sessionName
                sessionId   = $sessionId
                state       = $parts[$sessionIdIndex + 1]
            }
        }
    }
    catch {
    }

    return $records
}

$cfg = Get-Config -Path $ConfigPath
$hostValue = if ($Hostname) { $Hostname } else { [string]$env:COMPUTERNAME }
$apiBase = '{0}://{1}:{2}/api/0' -f [string]$cfg.server.scheme, [string]$cfg.server.host, [string]$cfg.server.port
$bucketId = 'aw-worktime-sessions_' + $hostValue
$pulse = 120
$sleepSec = if ($PollSeconds -gt 0) {
    $PollSeconds
}
elseif ($cfg.collector -and $cfg.collector.pollSeconds) {
    [int]$cfg.collector.pollSeconds
}
else {
    30
}

Ensure-Bucket -ApiBase $apiBase -BucketId $bucketId -HostnameValue $hostValue

while ($true) {
    $now = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
    $records = Get-SessionRecords
    if (-not $records -or $records.Count -eq 0) {
        $records = @([pscustomobject]@{
            username    = $env:USERNAME
            sessionName = ''
            sessionId   = (Get-Process -Id $PID).SessionId
            state       = 'Unknown'
        })
    }

    foreach ($rec in $records) {
        $payload = @{
            timestamp = $now
            duration  = 0
            data      = @{
                username    = [string]$rec.username
                userId      = "$($env:USERDOMAIN)\$($rec.username)"
                sessionId   = [int]$rec.sessionId
                sessionName = [string]$rec.sessionName
                state       = [string]$rec.state
                active      = ($rec.state -match 'Active')
                hostname    = $hostValue
                source      = 'worktime-session-collector'
            }
        } | ConvertTo-Json -Depth 6 -Compress

        try {
            Invoke-AwJsonPost -Uri "$apiBase/buckets/$bucketId/heartbeat?pulsetime=$pulse" -Json $payload
        }
        catch {
        }
    }

    Start-Sleep -Seconds $sleepSec
}
