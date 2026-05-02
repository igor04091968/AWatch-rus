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

Нужен один из двух режимов:

- **Online**: скрипты скачивают `activitywatch-<version>-windows-x86_64.zip` из GitHub Releases.
- **Offline**: ZIP добавляется в пакет (например `payload\activitywatch-v0.13.2-windows-x86_64.zip`) и передаётся через `-PackageZipPath`.

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
- `payload\activitywatch-v0.13.2-windows-x86_64.zip` (только для offline-режима)

## 6) Контроль перед сборкой .iss

1. Все файлы из раздела 1 присутствуют.
2. Выбран режим payload: online или offline.
3. Для offline-режима ZIP действительно лежит в `payload\`.
4. В .iss есть запуск нужного deploy-сценария (`deploy-ensemble.ps1` или `deploy-domain-users.ps1`).
5. После установки запускается `validate-deployment.ps1` с сохранением JSON-отчёта.
