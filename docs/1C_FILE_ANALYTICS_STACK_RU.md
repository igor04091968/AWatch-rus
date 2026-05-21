# Файловая 1С: ClickHouse + Grafana + AI Investigator

Этот документ описывает целевой и уже подготовленный scaffold для **файловой 1С**.

Он нужен для случаев, когда:

- 1С работает как файловая база на Windows/RDP host;
- на сам RDP host не хочется ставить лишние тяжёлые сервисы;
- нужен не только KPI-обзор, но и audit/detection/investigation контур.

## Почему не `prometheus_1C_exporter`

`prometheus_1C_exporter` полезен для **серверной 1С** с `rac`.

Для файловой 1С он не даёт нужного контекста:

- нет кластера `rac`;
- нет нормального session/license/runtime слоя как у серверной 1С;
- остаются только host-level и export-level данные.

Поэтому для файловой 1С правильный путь другой:

```text
1С exports + reglog + host telemetry
              ↓
       ETL / normalize
              ↓
         ClickHouse
              ↓
     Grafana + detections
              ↓
       AI Investigator
```

## Что входит в scaffold

- `clickhouse-1c/` — новый каталог стека;
- ClickHouse schema для raw/core/timeline/cases;
- ETL loader CSV/JSON выгрузок;
- detection catalog;
- Grafana dashboard catalog;
- AI Investigator API contract.

## Основные таблицы

- `raw_1c_documents`
- `raw_1c_postings`
- `raw_reglog`
- `raw_audit`
- `raw_host_metrics`
- `documents`
- `postings`
- `reglog_events`
- `audit_events`
- `host_events`
- `entity_timeline`
- `detections`
- `cases`

## Что визуализировать в Grafana

Роли:

1. `1C Executive Summary`
2. `1C Operations Health`
3. `1C Audit Overview`
4. `1C Detections`
5. `1C Investigation Timeline`
6. `1C Data Quality`

Полный каталог панелей:

- `clickhouse-1c/grafana/dashboard-catalog.md`
- `clickhouse-1c/grafana/query-pack.sql`

## Detections

Первый production-набор правил:

- вход вне рабочего времени;
- всплеск failed logins;
- массовое перепроведение;
- изменение критичных объектов;
- аномальный рост ручных корректировок;
- аномальный рост возвратов;
- рост просроченной дебиторки;
- длительные операции;
- всплеск ошибок обмена;
- высокая задержка диска;
- stale backup;
- нетипичные проводки по счетам.

См.:

- `clickhouse-1c/detections/rules.yml`
- `clickhouse-1c/detections/insert_detections.sql`
- `clickhouse-1c/detections/build_entity_timeline.sql`
- `clickhouse-1c/detections/open_cases_from_detections.sql`
- `clickhouse-1c/ops/etl-cron.example`

## Операционный порядок

1. На файловом/RDP host:
   - выгрузить данные 1С в `CSV/JSON`;
   - выгрузить журнал регистрации;
   - снять host telemetry.
2. На utility VM:
   - положить файлы в `clickhouse-1c/landing/*`;
   - прогнать `etl/load_1c_exports.py`;
   - прогнать `insert_detections.sql`;
   - открыть Grafana dashboards;
   - при необходимости построить AI summary поверх cases/timeline.

## Где граница AI

AI не должен:

- писать обратно в 1С;
- выполнять произвольный SQL;
- менять case state без явного правила.

AI должен:

- объяснять detections;
- строить summary по case;
- связывать audit/reglog/documents в timeline;
- предлагать next steps.
