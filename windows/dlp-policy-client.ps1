[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Invoke-DlpPolicyGetJson {
    param(
        [Parameter(Mandatory = $true)][string]$Uri,
        [int]$TimeoutSec = 10
    )

    $request = [System.Net.HttpWebRequest]::Create($Uri)
    $request.Method = 'GET'
    $request.Accept = 'application/json'
    $request.KeepAlive = $false
    $request.Timeout = $TimeoutSec * 1000
    $request.ReadWriteTimeout = $TimeoutSec * 1000

    $response = $request.GetResponse()
    try {
        $stream = $response.GetResponseStream()
        $reader = New-Object System.IO.StreamReader($stream, [System.Text.Encoding]::UTF8)
        try {
            $reader.ReadToEnd() | ConvertFrom-Json
        }
        finally {
            $reader.Close()
        }
    }
    finally {
        $response.Close()
    }
}

function Get-RemoteDlpPolicyBundle {
    param(
        [Parameter(Mandatory = $true)][string]$ApiBase,
        [int]$TimeoutSec = 10
    )

    $bundle = Invoke-DlpPolicyGetJson -Uri ($ApiBase.TrimEnd('/') + '/dlp/policies/active') -TimeoutSec $TimeoutSec
    if (-not $bundle) {
        throw 'Policy engine returned empty response.'
    }
    if (-not $bundle.active) {
        throw 'Policy engine has no active policy.'
    }
    if (-not $bundle.policy) {
        throw 'Policy engine response has no policy payload.'
    }
    return $bundle
}

function Read-CachedDlpPolicyBundle {
    param([Parameter(Mandatory = $true)][string]$CachePath)

    if (-not (Test-Path -LiteralPath $CachePath)) {
        return $null
    }

    try {
        return Get-Content -LiteralPath $CachePath -Raw | ConvertFrom-Json
    }
    catch {
        return $null
    }
}

function Save-CachedDlpPolicyBundle {
    param(
        [Parameter(Mandatory = $true)]$Bundle,
        [Parameter(Mandatory = $true)][string]$CachePath
    )

    $directory = Split-Path -Path $CachePath -Parent
    if ($directory -and -not (Test-Path -LiteralPath $directory)) {
        New-Item -Path $directory -ItemType Directory -Force | Out-Null
    }

    $json = $Bundle | ConvertTo-Json -Depth 20
    Set-Content -LiteralPath $CachePath -Value $json -Encoding UTF8
}

Export-ModuleMember -Function Invoke-DlpPolicyGetJson, Get-RemoteDlpPolicyBundle, Read-CachedDlpPolicyBundle, Save-CachedDlpPolicyBundle
