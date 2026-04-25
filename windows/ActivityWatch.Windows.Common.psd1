@{
    RootModule = 'ActivityWatch.Windows.Common.psm1'
    ModuleVersion = '1.0.0'
    GUID = '90b3fcf6-df9f-4f9b-9ee0-8a7de4dc0ee2'
    Author = 'igor04091968'
    CompanyName = 'Private'
    Description = 'Common PowerShell functions for ActivityWatch Windows deployment, hardening and recovery.'
    PowerShellVersion = '5.1'
    FunctionsToExport = @(
        '*-ActivityWatch*',
        'Assert-Administrator',
        'Normalize-ActivityWatchUsers',
        'Get-ActivityWatchPackageUrl',
        'Remove-LegacyActivityWatchEntries'
    )
    CmdletsToExport = @()
    VariablesToExport = '*'
    AliasesToExport = @()
    PrivateData = @{
        PSData = @{
            Tags = @('ActivityWatch', 'Windows', 'Deployment', 'Recovery')
            ProjectUri = 'https://github.com/igor04091968/AWatch-rus'
        }
    }
}
