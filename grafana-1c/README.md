# 1С-Бухгалтерия → Grafana (production scaffold)

Готовый каркас для непрерывного сбора KPI из 1С и анализа в Grafana:

- `sql-exporter` читает SQL-представления KPI из БД 1С;
- `aw-exporter` собирает метрики ActivityWatch и отдает их Prometheus;
- `prometheus` собирает метрики и применяет alert-rules;
- `grafana` поднимает datasource и дашборд автоматически.

## Полные пути

- `./.env.example`
- `./docker-compose.yml`
- `./sql-exporter/sql_exporter.yml`
- `./sql-exporter/collectors/onec_accounting_kpi.collector.yml`
- `./prometheus/prometheus.yml`
- `./prometheus/alerts.yml`
- `./prometheus/recording_rules.yml`
- `./grafana/dashboards/1c-accounting-overview.json`
- `./grafana/dashboards/1c-accounting-sre.json`
- `./sql/postgres_views_template.sql`
- `./sql/mssql_views_template.sql`
- `./tools/discover_postgres_1c.sh`
- `./tools/validate_kpi_views.sh`
- `./tools/check_pipeline.sh`

## Быстрый запуск

1. Подготовьте env:

```bash
cd grafana-1c
cp .env.example .env
```

2. В `.env` задайте:

- `GRAFANA_ADMIN_USER`, `GRAFANA_ADMIN_PASSWORD`;
- `ONEC_DSN` (DSN read-only пользователя в БД 1С);
- при необходимости `AW_SERVER_HOST`, `AW_SERVER_PORT`, `AW_SERVER_SCHEME`, `AW_EXPORTER_PORT` и `AW_SCRAPE_INTERVAL_SECONDS` для ActivityWatch exporter.

3. В БД 1С создайте KPI-представления:

- для PostgreSQL возьмите `./sql/postgres_views_template.sql`;
- для MS SQL возьмите `./sql/mssql_views_template.sql`.

4. Поднимите стек:

```bash
docker compose up -d
```

5. Проверка:

```bash
curl -fsS http://127.0.0.1:9399/metrics | head
curl -fsS http://127.0.0.1:9398/metrics | head
curl -fsS http://127.0.0.1:9090/-/healthy
curl -fsS http://127.0.0.1:3000/api/health
```

Откройте Grafana: `http://<host>:3000`.

## Быстрая диагностика

Профилирование структуры 1С (PostgreSQL):

```bash
sh ./tools/discover_postgres_1c.sh \
  "postgres://user:pass@db-host:5432/db?sslmode=disable"
```

Проверка KPI views:

```bash
sh ./tools/validate_kpi_views.sh \
  "postgres://user:pass@db-host:5432/db?sslmode=disable"
```

Проверка end-to-end пайплайна:

```bash
sh ./tools/check_pipeline.sh
```

Скрипт проверяет полный путь сбора данных: `sql-exporter` и `aw-exporter` отдают обязательные метрики, Prometheus успешно выполняет запросы по scrape-targets, а Grafana отвечает на health/API, видит datasource `prometheus` и provisioned dashboards. Если стек запущен не из каталога репозитория, передайте путь к каталогу `grafana-1c` первым аргументом.

## Что контролируется

- Непроведенные документы (`onec_unposted_documents_total`)
- Продажи текущего дня (`onec_sales_amount_today`)
- Просроченная дебиторка (`onec_overdue_receivables_total`)
- Ошибки проведения за 24ч (`onec_posting_errors_total`)
- Свежесть данных из 1С (`onec_data_freshness_seconds`)
- Доступность ActivityWatch API (`aw_up`)
- Количество bucket/events ActivityWatch (`aw_buckets_total`, `aw_bucket_events_count`)

## Принципы безопасности

- только read-only аккаунт к БД 1С;
- минимальные SQL-права только на KPI views;
- секреты только в `.env`, не в git.
