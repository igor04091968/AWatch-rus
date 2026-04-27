# 1С-Бухгалтерия: сбор и анализ в Grafana

Документ описывает внедрение контура мониторинга 1С в Grafana через Prometheus и SQL Exporter.

## 1. Целевая схема

1. База 1С (PostgreSQL или MS SQL) содержит KPI views.
2. `sql-exporter` опрашивает views read-only пользователем.
3. `prometheus` собирает метрики и считает alert-rules.
4. `grafana` визуализирует KPI и алерты.

## 2. Подготовка

Рабочий каталог:

- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c`

Подготовка env:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c
cp /mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/.env.example \
   /mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/.env
```

Обязательно изменить:

- `GRAFANA_ADMIN_PASSWORD`
- `ONEC_DSN`

## 3. SQL-представления KPI

Используйте шаблон под вашу СУБД:

- PostgreSQL: `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/sql/postgres_views_template.sql`
- MS SQL: `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/sql/mssql_views_template.sql`

После адаптации шаблона под фактические таблицы 1С создайте views в БД.

Быстрое профилирование схемы 1С (PostgreSQL):

```bash
sh /mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/tools/discover_postgres_1c.sh \
  "postgres://user:pass@db-host:5432/db?sslmode=disable"
```

Проверка KPI views перед запуском:

```bash
sh /mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/tools/validate_kpi_views.sh \
  "postgres://user:pass@db-host:5432/db?sslmode=disable"
```

## 4. Запуск стека

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c
docker compose up -d
```

## 5. Верификация

Проверка метрик sql-exporter:

```bash
curl -fsS http://127.0.0.1:9399/metrics | rg "onec_"
```

Проверка Prometheus:

```bash
curl -fsS http://127.0.0.1:9090/-/healthy
```

Проверка Grafana:

- URL: `http://<host>:3000`
- Дашборд: `1C Бухгалтерия — Обзор`
- Дашборд: `1C Бухгалтерия — SRE`

E2E health-check:

```bash
sh /mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/tools/check_pipeline.sh
```

## 6. KPI и алерты

Метрики:

- `onec_unposted_documents_total`
- `onec_sales_amount_today`
- `onec_overdue_receivables_total`
- `onec_posting_errors_total`
- `onec_data_freshness_seconds`

Алерты:

- `OneCUnpostedDocumentsHigh` (warning)
- `OneCOverdueReceivablesCritical` (critical)
- `OneCSQLExporterDown` (critical)
- `OneCDataFreshnessCritical` (critical)

Файл правил:

- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c/prometheus/alerts.yml`

## 7. Эксплуатационный минимум

- Доступ к БД 1С только read-only.
- Права только на KPI views.
- Ротация пароля read-only пользователя не реже 90 дней.
- Бэкап `grafana-data` и `prometheus-data` (docker volumes).
