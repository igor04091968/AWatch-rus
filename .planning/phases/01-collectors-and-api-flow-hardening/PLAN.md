# PLAN — Phase 01: collectors-and-api-flow-hardening

## Goal

Deliver stable collector-to-server data flow so activity/worktime pages have consistent data.

## Work Items

1. Validate collector runtime and log rotation behavior.
2. Validate server ingest endpoints and bucket write/read checks.
3. Validate CORS/origin and report link consistency.
4. Add/adjust scripts or runbook checks to detect zero-data regressions early.

## Verification

- Manual and scripted checks show fresh events in target buckets.
- Host activity page reflects real activity (not `0s`) for active sessions.
- No repeating transport errors in collector logs during test window.

## Status

Planned.

## 2. Варианты доработки DLP

### Вариант A: “Hardening” — Стабилизация текущего

Цель: довести текущие коллекторы до production-grade уровня надёжности.

| # | Задача | Усилие | Влияние |
|---|---|---|---|
| A1 | HTTP retry + exponential backoff во всех коллекторах | 3-5 дней | Высокое — перестанут теряться события |
| A2 | Локальный WAL (Write-Ahead Log) — буферизация событий при недоступности сервера | 1-2 нед | Критическое — гарантия доставки |
| A3 | Healthcheck endpoint и self-diagnostics в каждом коллекторе | 3-5 дней | Среднее — видимость состояния агентов |
| A4 | Расширить aggregator: добавить `aw-email-monitor_` и `aw-dlp-endpoint-signals_` в сбор | 1 день | Среднее |
| A5 | Systemd timer / Windows Task для aggregator (автоматический запуск) | 1 день | Среднее |
| A6 | Убрать пароль из `inventory.ini` → использовать Ansible Vault или env var | 1 час | Критическое (безопасность) |
| A7 | Graceful shutdown и cleanup event subscriptions во всех коллекторах | 2-3 дня | Среднее |

Общее усилие: ~3-4 недели.

Рекомендация: обязательно сделать перед любым масштабированием. Без этого DLP — “best effort” мониторинг, а не надёжная система.
