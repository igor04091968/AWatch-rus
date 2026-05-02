#define MyAppName "AWatch-rus InstallKit"
#define MyAppVersion "1.0.0"
#define MyAppPublisher "AWatch-rus"

#define AwDefaultServerHost "10.10.10.13"
#define AwDefaultServerPort "5600"
#define AwDefaultUsers "user1,user2,user3,user4,user5"
#define AwDefaultInstallRoot "C:\\Program Files\\ActivityWatch-Phase2"
#define AwDefaultStateRoot "C:\\ProgramData\\ActivityWatch-Phase2"
#define AwDefaultZipName "activitywatch-v0.13.2-windows-x86_64.zip"

[Setup]
AppId={{6D6A1F74-0F4F-4A57-B5E3-1C2C2F56C0E9}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\AWatch-rus
DefaultGroupName=AWatch-rus
OutputDir=.
OutputBaseFilename=AWatch-rus-InstallKit
Compression=lzma
SolidCompression=yes
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin

[Languages]
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"

[Tasks]
Name: "deploy"; Description: "Запустить деплой после установки"; Flags: checkedonce
Name: "validate"; Description: "Запустить validate-deployment (через -ValidateAfterDeploy)"; Flags: checkedonce

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
; Offline payload (optional): place ZIP into windows/installkit/innosetup/payload/ before compiling.
Source: "payload\{#AwDefaultZipName}"; DestDir: "{app}\payload"; Flags: ignoreversion skipifsourcedoesntexist
Source: "innosetup-rdp-package-filelist.md"; DestDir: "{app}\windows\installkit\innosetup"; Flags: ignoreversion

[Run]
Filename: "powershell.exe"; Parameters: "{code:GetDeployEnsembleParams}"; Flags: runhidden; Tasks: deploy

[Code]
var
  ServerHostPage: TInputQueryWizardPage;
  UsersPage: TInputQueryWizardPage;
  OptionsPage: TInputOptionWizardPage;

function NormalizeUserCsv(const UserCsv: string): string;
var
  i: Integer;
  s: string;
  token: string;
begin
  Result := '';
  s := UserCsv;
  while True do
  begin
    i := Pos(',', s);
    if i = 0 then
    begin
      token := Trim(s);
      s := '';
    end
    else
    begin
      token := Trim(Copy(s, 1, i - 1));
      Delete(s, 1, i);
    end;

    if token <> '' then
    begin
      if Result <> '' then
        Result := Result + ',';
      Result := Result + token;
    end;

    if s = '' then
      Break;
  end;
end;

function BuildUsersPowerShellArg(const UserCsv: string): string;
var
  i: Integer;
  s: string;
  token: string;
  quoted: string;
begin
  Result := '';
  s := UserCsv;
  while True do
  begin
    i := Pos(',', s);
    if i = 0 then
    begin
      token := Trim(s);
      s := '';
    end
    else
    begin
      token := Trim(Copy(s, 1, i - 1));
      Delete(s, 1, i);
    end;

    if token <> '' then
    begin
      quoted := '"' + token + '"';
      if Result <> '' then
        Result := Result + ',';
      Result := Result + quoted;
    end;

    if s = '' then
      Break;
  end;
  if Result <> '' then
    Result := '-Users ' + Result;
end;

function PayloadZipPath: string;
begin
  Result := ExpandConstant('{app}\payload\{#AwDefaultZipName}');
end;

function HasPayloadZip: Boolean;
begin
  Result := FileExists(ExpandConstant('{src}\payload\{#AwDefaultZipName}'));
end;

procedure InitializeWizard;
begin
  ServerHostPage := CreateInputQueryPage(
    wpSelectDir,
    'Параметры AW сервера',
    'Укажите сервер ActivityWatch (куда агенты будут отправлять данные).',
    'Если нужно, измените host/port. По умолчанию — наша конфигурация.'
  );
  ServerHostPage.Add('ServerHost', False);
  ServerHostPage.Add('ServerPort', False);
  ServerHostPage.Values[0] := '{#AwDefaultServerHost}';
  ServerHostPage.Values[1] := '{#AwDefaultServerPort}';

  UsersPage := CreateInputQueryPage(
    ServerHostPage.ID,
    'Пользователи (RDP)',
    'Перечень пользователей, для которых разворачиваем агенты.',
    'Введите список через запятую. Пример: user1,user2,user3'
  );
  UsersPage.Add('Users (CSV)', False);
  UsersPage.Values[0] := '{#AwDefaultUsers}';

  OptionsPage := CreateInputOptionPage(
    UsersPage.ID,
    'Опции деплоя',
    'Выберите опции для установки/валидации.',
    '',
    False,
    False
  );
  OptionsPage.Add('Использовать offline payload (встроенный ZIP)');
  OptionsPage.Add('Запустить validate-deployment после деплоя');
  OptionsPage.Values[0] := HasPayloadZip;
  OptionsPage.Values[1] := True;
end;

function GetDeployEnsembleParams(Param: string): string;
var
  serverHost: string;
  serverPort: string;
  usersCsv: string;
  usersArg: string;
  zipArg: string;
  validateArg: string;
begin
  serverHost := Trim(ServerHostPage.Values[0]);
  serverPort := Trim(ServerHostPage.Values[1]);
  usersCsv := NormalizeUserCsv(UsersPage.Values[0]);

  usersArg := BuildUsersPowerShellArg(usersCsv);
  if usersArg = '' then
    RaiseException('Users list is empty.');

  zipArg := '';
  if OptionsPage.Values[0] then
    zipArg := ' -PackageZipPath "' + PayloadZipPath + '"';

  validateArg := '';
  if OptionsPage.Values[1] and WizardIsTaskSelected('validate') then
    validateArg := ' -ValidateAfterDeploy';

  Result :=
    '-NoProfile -ExecutionPolicy Bypass -File "' + ExpandConstant('{app}\windows\deploy-ensemble.ps1') + '"' +
    ' -ServerHost "' + serverHost + '"' +
    ' -ServerPort ' + serverPort +
    ' ' + usersArg +
    zipArg +
    ' -InstallRoot "{#AwDefaultInstallRoot}"' +
    ' -StateRoot "{#AwDefaultStateRoot}"' +
    validateArg;
end;
