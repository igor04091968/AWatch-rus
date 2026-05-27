$script:DetMirConfigPath = if ($env:DETMIR_WINDOWS_CONFIG_PATH) {
    $env:DETMIR_WINDOWS_CONFIG_PATH
}
else {
    Join-Path $HOME '.config/powershell/detmir-windows.psd1'
}

$script:DetMirConfig = @{}

if (Test-Path -LiteralPath $script:DetMirConfigPath) {
    try {
        $script:DetMirConfig = Import-PowerShellDataFile -Path $script:DetMirConfigPath
    }
    catch {
        $script:DetMirConfig = @{}
    }
}

function Get-DetMirWindowsSetting {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$EnvName,

        [Parameter(Mandatory)]
        [string]$ConfigKey,

        [string]$Default = ''
    )

    $envValue = [Environment]::GetEnvironmentVariable($EnvName)
    if (-not [string]::IsNullOrWhiteSpace($envValue)) {
        return $envValue
    }

    if ($script:DetMirConfig.ContainsKey($ConfigKey) -and -not [string]::IsNullOrWhiteSpace([string]$script:DetMirConfig[$ConfigKey])) {
        return [string]$script:DetMirConfig[$ConfigKey]
    }

    return $Default
}

$Global:DetMirWindowsHost = Get-DetMirWindowsSetting -EnvName 'DETMIR_WINDOWS_SSH_HOST' -ConfigKey 'Host' -Default '192.168.100.18'
$Global:DetMirWindowsUser = Get-DetMirWindowsSetting -EnvName 'DETMIR_WINDOWS_SSH_USER' -ConfigKey 'User' -Default 'Администратор'
$Global:DetMirWindowsPassword = Get-DetMirWindowsSetting -EnvName 'DETMIR_WINDOWS_SSH_PASSWORD' -ConfigKey 'Password'
$Global:DetMirWindowsPort = [int](Get-DetMirWindowsSetting -EnvName 'DETMIR_WINDOWS_SSH_PORT' -ConfigKey 'Port' -Default '22')
$Global:DetMirWindowsPowerShellPath = Get-DetMirWindowsSetting -EnvName 'DETMIR_WINDOWS_POWERSHELL_PATH' -ConfigKey 'PowerShellPath' -Default 'powershell.exe'

function Get-DetMirWindowsSshTarget {
    [CmdletBinding()]
    param()

    if ([string]::IsNullOrWhiteSpace($Global:DetMirWindowsUser)) {
        return $Global:DetMirWindowsHost
    }

    return "$($Global:DetMirWindowsUser)@$($Global:DetMirWindowsHost)"
}

function Get-DetMirWindowsSshArguments {
    [CmdletBinding()]
    param()

    return @(
        '-o', 'ServerAliveInterval=15',
        '-o', 'ServerAliveCountMax=3',
        '-o', 'StrictHostKeyChecking=accept-new',
        '-o', 'LogLevel=ERROR',
        '-o', 'PreferredAuthentications=password,keyboard-interactive,publickey',
        '-p', [string]$Global:DetMirWindowsPort,
        (Get-DetMirWindowsSshTarget)
    )
}

function Invoke-DetMirWindowsSsh {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$Command
    )

    $sshArgs = Get-DetMirWindowsSshArguments
    $sshArgs += $Command

    if ([string]::IsNullOrWhiteSpace($Global:DetMirWindowsPassword)) {
        & ssh @sshArgs
        return
    }

    & sshpass '-p' $Global:DetMirWindowsPassword ssh @sshArgs
}

function Enter-DetMirWindowsSsh {
    [CmdletBinding()]
    param()

    $sshArgs = Get-DetMirWindowsSshArguments

    if ([string]::IsNullOrWhiteSpace($Global:DetMirWindowsPassword)) {
        & ssh @sshArgs
        return
    }

    & sshpass '-p' $Global:DetMirWindowsPassword ssh @sshArgs
}

function Invoke-DetMirWindowsPowerShell {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$Script
    )

    $bootstrap = @"
[Console]::InputEncoding = [System.Text.Encoding]::UTF8
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
`$OutputEncoding = [System.Text.Encoding]::UTF8
$Script
"@

    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($bootstrap))
    Invoke-DetMirWindowsSsh "$($Global:DetMirWindowsPowerShellPath) -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand $encoded"
}

function Test-DetMirWindowsPowerShell {
    [CmdletBinding()]
    param()

    Invoke-DetMirWindowsPowerShell @'
$PSVersionTable.PSVersion.ToString()
[Environment]::OSVersion.VersionString
(Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion').ProductName
'@
}

Set-Alias detmir-win-target Get-DetMirWindowsSshTarget
Set-Alias detmir-win-ssh Invoke-DetMirWindowsSsh
Set-Alias detmir-win-shell Enter-DetMirWindowsSsh
Set-Alias detmir-win-ps Invoke-DetMirWindowsPowerShell
Set-Alias detmir-win-test Test-DetMirWindowsPowerShell
