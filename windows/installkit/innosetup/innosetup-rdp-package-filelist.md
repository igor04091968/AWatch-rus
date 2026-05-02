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

### 1.3 Коллекторы
- `windows/worktime-session-collector.ps1` (RDP/session presence)
- `windows/browser-domains-native-collector.ps1`
- `windows/dlp-endpoint-signals-collector.ps1`

### 1.4 Шаблоны конфигурации
- `windows/web-category-rules.example.json`
- `windows/dlp-policy.example.json`

## 2) Бинарный payload ActivityWatch

Поддерживаются оба режима:

- **Online**: ZIP скачивается из GitHub Releases.
- **Offline**: ZIP кладётся в installer (`payload\activitywatch-v0.13.2-windows-x86_64.zip`) и передаётся в deploy через `-PackageZipPath`.

Для нашей закрытой среды обычно используется **offline-режим**.

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
- `windows\worktime-session-collector.ps1`
- `windows\browser-domains-native-collector.ps1`
- `windows\dlp-endpoint-signals-collector.ps1`
- `windows\web-category-rules.example.json`
- `windows\dlp-policy.example.json`
- `payload\activitywatch-v0.13.2-windows-x86_64.zip` (только для offline-режима)

## 6) Контроль перед сборкой .iss

1. Все файлы из раздела 1 присутствуют.
2. В .iss не осталось вызова `deploy-ensemble.ps1` без параметров: нужны `-ServerHost` и `-Users`.
3. Для Windows/RDP используются единые пути:
   - `InstallRoot = C:\Program Files\AWatch-rus\bin`
   - `StateRoot = C:\ProgramData\AWatch-rus`
4. Для offline-режима ZIP лежит в `windows/installkit/innosetup/payload/` (имя: `activitywatch-v0.13.2-windows-x86_64.zip`).
5. Для проверки используется `-ValidateAfterDeploy` (отчёт `ensemble-report-*.json` пишется в `C:\ProgramData\AWatch-rus\`).
