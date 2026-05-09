param(
    [string]$ConfigPath = 'C:\ProgramData\AWatch-rus\deployment-config.json',
    [string]$Hostname,
    [int]$PollSeconds = 30
)

# Force UTF-8 for console I/O
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch {}
try { [Console]::InputEncoding  = [System.Text.Encoding]::UTF8 } catch {}
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Continue'

function Decode-Bytes-Auto {
    param([byte[]]$Bytes)
    if (-not $Bytes) { return '' }

    $candidates = @()

    # Try strict UTF8 first (detect invalid sequences)
    try {
        $utf8Strict = New-Object System.Text.UTF8Encoding($false,$true)
        $txt = $utf8Strict.GetString($Bytes)
        $candidates += @{enc='utf8'; text=$txt}
    }
    catch {
        # invalid UTF8 sequences; ignore
    }

    # Try CP866 and CP1251
    try { $cp866 = [System.Text.Encoding]::GetEncoding(866); $txt866 = $cp866.GetString($Bytes); $candidates += @{enc='cp866'; text=$txt866} } catch {}
    try { $cp1251 = [System.Text.Encoding]::GetEncoding(1251); $txt1251 = $cp1251.GetString($Bytes); $candidates += @{enc='cp1251'; text=$txt1251} } catch {}

    # If nothing decoded yet, fallback to UTF8 permissive
    if ($candidates.Count -eq 0) {
        try { $txt = [System.Text.Encoding]::UTF8.GetString($Bytes); return $txt } catch { return '' }
    }

    # Score decodings by count of Cyrillic letters; prefer highest
    $best = $null; $bestScore = -1
    foreach ($c in $candidates) {
        $t = $c.text
        if (-not $t) { continue }
        $score = 0
        try { $score = ([regex]::Matches($t,'\p{IsCyrillic}')).Count } catch { $score = 0 }
        if ($score -gt $bestScore) { $best = $c; $bestScore = $score }
    }

    if ($best -ne $null) { return $best.text }

    # Final fallback: first candidate text
    return $candidates[0].text
}

function Get-Config {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        throw "Config not found: $Path"
    }
    try {
        $bytes = [System.IO.File]::ReadAllBytes($Path)
        # Config is JSON. Prefer deterministic BOM-based decoding over heuristics.
        if ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF) {
            $text = [System.Text.Encoding]::UTF8.GetString($bytes)
        } elseif ($bytes.Length -ge 2 -and $bytes[0] -eq 0xFF -and $bytes[1] -eq 0xFE) {
            $text = [System.Text.Encoding]::Unicode.GetString($bytes)
        } else {
            $text = [System.Text.Encoding]::UTF8.GetString($bytes)
        }
        $text = $text -replace '^\uFEFF', ''
        return $text | ConvertFrom-Json -ErrorAction Stop
    }
    catch {
        throw "Failed to read config: $Path - $($_.Exception.Message)"
    }
}

function Invoke-AwJsonPost {
    param(
        [Parameter(Mandatory = $true)][string]$Uri,
        [Parameter(Mandatory = $true)][string]$Json
    )
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Json)
        Invoke-RestMethod -Method Post -Uri $Uri -ContentType 'application/json; charset=utf-8' -Body $bytes -ErrorAction Stop | Out-Null
        return $true
    }
    catch {
        Write-Verbose "POST error: $($_.Exception.Message)"
        return $false
    }
}

function Ensure-Bucket {
    param(
        [Parameter(Mandatory = $true)][string]$ApiBase,
        [Parameter(Mandatory = $true)][string]$BucketId,
        [Parameter(Mandatory = $true)][string]$HostnameValue
    )
    try { Invoke-RestMethod -Method Get -Uri "$ApiBase/buckets/$BucketId" -ErrorAction Stop | Out-Null; return } catch { Write-Verbose "Bucket not found, creating: $BucketId" }

    $body = @{ client='aw-worktime-session-collector'; type='aw.worktime.session'; hostname=$HostnameValue } | ConvertTo-Json -Compress
    $attempts = 0
    while ($attempts -lt 3) {
        $attempts++
        $ok = Invoke-AwJsonPost -Uri "$ApiBase/buckets/$BucketId" -Json $body
        if ($ok) { return }
        Start-Sleep -Seconds (2 * $attempts)
    }
    try { Invoke-RestMethod -Method Get -Uri "$ApiBase/buckets/$BucketId" -ErrorAction Stop | Out-Null } catch { Write-Verbose "Ensure-Bucket final check failed: $BucketId" }
}

function Run-QueryUser {
    $tries = @(@{File='quser';Args=''},@{File='query';Args='user'})
    foreach ($t in $tries) {
        try {
            $psi = New-Object System.Diagnostics.ProcessStartInfo
            $psi.FileName = $t.File
            if ($t.Args) { $psi.Arguments = $t.Args }
            $psi.RedirectStandardOutput = $true
            $psi.RedirectStandardError  = $true
            $psi.UseShellExecute = $false
            $psi.CreateNoWindow = $true

            $proc = [System.Diagnostics.Process]::Start($psi)
            $stream = $proc.StandardOutput.BaseStream
            $ms = New-Object System.IO.MemoryStream
            $buffer = New-Object byte[] 4096
            while (($read = $stream.Read($buffer,0,$buffer.Length)) -gt 0) { $ms.Write($buffer,0,$read) }
            $proc.WaitForExit()
            $bytes = $ms.ToArray()

            $text = Decode-Bytes-Auto -Bytes $bytes
            if ($text -and $text.Trim()) { return ($text -split "\r?\n") | Where-Object { $_ -ne '' } }
        }
        catch {
            # try next
        }
    }
    return @()
}

function Parse-SessionLines {
    param([string[]]$Lines)
    $records = @()
    if (-not $Lines) { return $records }

    $startIndex = 0
    # NOTE: Keep this script ASCII-only to stay compatible with Windows PowerShell 5
    # when the file is UTF-8 without BOM. Avoid Cyrillic literals in regex patterns.
    if ($Lines.Count -gt 0 -and $Lines[0] -match '\b(USERNAME|UserName|USER)\b') { $startIndex = 1 }

    for ($i = $startIndex; $i -lt $Lines.Count; $i++) {
        $line = $Lines[$i].Trim()
        if (-not $line) { continue }

        $m = [regex]::Match($line, '^\s*(?<user>\S+)\s+(?<sess>\S+)?\s+(?<id>\d+)\s+(?<state>\S+)', [System.Text.RegularExpressions.RegexOptions]::None)
        if ($m.Success) {
            $user = $m.Groups['user'].Value; $sess = $m.Groups['sess'].Value; $id = [int]$m.Groups['id'].Value; $state = $m.Groups['state'].Value
        }
        else {
            $parts = $line -split '\s+'
            if ($parts.Count -lt 4) { continue }
            $user = $parts[0]
            if ($parts[1] -match '^\d+$') { $sess = ''; $id = [int]$parts[1]; $state = $parts[2] } else { $sess = $parts[1]; $id = [int]$parts[2]; $state = $parts[3] }
        }

        $records += [pscustomobject]@{ username=$user; sessionName=$sess; sessionId=$id; state=$state }
    }
    return $records
}

function Test-SessionIsActive {
    param([string]$State)
    if (-not $State) { return $false }
    $s = $State.Trim().ToLowerInvariant()
    # Match English "active" and Russian "актив*" without embedding Cyrillic.
    # "актив" = \u0430\u043A\u0442\u0438\u0432
    return ($s -match 'active') -or ($s -match '\u0430\u043a\u0442\u0438\u0432')
}

# Main
$cfg = Get-Config -Path $ConfigPath
$hostValue = if ($Hostname -and $Hostname.Trim()) { $Hostname.Trim() } elseif ($cfg -and $cfg.PSObject.Properties.Name -contains 'awHostname' -and -not [string]::IsNullOrWhiteSpace([string]$cfg.awHostname)) { [string]$cfg.awHostname } elseif ($cfg -and $cfg.awHostname) { [string]$cfg.awHostname } else { [string]$env:COMPUTERNAME }
try { $apiBase = '{0}://{1}:{2}/api/0' -f [string]$cfg.server.scheme, [string]$cfg.server.host, [string]$cfg.server.port } catch { throw 'Invalid server configuration in config file.' }

$bucketId = 'aw-worktime-sessions_' + $hostValue
$pulse = 120
$sleepSec = if ($PollSeconds -gt 0) { $PollSeconds } elseif ($cfg.collector -and $cfg.collector.pollSeconds) { [int]$cfg.collector.pollSeconds } else { 30 }

Ensure-Bucket -ApiBase $apiBase -BucketId $bucketId -HostnameValue $hostValue

while ($true) {
    $now = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
    try {
        $lines = Run-QueryUser
        $records = Parse-SessionLines -Lines $lines
    }
    catch {
        Write-Verbose "Session parse error: $($_.Exception.Message)"
        $records = @()
    }

    if (-not $records -or $records.Count -eq 0) {
        $records = @([pscustomobject]@{ username=$env:USERNAME; sessionName=''; sessionId=(Get-Process -Id $PID).SessionId; state='Unknown' })
    }

    foreach ($rec in $records) {
        $payloadObj = [PSCustomObject]@{
            timestamp = $now
            duration  = 0
            data      = [PSCustomObject]@{
                username    = [string]$rec.username
                userId      = "${env:USERDOMAIN}\$($rec.username)"
                sessionId   = [int]$rec.sessionId
                sessionName = [string]$rec.sessionName
                state       = [string]$rec.state
                active      = Test-SessionIsActive -State ([string]$rec.state)
                hostname    = $hostValue
                source      = 'worktime-session-collector'
            }
        }

        $payload = $payloadObj | ConvertTo-Json -Depth 6 -Compress

        try {
            $ok = Invoke-AwJsonPost -Uri "$apiBase/buckets/$bucketId/heartbeat?pulsetime=$pulse" -Json $payload
            if (-not $ok) { Write-Verbose "Heartbeat not confirmed for user $($rec.username)" }
        }
        catch {
            Write-Verbose "Heartbeat error: $($_.Exception.Message)"
        }
    }

    Start-Sleep -Seconds $sleepSec
}
