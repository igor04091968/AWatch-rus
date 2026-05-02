#!/usr/bin/env bash
set -euo pipefail

mkdir -p windows/installkit/innosetup
mkdir -p docs/windows

cat > windows/installkit/innosetup/innosetup-rdp-package-filelist.md <<'EOF'
# Inno Setup: файл-лист для Windows RDP deployment

Дата актуализации: 2026-05-02 (UTC).

## Что это за документ

Этот файл — **чеклист упаковки** для Inno Setup.

- Он описывает, **что класть** в инсталлятор.
- Он описывает, **что не класть** (генерируется уже на целевом хосте).
- Он **не меняет** текущие deploy-скрипты и логику проекта.

## Важное уточнение по единой Windows-директории

Чтобы исключить путаницу:

1. InnoSetup и Ansible используют один набор путей.
2. Toolkit лежит в `{app}\windows` = `C:\Program Files\AWatch-rus\windows`.
3. Бинарники ActivityWatch лежат в `C:\Program Files\AWatch-rus\bin`.
4. Runtime-конфиг, collectors, логи и отчёты лежат в `C:\ProgramData\AWatch-rus`.

## 1) Обязательные файлы для Inno Setup пакета

### 1.1 PowerShell-модуль
- `windows/ActivityWatch.Windows.Common.psd1`
- `windows/ActivityWatch.Windows.Common.psm1`

### 1.2 Скрипты деплоя и сопровождения
- `windows/deploy-single-user.ps1`
- `windows/deploy-domain-users.ps1`
- `windows/deploy-ensemble.ps1`
- `windows/hardening-recovery.ps1`
- `windows/validate-deployment.ps1`
- `windows/migrate-awatch-rus-paths.ps1`

### 1.3 Коллекторы
- `windows/worktime-session-collector.ps1` (RDP/session presence)
- `windows/browser-domains-native-collector.ps1`
- `windows/dlp-endpoint-signals-collector.ps1`

### 1.4 Шаблоны конфигурации
- `windows/web-category-rules.example.json`
- `windows/dlp-policy.example.json`

## 2) Бинарный payload ActivityWatch

Нужен один из двух режимов:

- **Online**: скрипты скачивают `activitywatch-<version>-windows-x86_64.zip` из GitHub Releases.
- **Offline**: ZIP добавляется в пакет (например `payload\activitywatch-v0.13.2-windows-x86_64.zip`) и передаётся через `-PackageZipPath`.

## 3) Что НЕ включать в installer как статические файлы

Эти файлы/папки появляются на целевом Windows-хосте во время/после деплоя:

- `C:\ProgramData\AWatch-rus\deployment-config.json`
- `C:\ProgramData\AWatch-rus\web-category-rules.json`
- `C:\ProgramData\AWatch-rus\dlp-policy.json`
- `C:\ProgramData\AWatch-rus\logs\*`
- `%LOCALAPPDATA%\AWatch-rus\incident-artifacts\*`

## 4) Опционально приложить в операторский install-kit

- `docs/windows/deployment.md`
- `docs/windows/validation.md`
- `docs/windows/troubleshooting.md`
- `docs/windows/ensemble.md`

## 5) Рекомендуемая структура внутри пакета

- `windows\ActivityWatch.Windows.Common.psd1`
- `windows\ActivityWatch.Windows.Common.psm1`
- `windows\deploy-single-user.ps1`
- `windows\deploy-domain-users.ps1`
- `windows\deploy-ensemble.ps1`
- `windows\hardening-recovery.ps1`
- `windows\validate-deployment.ps1`
- `windows\migrate-awatch-rus-paths.ps1`
- `windows\worktime-session-collector.ps1`
- `windows\browser-domains-native-collector.ps1`
- `windows\dlp-endpoint-signals-collector.ps1`
- `windows\web-category-rules.example.json`
- `windows\dlp-policy.example.json`
- `payload\activitywatch-v0.13.2-windows-x86_64.zip` (только для offline-режима)

## 6) Контроль перед сборкой .iss

1. Все файлы из раздела 1 присутствуют.
2. Выбран режим payload: online или offline.
3. Для offline-режима ZIP действительно лежит в `payload\`.
4. В .iss есть запуск нужного deploy-сценария (`deploy-ensemble.ps1` или `deploy-domain-users.ps1`).
5. После установки запускается `validate-deployment.ps1` с сохранением JSON-отчёта.
EOF

cat > docs/windows/innosetup-rdp-package-filelist.md <<'EOF'
windows/installkit/innosetup/innosetup-rdp-package-filelist.md
EOF

cat > windows/installkit/innosetup/AWatch-rus-InnoSetup.iss <<'EOF'
#define MyAppName "AWatch-rus InstallKit"
#define MyAppVersion "1.0.0"
#define MyAppPublisher "AWatch-rus"
#define MyAppExeName "powershell.exe"

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
Source: "payload\activitywatch-v0.13.2-windows-x86_64.zip"; DestDir: "{app}\payload"; Flags: ignoreversion skipifsourcedoesntexist
Source: "innosetup-rdp-package-filelist.md"; DestDir: "{app}\windows\installkit\innosetup"; Flags: ignoreversion

[Run]
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\windows\deploy-ensemble.ps1"""; Flags: runhidden
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\windows\validate-deployment.ps1"""; Flags: runhidden
EOF

mkdir -p windows/installkit/innosetup/payload
touch windows/installkit/innosetup/payload/.gitkeep

echo "patched"
echo "windows/installkit/innosetup/innosetup-rdp-package-filelist.md"
echo "docs/windows/innosetup-rdp-package-filelist.md"
echo "windows/installkit/innosetup/AWatch-rus-InnoSetup.iss"
echo "windows/installkit/innosetup/payload/.gitkeep"
