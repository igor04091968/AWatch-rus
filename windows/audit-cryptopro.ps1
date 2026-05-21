[CmdletBinding()]
param(
    [string[]]$ExpectedUsers = @(),
    [switch]$IncludeUnexpectedProfiles
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-UninstallEntries {
    param(
        [Parameter(Mandatory = $true)]
        [string]$NamePattern
    )

    $paths = @(
        'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
    )

    return @(
        Get-ItemProperty $paths -ErrorAction SilentlyContinue |
            Where-Object {
                $_.PSObject.Properties.Name -contains 'DisplayName' -and
                -not [string]::IsNullOrWhiteSpace([string]$_.DisplayName) -and
                ([string]$_.DisplayName -match $NamePattern)
            } |
            Select-Object DisplayName, DisplayVersion, Publisher
    )
}

function Get-CryptoProToolPath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$LeafName
    )

    $candidate = Join-Path 'C:\Program Files\Crypto Pro\CSP' $LeafName
    if (Test-Path -LiteralPath $candidate) {
        return $candidate
    }
    return $null
}

function Get-CryptoProStoreMapForCurrentUser {
    param(
        [string]$CertmgrPath
    )

    $map = @{}
    if ([string]::IsNullOrWhiteSpace($CertmgrPath) -or -not (Test-Path -LiteralPath $CertmgrPath)) {
        return $map
    }

    $raw = & $CertmgrPath -list -store uMy 2>&1
    if ($LASTEXITCODE -ne 0) {
        return $map
    }

    $current = @{}
    foreach ($line in @($raw | ForEach-Object { [string]$_ })) {
        $trimmed = $line.Trim()
        if ($trimmed -match '^SHA1 Thumbprint\s*:\s*(.+)$') {
            $current.thumbprint = $Matches[1].Trim().ToUpperInvariant()
            continue
        }
        if ($trimmed -match '^Embedded License\s*:\s*(.+)$') {
            $current.embeddedLicense = $Matches[1].Trim()
            continue
        }
        if ($trimmed -match '^Container\s*:\s*(.+)$') {
            $current.container = $Matches[1].Trim()
            continue
        }
        if ($trimmed -match '^\[ErrorCode:') {
            if ($current.ContainsKey('thumbprint')) {
                $map[$current.thumbprint] = [pscustomobject]@{
                    embeddedLicense = if ($current.ContainsKey('embeddedLicense')) { [string]$current.embeddedLicense } else { $null }
                    container = if ($current.ContainsKey('container')) { [string]$current.container } else { $null }
                }
            }
            $current = @{}
        }
    }

    return $map
}

function Test-CertificateEmbeddedLicense {
    param(
        [Parameter(Mandatory = $true)]
        [System.Security.Cryptography.X509Certificates.X509Certificate2]$Certificate,
        [string]$CspTestPath
    )

    $result = [ordered]@{
        ok = $null
        status = $null
        error = $null
    }

    if ([string]::IsNullOrWhiteSpace($CspTestPath) -or -not (Test-Path -LiteralPath $CspTestPath)) {
        $result.error = 'csptest_missing'
        return [pscustomobject]$result
    }

    $tempPath = Join-Path $env:TEMP ("cryptopro-audit-{0}.cer" -f ([Guid]::NewGuid().ToString('N')))
    try {
        Export-Certificate -Cert $Certificate -FilePath $tempPath -Force | Out-Null
        $raw = & $CspTestPath -certlic -check -certfile $tempPath 2>&1
        if ($LASTEXITCODE -eq 0) {
            $licenseLine = @($raw | Where-Object { [string]$_ -match '^License:\s*' } | Select-Object -First 1)
            if ($licenseLine.Count -gt 0) {
                $statusText = (($licenseLine[0] -replace '^License:\s*', '').Trim())
                $result.status = $statusText
                $result.ok = ($statusText -match '^Good license\b')
            }
            else {
                $result.ok = $false
                $result.error = 'license_line_missing'
            }
        }
        else {
            $result.ok = $false
            $result.error = ('csptest_exit_{0}' -f $LASTEXITCODE)
        }
    }
    catch {
        $result.ok = $false
        $result.error = $_.Exception.Message
    }
    finally {
        Remove-Item -LiteralPath $tempPath -Force -ErrorAction SilentlyContinue
    }

    return [pscustomobject]$result
}

function Get-ProfileMap {
    $profileMap = @{}
    foreach ($profile in @(Get-CimInstance Win32_UserProfile | Where-Object { -not $_.Special })) {
        $leaf = Split-Path -Leaf ([string]$profile.LocalPath)
        if ([string]::IsNullOrWhiteSpace($leaf)) { continue }
        $profileMap[$leaf.ToUpperInvariant()] = [pscustomobject]@{
            user = $leaf
            sid = [string]$profile.SID
            localPath = [string]$profile.LocalPath
            loaded = [bool]$profile.Loaded
        }
    }
    return $profileMap
}

function Get-UserAuditRows {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RequestedUser,
        [pscustomobject]$Profile,
        [string]$CspTestPath,
        [hashtable]$CurrentUserStoreMap,
        [bool]$ExpectedSigner
    )

    if ($null -eq $Profile) {
        return @(
            [pscustomobject]@{
                requestedUser = $RequestedUser
                profileUser = $null
                profilePath = $null
                profileLoaded = $false
                sid = $null
                storeFilePath = $null
                thumbprint = $null
                subject = $null
                issuer = $null
                notAfterUtc = $null
                hasPrivateKey = $false
                embeddedLicenseOk = $null
                embeddedLicenseStatus = $null
                container = $null
                expectedSigner = $ExpectedSigner
                actionNeeded = 'profile_missing_or_no_login'
            }
        )
    }

    $certDir = Join-Path $Profile.localPath 'AppData\Roaming\Microsoft\SystemCertificates\My\Certificates'
    $files = @()
    if (Test-Path -LiteralPath $certDir) {
        $files = @(Get-ChildItem -LiteralPath $certDir -File -ErrorAction SilentlyContinue)
    }
    if ($files.Count -eq 0) {
        return @(
            [pscustomobject]@{
                requestedUser = $RequestedUser
                profileUser = $Profile.user
                profilePath = $Profile.localPath
                profileLoaded = [bool]$Profile.loaded
                sid = $Profile.sid
                storeFilePath = $certDir
                thumbprint = $null
                subject = $null
                issuer = $null
                notAfterUtc = $null
                hasPrivateKey = $false
                embeddedLicenseOk = $null
                embeddedLicenseStatus = $null
                container = $null
                expectedSigner = $ExpectedSigner
                actionNeeded = 'certificate_missing'
            }
        )
    }

    $rows = @()
    foreach ($file in $files) {
        try {
            $cert = Get-PfxCertificate -FilePath $file.FullName
            $thumbprint = [string]$cert.Thumbprint
            $license = Test-CertificateEmbeddedLicense -Certificate $cert -CspTestPath $CspTestPath
            $storeMeta = $null
            if ($CurrentUserStoreMap.ContainsKey($thumbprint.ToUpperInvariant())) {
                $storeMeta = $CurrentUserStoreMap[$thumbprint.ToUpperInvariant()]
            }

            $actionNeeded = 'manual_review'
            if ($cert.NotAfter -lt (Get-Date)) {
                $actionNeeded = 'renew_certificate'
            }
            elseif (-not [bool]$cert.HasPrivateKey) {
                $actionNeeded = 'attach_private_key_or_token'
            }
            elseif ($license.ok -eq $true) {
                $actionNeeded = 'ready'
            }
            elseif ($license.ok -eq $false) {
                $actionNeeded = 'embedded_license_missing_or_invalid'
            }

            $rows += [pscustomobject]@{
                requestedUser = $RequestedUser
                profileUser = $Profile.user
                profilePath = $Profile.localPath
                profileLoaded = [bool]$Profile.loaded
                sid = $Profile.sid
                storeFilePath = $file.FullName
                thumbprint = $thumbprint
                subject = [string]$cert.Subject
                issuer = [string]$cert.Issuer
                notAfterUtc = $cert.NotAfter.ToUniversalTime().ToString('o')
                hasPrivateKey = [bool]$cert.HasPrivateKey
                embeddedLicenseOk = $license.ok
                embeddedLicenseStatus = $license.status
                container = if ($null -ne $storeMeta) { [string]$storeMeta.container } else { $null }
                expectedSigner = $ExpectedSigner
                actionNeeded = $actionNeeded
            }
        }
        catch {
            $rows += [pscustomobject]@{
                requestedUser = $RequestedUser
                profileUser = $Profile.user
                profilePath = $Profile.localPath
                profileLoaded = [bool]$Profile.loaded
                sid = $Profile.sid
                storeFilePath = $file.FullName
                thumbprint = $null
                subject = $null
                issuer = $null
                notAfterUtc = $null
                hasPrivateKey = $false
                embeddedLicenseOk = $null
                embeddedLicenseStatus = $null
                container = $null
                expectedSigner = $ExpectedSigner
                actionNeeded = ('certificate_parse_error: {0}' -f $_.Exception.Message)
            }
        }
    }

    return $rows
}

$cspEntries = @(Get-UninstallEntries -NamePattern 'CryptoPro|КриптоПро|Крипто')
$cspMain = @($cspEntries | Where-Object { $_.DisplayName -match 'CSP' } | Select-Object -First 1)
$browserPlugin = @($cspEntries | Where-Object { $_.DisplayName -match 'Browser' } | Select-Object -First 1)
$cspTestPath = Get-CryptoProToolPath -LeafName 'csptest.exe'
$certmgrPath = Get-CryptoProToolPath -LeafName 'certmgr.exe'
$currentUserStoreMap = Get-CryptoProStoreMapForCurrentUser -CertmgrPath $certmgrPath
$profileMap = Get-ProfileMap

$targets = New-Object 'System.Collections.Generic.List[string]'
foreach ($expected in @($ExpectedUsers)) {
    if (-not [string]::IsNullOrWhiteSpace([string]$expected)) {
        $targets.Add([string]$expected)
    }
}
if ($IncludeUnexpectedProfiles.IsPresent -or $targets.Count -eq 0) {
    foreach ($profileKey in @($profileMap.Keys | Sort-Object)) {
        $targets.Add([string]$profileMap[$profileKey].user)
    }
}

$seen = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
$rows = @()
foreach ($target in $targets) {
    if (-not $seen.Add($target)) { continue }
    $key = $target.ToUpperInvariant()
    $profile = if ($profileMap.ContainsKey($key)) { $profileMap[$key] } else { $null }
    $rows += Get-UserAuditRows -RequestedUser $target -Profile $profile -CspTestPath $cspTestPath -CurrentUserStoreMap $currentUserStoreMap -ExpectedSigner:([bool]($ExpectedUsers -contains $target))
}

$summary = [ordered]@{
    expectedSignerCount = [int](@($ExpectedUsers | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) }).Count)
    readyCount = [int](@($rows | Where-Object { $_.actionNeeded -eq 'ready' }).Count)
    missingProfileCount = [int](@($rows | Where-Object { $_.actionNeeded -eq 'profile_missing_or_no_login' }).Count)
    missingCertificateCount = [int](@($rows | Where-Object { $_.actionNeeded -eq 'certificate_missing' }).Count)
    noPrivateKeyCount = [int](@($rows | Where-Object { $_.actionNeeded -eq 'attach_private_key_or_token' }).Count)
    licenseProblemCount = [int](@($rows | Where-Object { $_.actionNeeded -eq 'embedded_license_missing_or_invalid' }).Count)
    renewalNeededCount = [int](@($rows | Where-Object { $_.actionNeeded -eq 'renew_certificate' }).Count)
}

[pscustomobject]@{
    generatedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
    computerName = [string]$env:COMPUTERNAME
    currentUser = [string]$env:USERNAME
    cryptopro = [ordered]@{
        cspInstalled = ($null -ne $cspMain)
        cspVersion = if ($null -ne $cspMain) { [string]$cspMain.DisplayVersion } else { $null }
        browserPluginInstalled = ($null -ne $browserPlugin)
        browserPluginVersion = if ($null -ne $browserPlugin) { [string]$browserPlugin.DisplayVersion } else { $null }
        cspTestPath = $cspTestPath
        certmgrPath = $certmgrPath
    }
    expectedUsers = @($ExpectedUsers)
    signers = @($rows)
    summary = $summary
}
