#define MyAppName "AWatch-rus Windows Agent"
#define MyAppVersion "1.0.0"
#define MyAppPublisher "AWatch-rus"
#define MyAppURL "https://example.local/awatch-rus"
#define MyAppExeName "AWatch-rus-Setup.exe"

[Setup]
AppId={{7F2C4B63-2B8A-4F1C-BA12-66A7E2C0A0A1}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\AWatch-rus
DefaultGroupName=AWatch-rus
DisableProgramGroupPage=yes
PrivilegesRequired=admin
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=output
OutputBaseFilename={#MyAppExeName}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
SetupLogging=yes
DisableWelcomePage=no
AllowNoIcons=yes
UninstallDisplayIcon={app}\tools\validate-deployment.ps1

[Languages]
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\ActivityWatch.Windows.Common.psd1"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\ActivityWatch.Windows.Common.psm1"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\browser-domains-native-collector.ps1"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\dlp-endpoint-signals-collector.ps1"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\deploy-single-user.ps1"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\deploy-domain-users.ps1"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\deploy-ensemble.ps1"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\hardening-recovery.ps1"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\validate-deployment.ps1"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\web-category-rules.example.json"; DestDir: "{app}\config"; Flags: ignoreversion
Source: "..\dlp-policy.example.json"; DestDir: "{app}\config"; Flags: ignoreversion
Source: "README-INSTALLER.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "Invoke-AWatchRusInstall.ps1"; DestDir: "{app}\tools"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\AWatch-rus\Run validation"; Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\tools\validate-deployment.ps1"" -ConfigPath ""C:\ProgramData\ActivityWatch\deployment-config.json"""; WorkingDir: "{app}\tools"
Name: "{autodesktop}\AWatch-rus Validation"; Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\tools\validate-deployment.ps1"" -ConfigPath ""C:\ProgramData\ActivityWatch\deployment-config.json"""; Tasks: desktopicon

[Run]
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\tools\Invoke-AWatchRusInstall.ps1"" -ServerHost ""{code:GetServerHost}"" -ServerPort {code:GetServerPort} -Domain ""{code:GetDomain}"" -Users ""{code:GetUsers}"" -InstallRoot ""{code:GetInstallRoot}"" -StateRoot ""{code:GetStateRoot}"" -CustomRulesPath ""{app}\config\web-category-rules.example.json"" -CustomPolicyPath ""{app}\config\dlp-policy.example.json"" -AfkEnabled:{code:GetAfkEnabled} -WindowEnabled:{code:GetWindowEnabled}"; Flags: runhidden waituntilterminated
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\tools\validate-deployment.ps1"" -ConfigPath ""{code:GetStateRoot}\deployment-config.json"""; Flags: postinstall shellexec skipifsilent

[UninstallRun]
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -Command ""Get-ScheduledTask -TaskName 'ActivityWatch Launch *' -ErrorAction SilentlyContinue | ForEach-Object { Unregister-ScheduledTask -TaskName $_.TaskName -Confirm:$false }; Unregister-ScheduledTask -TaskName 'ActivityWatch Recovery' -Confirm:$false -ErrorAction SilentlyContinue"""; Flags: runhidden waituntilterminated

[Code]
var
  ConfigPage: TWizardPage;
  ServerHostEdit: TEdit;
  ServerPortEdit: TEdit;
  DomainEdit: TEdit;
  UsersEdit: TEdit;
  InstallRootEdit: TEdit;
  StateRootEdit: TEdit;
  AfkCheck: TNewCheckBox;
  WindowCheck: TNewCheckBox;

procedure InitializeWizard;
begin
  ConfigPage := CreateCustomPage(wpSelectTasks,
    'AWatch-rus configuration',
    'Specify deployment settings passed to deploy-ensemble.ps1');

  ServerHostEdit := TEdit.Create(ConfigPage);
  ServerHostEdit.Parent := ConfigPage.Surface;
  ServerHostEdit.Left := ScaleX(0);
  ServerHostEdit.Top := ScaleY(8);
  ServerHostEdit.Width := ScaleX(420);
  ServerHostEdit.Text := 'aw.example.local';

  ServerPortEdit := TEdit.Create(ConfigPage);
  ServerPortEdit.Parent := ConfigPage.Surface;
  ServerPortEdit.Left := ScaleX(0);
  ServerPortEdit.Top := ScaleY(40);
  ServerPortEdit.Width := ScaleX(120);
  ServerPortEdit.Text := '5600';

  DomainEdit := TEdit.Create(ConfigPage);
  DomainEdit.Parent := ConfigPage.Surface;
  DomainEdit.Left := ScaleX(0);
  DomainEdit.Top := ScaleY(72);
  DomainEdit.Width := ScaleX(260);
  DomainEdit.Text := 'CONTOSO';

  UsersEdit := TEdit.Create(ConfigPage);
  UsersEdit.Parent := ConfigPage.Surface;
  UsersEdit.Left := ScaleX(0);
  UsersEdit.Top := ScaleY(104);
  UsersEdit.Width := ScaleX(420);
  UsersEdit.Text := 'user1,user2';

  InstallRootEdit := TEdit.Create(ConfigPage);
  InstallRootEdit.Parent := ConfigPage.Surface;
  InstallRootEdit.Left := ScaleX(0);
  InstallRootEdit.Top := ScaleY(136);
  InstallRootEdit.Width := ScaleX(420);
  InstallRootEdit.Text := 'C:\Program Files\ActivityWatch';

  StateRootEdit := TEdit.Create(ConfigPage);
  StateRootEdit.Parent := ConfigPage.Surface;
  StateRootEdit.Left := ScaleX(0);
  StateRootEdit.Top := ScaleY(168);
  StateRootEdit.Width := ScaleX(420);
  StateRootEdit.Text := 'C:\ProgramData\ActivityWatch';

  AfkCheck := TNewCheckBox.Create(ConfigPage);
  AfkCheck.Parent := ConfigPage.Surface;
  AfkCheck.Top := ScaleY(200);
  AfkCheck.Caption := 'Enable AFK watcher';
  AfkCheck.Checked := True;

  WindowCheck := TNewCheckBox.Create(ConfigPage);
  WindowCheck.Parent := ConfigPage.Surface;
  WindowCheck.Top := ScaleY(224);
  WindowCheck.Caption := 'Enable Window watcher';
  WindowCheck.Checked := True;
end;

function NextButtonClick(CurPageID: Integer): Boolean;
var
  PortNum: Integer;
begin
  Result := True;
  if CurPageID = ConfigPage.ID then
  begin
    if Trim(ServerHostEdit.Text) = '' then
    begin
      MsgBox('ServerHost is required.', mbError, MB_OK);
      Result := False;
      exit;
    end;

    PortNum := StrToIntDef(Trim(ServerPortEdit.Text), 0);
    if (PortNum < 1) or (PortNum > 65535) then
    begin
      MsgBox('ServerPort must be in range 1..65535.', mbError, MB_OK);
      Result := False;
      exit;
    end;

    if Trim(UsersEdit.Text) = '' then
    begin
      MsgBox('Users list is required (comma-separated).', mbError, MB_OK);
      Result := False;
      exit;
    end;
  end;
end;

function GetServerHost(Param: string): string;
begin
  Result := Trim(ServerHostEdit.Text);
end;

function GetServerPort(Param: string): string;
begin
  Result := Trim(ServerPortEdit.Text);
end;

function GetDomain(Param: string): string;
begin
  Result := Trim(DomainEdit.Text);
end;

function GetUsers(Param: string): string;
begin
  Result := Trim(UsersEdit.Text);
end;

function GetInstallRoot(Param: string): string;
begin
  Result := Trim(InstallRootEdit.Text);
end;

function GetStateRoot(Param: string): string;
begin
  Result := Trim(StateRootEdit.Text);
end;

function GetAfkEnabled(Param: string): string;
begin
  if AfkCheck.Checked then Result := '$true' else Result := '$false';
end;

function GetWindowEnabled(Param: string): string;
begin
  if WindowCheck.Checked then Result := '$true' else Result := '$false';
end;
