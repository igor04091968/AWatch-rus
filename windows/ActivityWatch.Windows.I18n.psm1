<#
.SYNOPSIS
    PowerShell i18n module with JSON-based localization and fallback mechanism.
.DESCRIPTION
    Provides internationalization support for PowerShell scripts with:
    - JSON-based message catalogs
    - Automatic fallback to default language
    - Parameterized messages with format placeholders
    - Auto-versioning support
.EXAMPLE
    Import-Module ./ActivityWatch.Windows.I18n.psm1
    Initialize-Locale -Culture "ru-RU"
    Get-LocalizedString -Key "errors.admin_required"
#>

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Module state
$Script:I18nState = @{
    CurrentCulture = 'en-US'
    FallbackCulture = 'en-US'
    Messages = @{}
    FallbackMessages = @{}
    I18nRoot = $PSScriptRoot + '\..\i18n'
}

function Get-I18nFilePath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Culture,
        [string]$I18nRoot = $Script:I18nState.I18nRoot
    )
    
    $fileName = "{0}.json" -f $Culture
    return Join-Path -Path $I18nRoot -ChildPath $fileName
}

function Test-I18nFileExists {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Culture,
        [string]$I18nRoot = $Script:I18nState.I18nRoot
    )
    
    $filePath = Get-I18nFilePath -Culture $Culture -I18nRoot $I18nRoot
    return Test-Path -LiteralPath $filePath
}

function Load-MessagesForCulture {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Culture,
        [string]$I18nRoot = $Script:I18nState.I18nRoot
    )
    
    $filePath = Get-I18nFilePath -Culture $Culture -I18nRoot $I18nRoot
    
    if (-not (Test-Path -LiteralPath $filePath)) {
        throw "Localization file not found: $filePath"
    }
    
    $content = Get-Content -LiteralPath $filePath -Raw -Encoding UTF8
    $catalog = $content | ConvertFrom-Json
    
    return @{
        Version = $catalog.version
        Language = $catalog.language
        Fallback = $catalog.fallback
        Messages = $catalog.messages
    }
}

function Initialize-Locale {
    param(
        [string]$Culture = 'ru-RU',
        [string]$FallbackCulture = 'en-US',
        [string]$I18nRoot = $Script:I18nState.I18nRoot,
        [switch]$AutoDetect
    )
    
    if ($AutoDetect) {
        $Culture = (Get-Culture).Name
        Write-Host "Auto-detected culture: $Culture" -ForegroundColor Cyan
    }
    
    $Script:I18nState.CurrentCulture = $Culture
    $Script:I18nState.FallbackCulture = $FallbackCulture
    
    try {
        $primaryCatalog = Load-MessagesForCulture -Culture $Culture -I18nRoot $I18nRoot
        $Script:I18nState.Messages = $primaryCatalog.Messages
        
        if ($primaryCatalog.Fallback) {
            $fallbackCatalog = Load-MessagesForCulture -Culture $primaryCatalog.Fallback -I18nRoot $I18nRoot
            $Script:I18nState.FallbackMessages = $fallbackCatalog.Messages
        }
        
        Write-Host "Locale initialized: $Culture (fallback: $($primaryCatalog.Fallback ?? $FallbackCulture))" -ForegroundColor Green
        return $true
    }
    catch {
        Write-Warning "Failed to load primary locale '$Culture'. Attempting fallback..."
        
        try {
            $fallbackCatalog = Load-MessagesForCulture -Culture $FallbackCulture -I18nRoot $I18nRoot
            $Script:I18nState.Messages = @{}
            $Script:I18nState.FallbackMessages = $fallbackCatalog.Messages
            Write-Host "Using fallback locale only: $FallbackCulture" -ForegroundColor Yellow
            return $true
        }
        catch {
            Write-Error "Failed to load both primary and fallback locales."
            return $false
        }
    }
}

function Get-LocalizedString {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Key,
        [object[]]$FormatArgs = @(),
        [string]$DefaultValue
    )
    
    $message = $null
    
    if ($Script:I18nState.Messages.ContainsKey($Key)) {
        $message = $Script:I18nState.Messages[$Key]
    }
    elseif ($Script:I18nState.FallbackMessages.ContainsKey($Key)) {
        $message = $Script:I18nState.FallbackMessages[$Key]
    }
    elseif ($DefaultValue) {
        $message = $DefaultValue
    }
    else {
        $message = "[MISSING: $Key]"
    }
    
    if ($FormatArgs -and $FormatArgs.Count -gt 0) {
        try {
            $message = [string]::Format($message, $FormatArgs)
        }
        catch {
            Write-Warning "Failed to format message '$Key' with args: $($FormatArgs -join ', ')"
        }
    }
    
    return $message
}

function Get-LocalizedError {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Key,
        [object[]]$FormatArgs = @()
    )
    
    $message = Get-LocalizedString -Key "errors.$Key" -FormatArgs $FormatArgs
    return New-Object System.Management.Automation.ErrorRecord(
        (New-Object Exception($message)),
        $Key,
        [System.Management.Automation.ErrorCategory]::OperationStopped,
        $null
    )
}

function Get-LocalizedWarning {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Key,
        [object[]]$FormatArgs = @()
    )
    
    $message = Get-LocalizedString -Key "warnings.$Key" -FormatArgs $FormatArgs
    Write-Warning -Message $message
}

function Get-LocalizedInfo {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Key,
        [object[]]$FormatArgs = @(),
        [ConsoleColor]$Color = 'Cyan'
    )
    
    $message = Get-LocalizedString -Key "info.$Key" -FormatArgs $FormatArgs
    Write-Host -Message $message -ForegroundColor $Color
}

function Get-LocalizedStatus {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Key,
        [object[]]$FormatArgs = @()
    )
    
    return Get-LocalizedString -Key "status.$Key" -FormatArgs $FormatArgs
}

function Get-LocalizedPrompt {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Key,
        [object[]]$FormatArgs = @()
    )
    
    return Get-LocalizedString -Key "prompts.$Key" -FormatArgs $FormatArgs
}

function Read-LocalizedChoice {
    param(
        [Parameter(Mandatory = $true)]
        [string]$PromptKey,
        [string[]]$Choices,
        [object[]]$FormatArgs = @(),
        [int]$DefaultChoice = 0
    )
    
    $promptMessage = Get-LocalizedPrompt -Key $PromptKey -FormatArgs $FormatArgs
    $choiceMessages = $Choices | ForEach-Object {
        Get-LocalizedString -Key "choices.$_"
    }
    
    $formattedChoices = for ($i = 0; $i -lt $Choices.Count; $i++) {
        "[{0}] {1}" -f ($i + 1), $choiceMessages[$i]
    }
    
    $fullPrompt = "{0}`n{1}" -f $promptMessage, ($formattedChoices -join "`n")
    
    $result = Read-Host -Prompt $fullPrompt
    
    if ([string]::IsNullOrWhiteSpace($result)) {
        return $DefaultChoice
    }
    
    $selectedIndex = 0
    if ([int]::TryParse($result, [ref]$selectedIndex) -and $selectedIndex -gt 0 -and $selectedIndex -le $Choices.Count) {
        return $selectedIndex - 1
    }
    
    return $DefaultChoice
}

function Read-LocalizedConfirm {
    param(
        [Parameter(Mandatory = $true)]
        [string]$PromptKey,
        [object[]]$FormatArgs = @(),
        [switch]$Force
    )
    
    if ($Force) {
        return $true
    }
    
    $promptMessage = Get-LocalizedPrompt -Key $PromptKey -FormatArgs $FormatArgs
    $yesMessage = Get-LocalizedString -Key "choices.yes" -DefaultValue "Yes"
    $noMessage = Get-LocalizedString -Key "choices.no" -DefaultValue "No"
    
    $result = Read-Host -Prompt "$promptMessage ($yesMessage/$noMessage)"
    
    return $result -in @('y', 'Y', 'yes', 'Yes', $yesMessage)
}

function Get-AvailableLocales {
    param(
        [string]$I18nRoot = $Script:I18nState.I18nRoot
    )
    
    if (-not (Test-Path -LiteralPath $I18nRoot)) {
        return @()
    }
    
    $locales = Get-ChildItem -Path $I18nRoot -Filter "*.json" -File | ForEach-Object {
        $culture = $_.BaseName
        try {
            $catalog = Load-MessagesForCulture -Culture $culture -I18nRoot $I18nRoot
            [PSCustomObject]@{
                Culture = $culture
                Language = $catalog.Language
                Version = $catalog.Version
                HasFallback = [bool]$catalog.Fallback
            }
        }
        catch {
            Write-Warning "Failed to load locale $culture : $_"
        }
    }
    
    return $locales
}

function Get-I18nVersion {
    param(
        [string]$Culture = $Script:I18nState.CurrentCulture,
        [string]$I18nRoot = $Script:I18nState.I18nRoot
    )
    
    try {
        $catalog = Load-MessagesForCulture -Culture $Culture -I18nRoot $I18nRoot
        return $catalog.Version
    }
    catch {
        return $null
    }
}

function Test-I18nUpdateAvailable {
    param(
        [string]$CurrentVersion,
        [string]$Culture = $Script:I18nState.CurrentCulture,
        [string]$I18nRoot = $Script:I18nState.I18nRoot
    )
    
    $availableVersion = Get-I18nVersion -Culture $Culture -I18nRoot $I18nRoot
    
    if (-not $CurrentVersion -or -not $availableVersion) {
        return $false
    }
    
    try {
        $current = [Version]$CurrentVersion
        $available = [Version]$availableVersion
        return $available -gt $current
    }
    catch {
        return $false
    }
}

function Export-LocaleTemplate {
    param(
        [Parameter(Mandatory = $true)]
        [string]$OutputPath,
        [string]$SourceCulture = 'en-US'
    )
    
    $catalog = Load-MessagesForCulture -Culture $SourceCulture
    
    $template = [PSCustomObject]@{
        version = "1.0.0"
        language = $catalog.Language
        fallback = $null
        messages = $catalog.Messages
    }
    
    $directory = Split-Path -Path $OutputPath -Parent
    if ($directory -and -not (Test-Path -LiteralPath $directory)) {
        New-Item -Path $directory -ItemType Directory -Force | Out-Null
    }
    
    $template | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $OutputPath -Encoding UTF8
    Write-Host "Locale template exported to: $OutputPath" -ForegroundColor Green
}

function Compare-Locales {
    param(
        [string]$Culture1 = 'en-US',
        [string]$Culture2 = 'ru-RU',
        [string]$I18nRoot = $Script:I18nState.I18nRoot
    )
    
    $catalog1 = Load-MessagesForCulture -Culture $Culture1 -I18nRoot $I18nRoot
    $catalog2 = Load-MessagesForCulture -Culture $Culture2 -I18nRoot $I18nRoot
    
    $keys1 = $catalog1.Messages.Keys
    $keys2 = $catalog2.Messages.Keys
    
    $missing = $keys1 | Where-Object { $_ -notin $keys2 }
    $extra = $keys2 | Where-Object { $_ -notin $keys1 }
    
    return [PSCustomObject]@{
        Culture1 = $Culture1
        Culture2 = $Culture2
        KeysInCulture1 = $keys1.Count
        KeysInCulture2 = $keys2.Count
        MissingInCulture2 = @($missing)
        ExtraInCulture2 = @($extra)
        CoveragePercent = if ($keys1.Count -gt 0) { 
            [math]::Round((($keys1.Count - $missing.Count) / $keys1.Count) * 100, 2) 
        } else { 0 }
    }
}

Export-ModuleMember -Function @(
    'Initialize-Locale',
    'Get-LocalizedString',
    'Get-LocalizedError',
    'Get-LocalizedWarning',
    'Get-LocalizedInfo',
    'Get-LocalizedStatus',
    'Get-LocalizedPrompt',
    'Read-LocalizedChoice',
    'Read-LocalizedConfirm',
    'Get-AvailableLocales',
    'Get-I18nVersion',
    'Test-I18nUpdateAvailable',
    'Export-LocaleTemplate',
    'Compare-Locales'
)
