#!/usr/bin/env bash
set -euo pipefail

TARGET_DIR="windows/installkit/innosetup"
TARGET_FILE="$TARGET_DIR/innosetup-rdp-package-filelist.md"
POINTER_FILE="docs/windows/innosetup-rdp-package-filelist.md"
ISS_FILE="$TARGET_DIR/AWatch-rus-InnoSetup.iss"
PAYLOAD_DIR="$TARGET_DIR/payload"

mkdir -p "$TARGET_DIR" "$PAYLOAD_DIR" "docs/windows"

cat > "$TARGET_FILE" <<'DOC'
# Inno Setup: файл-лист для Windows RDP deployment

Дата актуализации: 2026-05-02 (UTC).

## Что это за документ

Этот файл — **чеклист упаковки** для Inno Setup.

- Он описывает, **что класть** в инсталлятор.
- Он описывает, **что не класть** (генерируется уже на целевом хосте).
- Он **не меняет** текущие deploy-скрипты и логику проекта.

## Важное уточнение по `phase2`

Чтобы исключить путаницу:

1. В каталоге `windows/` нет файлов с именами `phase2-*`.
2. `phase2` в проекте — это обозначение этапа/набора телеметрии (DLP + endpoint signals).
3. Файлы вида `phase2-*.deployment-config.json` — это **примерные конфиги install-kit**, они лежат в `install-kit-*/server-configs-*`.

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

### 1.3 Коллекторы
- `windows/worktime-session-collector.ps1` (RDP/session presence)
- `windows/browser-domains-native-collector.ps1`
- `windows/dlp-endpoint-signals-collector.ps1`

### 1.4 Шаблоны конфигурации
- `windows/web-category-rules.example.json`
- `windows/dlp-policy.example.json`

## 2) Бинарный payload ActivityWatch

Для закрытой среды используется **offline-режим по умолчанию**:

- В комплект Inno Setup сразу включается `payload\activitywatch-v0.13.2-windows-x86_64.zip`.
- Deploy запускается с параметром `-PackageZipPath` на локальный ZIP из `{app}\payload`.
- Online-загрузка из GitHub Releases в закрытой среде не требуется.

## 3) Что НЕ включать в installer как статические файлы

Эти файлы/папки появляются на целевом Windows-хосте во время/после деплоя:

- `C:\ProgramData\ActivityWatch\deployment-config.json`
- `C:\ProgramData\ActivityWatch\web-category-rules.json`
- `C:\ProgramData\ActivityWatch\dlp-policy.json`
- `C:\ProgramData\ActivityWatch\logs\*`
- `%LOCALAPPDATA%\ActivityWatch-Phase2\incident-artifacts\*`

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
- `windows\worktime-session-collector.ps1`
- `windows\browser-domains-native-collector.ps1`
- `windows\dlp-endpoint-signals-collector.ps1`
- `windows\web-category-rules.example.json`
- `windows\dlp-policy.example.json`
- `payload\activitywatch-v0.13.2-windows-x86_64.zip`

## 6) Контроль перед сборкой .iss

1. Все файлы из раздела 1 присутствуют.
2. ZIP `activitywatch-v0.13.2-windows-x86_64.zip` лежит в `windows/installkit/innosetup/payload/`.
3. В .iss добавлен `Source: "payload\activitywatch-v0.13.2-windows-x86_64.zip"`.
4. Deploy в .iss запускается с `-PackageZipPath` на локальный ZIP.
5. После установки запускается `validate-deployment.ps1` с сохранением JSON-отчёта.
DOC

cat > "$POINTER_FILE" <<'DOC'
windows/installkit/innosetup/innosetup-rdp-package-filelist.md
DOC

cat > "$ISS_FILE" <<'DOC'
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
DOC

touch "$PAYLOAD_DIR/.gitkeep"

echo "$TARGET_FILE"
echo "$POINTER_FILE"
echo "$ISS_FILE"
echo "$PAYLOAD_DIR/.gitkeep"
