# File 1C Analytics

Эта страница описывает новый контур для **файловой 1С**.

## Когда он нужен

Используй этот контур, если:

- 1С файловая;
- на RDP host нельзя или нежелательно ставить тяжёлые агенты;
- нужен audit/detection/investigation стек;
- Grafana должна быть не только для KPI, но и для расследования.

## Схема

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

## Основные компоненты

- `clickhouse-1c/README.md`
- `clickhouse-1c/clickhouse/init/*.sql`
- `clickhouse-1c/etl/load_1c_exports.py`
- `clickhouse-1c/detections/rules.yml`
- `clickhouse-1c/grafana/dashboard-catalog.md`
- `clickhouse-1c/ai/INVESTIGATOR_API.md`

## Основные dashboard-ы

- `1C Executive Summary`
- `1C Operations Health`
- `1C Audit Overview`
- `1C Detections`
- `1C Investigation Timeline`
- `1C Data Quality`

## Связанные документы

- [File 1C analytics stack](../1C_FILE_ANALYTICS_STACK_RU.md)
- [1C Grafana deployment](../1C_GRAFANA_DEPLOYMENT_RU.md)
- [Runbook](../runbook.md)
