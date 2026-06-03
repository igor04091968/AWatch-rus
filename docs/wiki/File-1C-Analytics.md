# File 1C Analytics

Эта страница фиксирует **production-контур для файловой 1С Detmir**.

## Что это за контур

Это не SQL-exporter и не `rac`-мониторинг серверной 1С.

Это отдельный стек для **файловой 1С**, где:

- Windows/RDP host отдаёт только `read-only export/telemetry`;
- `<GATEWAY_HOST>` принимает данные, грузит их в `ClickHouse`, строит `detections/cases`;
- `<GRAFANA_HOST>` показывает dashboards в `Grafana`.

## Production topology

- `<WINDOWS_HOST>`
  - файловая 1С
  - scheduled task `ActivityWatch File1C Upload`
- `<GATEWAY_HOST>`
  - `ClickHouse`
  - ETL/ingest
  - `aw-1c-ingest.timer`
  - `aw-1c-proofcheck.timer`
- `<GRAFANA_HOST>`
  - `Grafana`
  - datasource `clickhouse-1c`
  - folder `1C File Analytics`

## Подтверждённое рабочее состояние

Подтверждённые таблицы:

- `documents`
- `reglog_events`
- `audit_events`
- `host_events`
- `entity_timeline`
- `detections`
- `cases`

Подтверждённый runtime:

- scheduled task `\ActivityWatch File1C Upload`
  - `Run As User: Администратор`
  - `Last Result: 0`
- `aw-1c-proofcheck.timer`
  - `active`
  - `enabled`

## Что важно помнить

- Контур **не трогает содержимое 1С**.
- `1Cv8.1CD` не меняется.
- `COM`, `Configurator`, `Designer` сюда не входят.
- Для scheduled task на Windows нужен рабочий principal, а не `SYSTEM`.

## Основные dashboards

- `1C Executive Summary`
- `1C Operations Health`
- `1C Audit Overview`
- `1C Detections`
- `1C Investigation Timeline`
- `1C Data Quality`

## Главный документ

- [Файловая 1С Detmir: промышленное развёртывание ClickHouse/Grafana контура](../1C_FILE_ANALYTICS_STACK_RU.md)

## Связанные документы

- [1C Grafana deployment](../1C_GRAFANA_DEPLOYMENT_RU.md)
- [Runbook](../runbook.md)
