# 1С-Бухгалтерия → Grafana (production scaffold)

Готовый каркас для непрерывного сбора KPI из 1С и анализа в Grafana:

- `sql-exporter` читает SQL-представления KPI из БД 1С;
- `prometheus` собирает метрики и применяет alert-rules;
- `grafana` поднимает datasource и дашборд автоматически.

## Полные пути

- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/.env.example`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/docker-compose.yml`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/sql-exporter/sql_exporter.yml`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/sql-exporter/collectors/onec_accounting_kpi.collector.yml`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/prometheus/prometheus.yml`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/prometheus/alerts.yml`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/prometheus/recording_rules.yml`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/grafana/dashboards/1c-accounting-overview.json`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/grafana/dashboards/1c-accounting-sre.json`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/sql/postgres_views_template.sql`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/sql/mssql_views_template.sql`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/tools/discover_postgres_1c.sh`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/tools/validate_kpi_views.sh`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/tools/check_pipeline.sh`

## Быстрый запуск

1. Подготовьте env:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c
cp .env.example .env
```

2. В `.env` задайте:

- `GRAFANA_ADMIN_USER`, `GRAFANA_ADMIN_PASSWORD`;
- `ONEC_DSN` (DSN read-only пользователя в БД 1С).

3. В БД 1С создайте KPI-представления:

- для PostgreSQL возьмите `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/sql/postgres_views_template.sql`;
- для MS SQL возьмите `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/sql/mssql_views_template.sql`.

4. Поднимите стек:

```bash
docker compose up -d
```

5. Проверка:

```bash
curl -fsS http://127.0.0.1:9399/metrics | head
curl -fsS http://127.0.0.1:9090/-/healthy
```

Откройте Grafana: `http://<host>:3000`.

## Быстрая диагностика

Профилирование структуры 1С (PostgreSQL):

```bash
sh /mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/tools/discover_postgres_1c.sh \
  "postgres://user:pass@db-host:5432/db?sslmode=disable"
```

Проверка KPI views:

```bash
sh /mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/tools/validate_kpi_views.sh \
  "postgres://user:pass@db-host:5432/db?sslmode=disable"
```

Проверка end-to-end пайплайна:

```bash
sh /mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/tools/check_pipeline.sh
```

## Что контролируется

- Непроведенные документы (`onec_unposted_documents_total`)
- Продажи текущего дня (`onec_sales_amount_today`)
- Просроченная дебиторка (`onec_overdue_receivables_total`)
- Ошибки проведения за 24ч (`onec_posting_errors_total`)
- Свежесть данных из 1С (`onec_data_freshness_seconds`)

## Принципы безопасности

- только read-only аккаунт к БД 1С;
- минимальные SQL-права только на KPI views;
- секреты только в `.env`, не в git.
