# Матрица PowerShell-скриптов AWatch-rus

Дата актуализации: 2026-06-17

Документ фиксирует фактический статус оставшихся PowerShell-файлов после
перехода на Rust-first контур. Цель матрицы: отделить рабочий runtime от
установочных действий, аварийного отката и устаревших проверочных скриптов.

В tracked документации не публикуются имена рабочих хостов, private IP,
учетные записи, токены и runtime evidence paths. Live-проверка выполнялась по
фактическим Scheduled Tasks, процессам и deployment config, но приватные
идентификаторы здесь заменены на обобщенные описания.

## Классы

| Класс | Значение |
|---|---|
| `runtime` | Скрипт участвует в штатном выполнении или является зависимостью штатного выполнения. |
| `installer` | Скрипт нужен для установки, настройки, проверки или ремонта, но не должен постоянно работать. |
| `fallback` | Скрипт оставлен как аварийный откат или reference-слой после перевода штатного пути на Rust. |
| `obsolete` | Скрипт не нужен для штатной работы; кандидат на удаление после проверки ссылок и задач. |

## Фактический runtime-базис

Штатный контур уже Rust-first для основных Windows-сборщиков:

- загрузка 1C/file telemetry выполняется через `aw-windows-telemetry.exe file1c-upload`;
- синхронизация DLP evidence выполняется через `aw-windows-telemetry.exe dlp-evidence-sync`;
- browser/DLP/file-operations collectors выполняются через подкоманды `aw-windows-telemetry.exe`;
- учет рабочего времени выполняется через `awatch-agent-rs.exe`;
- collector guard выполняется через `aw-windows-telemetry.exe collector-guard`.

Оставшийся рабочий PowerShell runtime на момент проверки:

- сгенерированный recovery loop;
- сгенерированный per-user launcher;
- Hayabusa/EVTX upload path.

## Runtime и generated runtime

| Файл | Класс | Фактическая роль | Штатный путь сейчас | Решение |
|---|---|---|---|---|
| `C:\ProgramData\AWatch-rus\launch-watchers.ps1` | `runtime` | Сгенерированный пользовательский launcher для watchers/collectors. | Запускается через скрытый VBS-wrapper из per-user Scheduled Tasks. | Оставить до замены launcher-а на Rust/service-friendly task reconciler. |
| `C:\ProgramData\AWatch-rus\recovery-loop.ps1` | `runtime` | Сгенерированный recovery loop. | Запускается через скрытый VBS-wrapper из recovery Scheduled Task. | Мигрировать в Rust recovery/guard, затем убрать PowerShell runtime. |
| `windows/export-upload-hayabusa-to-aw-server.ps1` | `runtime` | Выгрузка Hayabusa/EVTX evidence на сервер. | Используется отдельной Scheduled Task. | P1: перенести в `aw-windows-telemetry.exe` или отдельный Rust EXE. До миграции не удалять. |
| `windows/export-evtx-for-hayabusa.ps1` | `runtime` | Экспорт EVTX для Hayabusa upload path. | Используется как зависимость Hayabusa upload path. | P1: перенести bounded EVTX export в Rust. До миграции не удалять. |

## Fallback после Rust-first миграции

| Файл | Класс | Фактическая роль | Штатный путь сейчас | Решение |
|---|---|---|---|---|
| `windows/worktime-session-collector.ps1` | `fallback` | Legacy worktime/RDP collector. | `awatch-agent-rs.exe` является основным runtime. | Оставить до отключения `worktimeLegacyFallbackEnabled` после acceptance gate. |
| `windows/browser-domains-native-collector.ps1` | `fallback` | Legacy browser/domain collector. | `aw-windows-telemetry.exe browser-domains-collector` является основным runtime. | Оставить как rollback/reference до расширенной parity-проверки. |
| `windows/dlp-endpoint-signals-collector.ps1` | `fallback` | Legacy DLP endpoint collector. | `aw-windows-telemetry.exe dlp-endpoint-collector` является основным runtime. | Оставить как rollback/reference до расширенной parity-проверки. |
| `windows/file-operations-collector.ps1` | `fallback` | Legacy file operations collector. | `aw-windows-telemetry.exe file-operations-collector` является основным runtime. | Оставить как rollback/reference до расширенной parity-проверки. |
| `windows/dlp-policy-client.ps1` | `fallback` | Получение policy для PowerShell DLP collector. | Нужен только если включается legacy DLP collector. | Удалять вместе с PowerShell DLP fallback, не раньше. |
| `windows/export-upload-file-1c-telemetry.ps1` | `fallback` | Legacy 1C/file telemetry upload. | `aw-windows-telemetry.exe file1c-upload` является основным runtime. | Оставить rollback до нескольких успешных циклов production upload. |
| `windows/sync-dlp-evidence-artifacts.ps1` | `fallback` | Legacy DLP evidence sync. | `aw-windows-telemetry.exe dlp-evidence-sync` является основным runtime. | Оставить rollback до закрепления Rust evidence sync. |
| `windows/aw-collector-guard.ps1` | `fallback` | Legacy collector guard. | `aw-windows-telemetry.exe collector-guard` является основным runtime. | Оставить rollback до подтверждения guard/recovery parity. |
| `windows/aw-standalone-service.ps1` | `fallback` | Legacy standalone supervisor wrapper. | Не является целевым runtime для нового Rust-first контура. | Держать только для отката старого install-kit, затем удалить. |

## Installer, repair и operator layer

| Файл | Класс | Фактическая роль | Штатный путь сейчас | Решение |
|---|---|---|---|---|
| `windows/ActivityWatch.Windows.Common.psm1` | `installer` | Общие PowerShell-функции для установки/обслуживания. | Используется установочными и repair-скриптами. | Оставить до замены installer layer. |
| `windows/ActivityWatch.Windows.Common.psd1` | `installer` | Manifest общего PowerShell-модуля. | Используется вместе с `.psm1`. | Оставить до замены installer layer. |
| `windows/deploy-single-user.ps1` | `installer` | Установка для одного пользователя. | Установочный сценарий, не постоянный runtime. | Постепенно заменить Rust bootstrap/Ansible action. |
| `windows/deploy-domain-users.ps1` | `installer` | Установка для доменных пользователей. | Установочный сценарий, не постоянный runtime. | Постепенно заменить Rust bootstrap/Ansible action. |
| `windows/deploy-ensemble.ps1` | `installer` | Оркестрация Windows-компонентов установки. | Установочный сценарий, не постоянный runtime. | Постепенно заменить Rust deploy coordinator или Ansible wrapper. |
| `windows/install-standalone-service.ps1` | `installer` | Установка standalone service. | Установочное действие. | Заменить Rust installer/bootstrap CLI. |
| `windows/install-collector-guard-service.ps1` | `installer` | Установка collector guard service. | Установочное действие. | Заменить Rust installer/bootstrap CLI. |
| `windows/install-dlp-client.ps1` | `installer` | Установка DLP client части. | Установочное действие. | Заменить Rust installer/bootstrap CLI. |
| `windows/migrate-awatch-rus-paths.ps1` | `installer` | Миграция путей AWatch-rus. | Разовое migration/repair действие. | Заменить Rust migration CLI с backup/dry-run. |
| `windows/rebuild-worktime-tasks.ps1` | `installer` | Пересборка worktime Scheduled Tasks. | Repair/installer действие. | Заменить Rust task reconciliation CLI. |
| `windows/fix-session-watchers.ps1` | `installer` | Ремонт watcher-ов по сессиям. | Repair действие. | Заменить Rust repair subcommand. |
| `windows/cleanup-disc-sessions.ps1` | `installer` | Очистка устаревших disconnected-session артефактов. | Maintenance/repair действие. | Заменить Rust maintenance subcommand. |
| `windows/hardening-recovery.ps1` | `installer` | Восстановление и hardening после повреждения установки. | Emergency repair, не постоянный runtime. | Заменить Rust recovery CLI; опасные действия только через явный apply-режим. |
| `windows/validate-deployment.ps1` | `installer` | Проверка установки. | Частично заменен `aw-windows-telemetry.exe validate-deployment`. | Расширить Rust validation до полной parity, затем удалить `.ps1`. |
| `windows/audit-cryptopro.ps1` | `installer` | Аудит CryptoPro/сертификатного окружения. | Операторская проверка, не постоянный runtime. | Оставить до появления Rust audit CLI или отдельного Ansible-only gate. |
| `scripts/powershell/detmir-powershell-profile.ps1` | `installer` | Операторский PowerShell profile/helper. | Не является runtime AWatch-rus. | Оставить как operator helper, либо заменить документацией/CLI aliases. |

## Obsolete / parked

| Файл | Класс | Фактическая роль | Штатный путь сейчас | Решение |
|---|---|---|---|---|
| `windows/run-user1-probe.ps1` | `obsolete` | Ручной диагностический probe одного пользователя. | Не нужен для штатного runtime. | Удалить после проверки отсутствия ссылок в runbook/CI/tasks. |
| `.pssa_run.ps1` | `obsolete` | Локальный helper для PSScriptAnalyzer. | Не является продуктовым компонентом. | Удалить после завершения PowerShell retirement или заменить CI-командой. |
| `windows/email-outbound-collector.ps1` | `obsolete` | Legacy outbound email metadata collector. | Функция не включена в текущий runtime. | Не возвращать в production как PowerShell; при необходимости реализовывать заново в Rust с отдельным privacy/legal gate. |

## Правила дальнейшей миграции

1. Не удалять `runtime` и `fallback` скрипты без acceptance gate и проверенного
   rollback-плана.
2. Любой PowerShell runtime, который остается в Scheduled Tasks или Services,
   должен иметь явный владелец, причину сохранения и целевой Rust replacement.
3. `installer`-скрипты можно заменять постепенно: сначала Rust dry-run и
   structured report, затем apply-режим, затем удаление PowerShell layer.
4. `obsolete`-скрипты удалять только после `rg`-проверки ссылок, проверки
   Scheduled Tasks/Services и контрольного deploy/validation прогона.
5. Новые runtime-функции не добавлять на PowerShell. Для runtime, long-running
   collectors, upload, guard и validation целевой путь: Rust EXE или service.
