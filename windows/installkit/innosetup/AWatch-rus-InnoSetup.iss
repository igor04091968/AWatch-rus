#define MyAppName "AWatch-rus InstallKit"
#define MyAppVersion "1.0.0"
#define MyAppPublisher "AWatch-rus"

[Setup]
AppId={{6D6A1F74-0F4F-4A57-B5E3-1C2C2F56C0E9}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\AWatch-rus
DefaultGroupName=AWatch-rus
OutputDir=.
OutputBaseFilename=AWatch-rus-InstallKit-offline
Compression=lzma
SolidCompression=yes
ArchitecturesInstallIn64BitMode=x64
PrivilegesRequired=admin

[Languages]
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"

[Files]
Source: "..\..\ActivityWatch.Windows.Common.psd1"; DestDir: "{app}\windows"; Flags: ignoreversion
Source: "..\..\ActivityWatch.Windows.Common.psm1"; DestDir: "{app}\windows"; Flags: ignoreversion
Source: "..\..\deploy-single-user.ps1"; DestDir: "{app}\windows"; Flags: ignoreversion
Source: "..\..\deploy-domain-users.ps1"; DestDir: "{app}\windows"; Flags: ignoreversion
Source: "..\..\deploy-ensemble.ps1"; DestDir: "{app}\windows"; Flags: ignoreversion
Source: "..\..\hardening-recovery.ps1"; DestDir: "{app}\windows"; Flags: ignoreversion
Source: "..\..\validate-deployment.ps1"; DestDir: "{app}\windows"; Flags: ignoreversion
Source: "..\..\worktime-session-collector.ps1"; DestDir: "{app}\windows"; Flags: ignoreversion
Source: "..\..\browser-domains-native-collector.ps1"; DestDir: "{app}\windows"; Flags: ignoreversion
Source: "..\..\dlp-endpoint-signals-collector.ps1"; DestDir: "{app}\windows"; Flags: ignoreversion
Source: "..\..\web-category-rules.example.json"; DestDir: "{app}\windows"; Flags: ignoreversion
Source: "..\..\dlp-policy.example.json"; DestDir: "{app}\windows"; Flags: ignoreversion
Source: "payload\activitywatch-v0.13.2-windows-x86_64.zip"; DestDir: "{app}\payload"; Flags: ignoreversion
Source: "innosetup-rdp-package-filelist.md"; DestDir: "{app}\windows\installkit\innosetup"; Flags: ignoreversion

[Run]
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File \"{app}\windows\deploy-ensemble.ps1\" -PackageZipPath \"{app}\payload\activitywatch-v0.13.2-windows-x86_64.zip\""; Flags: runhidden
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File \"{app}\windows\validate-deployment.ps1\""; Flags: runhidden
