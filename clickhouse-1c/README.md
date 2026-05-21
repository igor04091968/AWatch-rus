# file-1C analytics stack for AW-rus

Этот каталог — отдельный production scaffold для **файловой 1С**.

Он не заменяет старый `grafana-1c/` контур и не ломает его. Старый контур
остаётся для случаев, когда 1С даёт SQL/read-only views и удобна схема
`sql-exporter -> Prometheus -> Grafana`.

Этот новый контур нужен именно тогда, когда:

- 1С работает как **файловая база** на Windows/RDP host;
- на сам RDP host нежелательно ставить тяжёлые сервисы;
- нужны нормальные расследования, timeline, detections и cases;
- Grafana должна строиться не только по KPI, а по аудиту и аномалиям.

## Целевая схема

```text
File 1C + reglog + host telemetry
          ↓
      raw landing
          ↓
   ETL / normalize / enrich
          ↓
      ClickHouse
   ├─ raw_*
   ├─ documents
   ├─ postings
   ├─ reglog_events
   ├─ audit_events
   ├─ host_events
   ├─ entity_timeline
   ├─ detections
   └─ cases
          ↓
 Grafana + Alerting + AI Investigator
```

## Что внутри

- `docker-compose.yml` — локальный scaffold ClickHouse + Grafana.
- `.env.example` — переменные окружения.
- `clickhouse/init/*.sql` — схема БД.
- `etl/load_1c_exports.py` — loader CSV/JSON выгрузок в raw/core таблицы.
- `etl/config.example.yml` — пример ETL-конфига.
- `detections/rules.yml` — каталог правил detections.
- `detections/insert_detections.sql` — SQL-шаблоны rule-based detections.
- `grafana/dashboard-catalog.md` — целевая структура дашбордов.
- `grafana/query-pack.sql` — базовые SQL-запросы для панелей.
- `grafana/provisioning/datasources/clickhouse.yml` — provisioned datasource для Grafana.
- `detections/build_entity_timeline.sql` — сборка единого timeline слоя.
- `detections/open_cases_from_detections.sql` — шаблон открытия cases из detections.
- `ops/etl-cron.example` — пример расписания каждые 6 часов.
- `ops/retention-policy.md` — минимальная retention policy.
- `ai/INVESTIGATOR_API.md` — контракт AI Investigator поверх ClickHouse/cases.

## Когда использовать именно этот контур

Используй `clickhouse-1c/`, если:

- 1С файловая;
- нужен контур `аудит -> detections -> cases -> timeline`;
- нужен drill-down в расследование, а не только KPI панели;
- данные можно выгружать из 1С в `CSV/JSON`, а не читать напрямую SQL exporter'ом.

Используй `grafana-1c/`, если:

- 1С даёт стабильный SQL/read-only доступ;
- достаточно KPI/Prometheus/Grafana;
- не нужен полноценный case-oriented audit stack.

## Быстрый старт

1. Скопировать env:

```bash
cd clickhouse-1c
cp .env.example .env
```

2. Поднять ClickHouse + Grafana:

```bash
docker compose up -d
```

3. Инициализировать landing-каталоги и ETL config:

```bash
mkdir -p landing/{documents,postings,reglog,audit,host}
cp etl/config.example.yml etl/config.yml
```

4. Запустить ETL:

```bash
python3 -m venv .venv
. .venv/bin/activate
pip install -r etl/requirements.txt
python etl/load_1c_exports.py --config etl/config.yml
```

5. Применить detections:

```bash
clickhouse-client --queries-file detections/insert_detections.sql
```

6. В Grafana строить dashboards из `grafana/dashboard-catalog.md` и
`grafana/query-pack.sql`.

## Ожидаемые источники данных

- выгрузки 1С по документам;
- выгрузки движений/проводок;
- журнал регистрации 1С;
- audit/export критичных изменений;
- host telemetry с Windows/RDP host.

## Границы

- AI Investigator не пишет в 1С;
- LLM не ходит прямо в production 1С;
- в ClickHouse кладутся нормализованные выгрузки и enrichment;
- case/timeline слой считается вне 1С.
