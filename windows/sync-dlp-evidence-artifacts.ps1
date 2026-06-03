param(
    [string]$ConfigPath = 'C:\ProgramData\AWatch-rus\deployment-config.json',
    [string]$EvidenceApiUrl = 'http://aw-server.example.local:8721/api/dlp/evidence/upload',
    [string]$TokenPath = 'C:\ProgramData\AWatch-rus\dlp-evidence-upload-token.txt',
    [string]$StatePath = 'C:\ProgramData\AWatch-rus\dlp-evidence-sync-state.json',
    [string]$LogPath = 'C:\ProgramData\AWatch-rus\logs\dlp-evidence-sync.log',
    [int]$MaxFiles = 200,
    [int]$MaxBytes = 8388608,
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'

function Ensure-Directory {
    param([Parameter(Mandatory = $true)][string]$Path)
    $dir = if ([System.IO.Path]::HasExtension($Path)) { Split-Path -Parent $Path } else { $Path }
    if ($dir -and -not (Test-Path -LiteralPath $dir)) {
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
    }
}

function Write-SyncLog {
    param([string]$Message)
    Ensure-Directory -Path $LogPath
    $line = "{0} {1}" -f ([DateTime]::UtcNow.ToString('o')), $Message
    Add-Content -LiteralPath $LogPath -Value $line -Encoding UTF8
}

function Get-JsonFile {
    param([string]$Path, [object]$Default)
    if (-not (Test-Path -LiteralPath $Path)) { return $Default }
    try {
        return Get-Content -Raw -LiteralPath $Path -Encoding UTF8 | ConvertFrom-Json
    }
    catch {
        Write-SyncLog ("state parse failed: {0}" -f $_.Exception.Message)
        return $Default
    }
}

function Save-JsonFile {
    param([string]$Path, [object]$Value)
    Ensure-Directory -Path $Path
    $tmp = "$Path.tmp"
    $Value | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $tmp -Encoding UTF8
    Move-Item -LiteralPath $tmp -Destination $Path -Force
}

function Get-FileSha256Hex {
    param([Parameter(Mandatory = $true)][string]$Path)
    return ((Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant())
}

function Get-Config {
    if (-not (Test-Path -LiteralPath $ConfigPath)) { return $null }
    try {
        return Get-Content -Raw -LiteralPath $ConfigPath -Encoding UTF8 | ConvertFrom-Json
    }
    catch {
        Write-SyncLog ("config parse failed: {0}" -f $_.Exception.Message)
        return $null
    }
}

function Add-RootIfExists {
    param(
        [System.Collections.Generic.List[string]]$Roots,
        [string]$Path
    )
    if ($Path -and (Test-Path -LiteralPath $Path)) {
        $full = [System.IO.Path]::GetFullPath($Path)
        if (-not $Roots.Contains($full)) {
            $Roots.Add($full)
        }
    }
}

function Get-ArtifactRoots {
    $roots = [System.Collections.Generic.List[string]]::new()
    $config = Get-Config
    if ($config -and $config.PSObject.Properties.Name -contains 'incidentCapture' -and
        $config.incidentCapture.PSObject.Properties.Name -contains 'artifactsRoot') {
        Add-RootIfExists -Roots $roots -Path ([string]$config.incidentCapture.artifactsRoot)
    }
    if ($config -and $config.PSObject.Properties.Name -contains 'paths' -and
        $config.paths.PSObject.Properties.Name -contains 'stateRoot') {
        Add-RootIfExists -Roots $roots -Path (Join-Path ([string]$config.paths.stateRoot) 'incident-artifacts')
    }
    Add-RootIfExists -Roots $roots -Path 'C:\ProgramData\AWatch-rus\incident-artifacts'
    Get-ChildItem -LiteralPath 'C:\Users' -Directory -ErrorAction SilentlyContinue | ForEach-Object {
        Add-RootIfExists -Roots $roots -Path (Join-Path $_.FullName 'AppData\Local\AWatch-rus\incident-artifacts')
    }
    return @($roots)
}

function Get-StateMap {
    $state = Get-JsonFile -Path $StatePath -Default ([pscustomobject]@{ uploaded = @{} })
    if (-not ($state.PSObject.Properties.Name -contains 'uploaded') -or -not $state.uploaded) {
        $state | Add-Member -NotePropertyName uploaded -NotePropertyValue ([pscustomobject]@{}) -Force
    }
    return $state
}

function Test-AlreadyUploaded {
    param(
        [object]$State,
        [string]$Sha256,
        [System.IO.FileInfo]$File
    )
    if (-not ($State.uploaded.PSObject.Properties.Name -contains $Sha256)) { return $false }
    $entry = $State.uploaded.$Sha256
    return (
        [string]$entry.path -eq $File.FullName -and
        [int64]$entry.length -eq [int64]$File.Length -and
        [string]$entry.lastWriteUtc -eq $File.LastWriteTimeUtc.ToString('o')
    )
}

function Set-UploadedState {
    param(
        [object]$State,
        [string]$Sha256,
        [System.IO.FileInfo]$File,
        [object]$Response
    )
    $entry = [pscustomobject]@{
        path = $File.FullName
        length = [int64]$File.Length
        lastWriteUtc = $File.LastWriteTimeUtc.ToString('o')
        uploadedAtUtc = [DateTime]::UtcNow.ToString('o')
        responseStored = [bool]$Response.stored
    }
    $State.uploaded | Add-Member -NotePropertyName $Sha256 -NotePropertyValue $entry -Force
}

function Invoke-EvidenceUpload {
    param(
        [System.IO.FileInfo]$File,
        [string]$Sha256,
        [string]$Token
    )
    $bytes = [System.IO.File]::ReadAllBytes($File.FullName)
    $payload = [pscustomobject]@{
        sha256 = $Sha256
        content_base64 = [Convert]::ToBase64String($bytes)
        content_type = 'image/png'
        source_file = $File.Name
        source_path = $File.FullName
        hostname = $env:COMPUTERNAME
        username = $env:USERNAME
    }
    if ($DryRun) {
        return [pscustomobject]@{ ok = $true; stored = $false; dryRun = $true }
    }
    return Invoke-RestMethod `
        -Method Post `
        -Uri $EvidenceApiUrl `
        -Headers @{ Authorization = "Bearer $Token" } `
        -ContentType 'application/json; charset=utf-8' `
        -Body ($payload | ConvertTo-Json -Depth 5 -Compress) `
        -TimeoutSec 30
}

$result = [ordered]@{
    ok = $true
    dryRun = [bool]$DryRun
    roots = @()
    scanned = 0
    uploaded = 0
    skipped = 0
    failed = 0
    errors = @()
}

try {
    $token = ''
    if (Test-Path -LiteralPath $TokenPath) {
        $token = (Get-Content -Raw -LiteralPath $TokenPath -Encoding UTF8).Trim()
    }
    if (-not $token) {
        throw "upload token is missing: $TokenPath"
    }
    $roots = @(Get-ArtifactRoots)
    $result.roots = $roots
    $state = Get-StateMap
    $files = @()
    foreach ($root in $roots) {
        $files += @(Get-ChildItem -LiteralPath $root -Filter '*.png' -File -Recurse -ErrorAction SilentlyContinue)
    }
    $files = @($files | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First $MaxFiles)
    foreach ($file in $files) {
        try {
            $result.scanned++
            if ($file.Length -le 0 -or $file.Length -gt $MaxBytes) {
                $result.skipped++
                continue
            }
            $sha = Get-FileSha256Hex -Path $file.FullName
            if (Test-AlreadyUploaded -State $state -Sha256 $sha -File $file) {
                $result.skipped++
                continue
            }
            $response = Invoke-EvidenceUpload -File $file -Sha256 $sha -Token $token
            if (-not $response.ok) {
                throw "upload response is not ok"
            }
            Set-UploadedState -State $state -Sha256 $sha -File $file -Response $response
            $result.uploaded++
            Write-SyncLog ("uploaded evidence sha={0} file={1}" -f $sha, $file.FullName)
        }
        catch {
            $result.failed++
            $result.errors += ("{0}: {1}" -f $file.FullName, $_.Exception.Message)
            Write-SyncLog ("upload failed file={0}: {1}" -f $file.FullName, $_.Exception.Message)
        }
    }
    Save-JsonFile -Path $StatePath -Value $state
    if ($result.failed -gt 0) { $result.ok = $false }
}
catch {
    $result.ok = $false
    $result.failed++
    $result.errors += $_.Exception.Message
    Write-SyncLog ("sync failed: {0}" -f $_.Exception.Message)
}

$result | ConvertTo-Json -Depth 6
if (-not $result.ok) { exit 1 }
