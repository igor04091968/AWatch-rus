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
   ├─ business_events
   ├─ document_change_events
   ├─ reglog_events
   ├─ audit_events
   ├─ host_events
   ├─ entity_timeline
   ├─ detections
   ├─ cases
   ├─ company_forecasts
   └─ company_health_signals
          ↓
 Grafana + Alerting + AI Investigator
```

## Что внутри

- `docker-compose.yml` — локальный scaffold ClickHouse + Grafana.
- `.env.example` — переменные окружения.
- `clickhouse/init/*.sql` — схема БД.
- `etl/load_1c_exports.py` — loader CSV/JSON выгрузок в raw/core таблицы.
- `etl/build_business_event_exports.py` — read-only normalizer из
  `documents/postings/audit` в canonical `business_events/document_changes`.
- `etl/extract_1c_mcp_toolkit.py` — read-only extractor из
  `1c-mcp-toolkit` REST API в `landing/*`.
- `etl/config.example.yml` — пример ETL-конфига.
- `docs/1C_BUSINESS_EVENT_LAYER_RU.md` — production contract следующего шага:
  canonical business-event слой для документов/проводок/изменений.
- `detections/rules.yml` — каталог правил detections.
- `detections/insert_detections.sql` — SQL-шаблоны rule-based detections.
- `grafana/dashboard-catalog.md` — целевая структура дашбордов.
- `grafana/query-pack.sql` — базовые SQL-запросы для панелей.
- `grafana/provisioning/datasources/clickhouse.yml` — provisioned datasource для Grafana.
- `grafana/provisioning/dashboards/files/1c-company-intelligence.json` — source dashboard для анализа и прогноза по компаниям.
- `grafana/provisioning/dashboards/files/1c-management-board.json` — management dashboard для руководителя с links на brief/actions/digest/recovery.
- `grafana/provisioning/dashboards/files/1c-financial-reporting.json` — первый financial board с разделением `ledger` и `proxy`.
- `grafana/provisioning/dashboards/files/1c-telemetry-board.json` — telemetry dashboard по состоянию файловых баз, reglog growth, busy markers и host load.
- `detections/build_entity_timeline.sql` — сборка единого timeline слоя.
- `detections/open_cases_from_detections.sql` — шаблон открытия cases из detections.
- `ops/etl-cron.example` — пример расписания каждые 6 часов.
- `ops/retention-policy.md` — минимальная retention policy.
- `ai/INVESTIGATOR_API.md` — контракт AI Investigator поверх ClickHouse/cases.
- `ai/refresh_company_intelligence.py` — materialization forecast/signals по `counterparty`.
- `ai/company_intelligence_api.py` — read-only API для AI/аналитики по компаниям.
- `ai/generate_manager_brief.py` — executive brief для руководителя поверх live company intelligence.
- `ai/manager_brief_prompt.md` — prompt для локального `codex exec`.
- `ai/manager_brief_schema.json` — строгая schema structured-brief ответа.

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
mkdir -p landing/{documents,postings,business_events,document_changes,companies,reglog,audit,host}
cp etl/config.example.yml etl/config.yml
```

4. Запустить ETL:

```bash
python3 -m venv .venv
. .venv/bin/activate
pip install -r etl/requirements.txt
python etl/extract_1c_mcp_toolkit.py --config etl/config.yml --validate-config
python etl/extract_1c_mcp_toolkit.py --config etl/config.yml --dataset documents --dry-run
python etl/extract_1c_mcp_toolkit.py --config etl/config.yml
python etl/build_business_event_exports.py --config etl/config.yml
python etl/load_1c_exports.py --config etl/config.yml
```

5. Применить detections:

```bash
clickhouse-client --queries-file detections/insert_detections.sql
```

6. Включить company intelligence слой:

```bash
clickhouse-client --queries-file clickhouse/init/04_company_intelligence.sql
python ai/refresh_company_intelligence.py --host localhost --port 8123 --user default --password change-me --database analytics_1c
```

7. Запустить read-only API:

```bash
python ai/company_intelligence_api.py --host 127.0.0.1 --port 8710
```

8. Сформировать executive brief:

```bash
python ai/generate_manager_brief.py --host localhost --port 8123 --user default --password change-me --database analytics_1c
```

9. В Grafana строить dashboards из `grafana/dashboard-catalog.md` и
`grafana/query-pack.sql`.

## Manager brief

Этот слой делает не raw LLM-чат, а промышленный pipeline:

- строит компактный context из `v_company_portfolio_overview`;
- вызывает локальный `codex exec` на `10.10.10.2`;
- валидирует structured output по JSON schema;
- при сбое `codex` отдаёт deterministic fallback, чтобы контур не пустел;
- пишет `latest.json` и `latest.md` в `state/manager-brief/`.

Read-only API для руководителя:

- `GET /api/1/analytics-1c/manager/brief/latest`
- `GET /api/1/analytics-1c/manager/brief/latest.md`
- `GET /api/1/analytics-1c/manager/brief/history`

## Ожидаемые источники данных

- выгрузки 1С по документам;
- выгрузки движений/проводок;
- выгрузки canonical business events;
- выгрузки изменений документов и реквизитов;
- read-only `companies` snapshot по файловым базам;
- журнал регистрации 1С;
- audit/export критичных изменений;
- host telemetry с Windows/RDP host.
- `1c-mcp-toolkit` REST API (`execute_query` + `get_event_log`) в
  строго read-only режиме.
- для company intelligence в file-based Detmir контуре `counterparty`
  наполняется read-only telemetry слоем как `counterparty = infobase`, а
  `amount` используется как интегральный `activity score`.

Отдельно:

- `companies` хранит текущую карту файловых баз:
  - `owner_user`
  - `base_path`
  - `db_size_bytes`
  - `reglog_size_bytes`
  - `active_locks`
  - `temp_db_present`
  - `scheduler_touched`
  - `activity_score`

## Границы

- AI Investigator не пишет в 1С;
- LLM не ходит прямо в production 1С;
- в ClickHouse кладутся нормализованные выгрузки и enrichment;
- case/timeline слой считается вне 1С.
- это прогноз активности компании/базы, а не финансовых проводок;
- если `counterparty` в live-выгрузках пустой, company-forecast слой останется корректно пустым.

## Следующий production шаг

В репо уже заложен scaffold под следующий слой:

- `business_events` — единый event stream бухгалтерских событий;
- `document_change_events` — изменения документов/реквизитов;
- ETL уже умеет принимать эти datasets в `landing/business_events` и
  `landing/document_changes`;
- built-in normalizer уже умеет собирать этот слой из существующих read-only
  выгрузок `documents/postings/audit`;
- extractor scaffold уже умеет забирать read-only snapshots/events через
  `1c-mcp-toolkit` и писать их в те же `landing/*`;
- дальше нужен только более богатый extractor из 1С или внешних безопасных
  выгрузок, если нужна большая бухгалтерская детализация.
- в `05_financial_reporting.sql` уже заложены первые financial marts и board,
  которые честно показывают `proxy_only`, пока настоящие `postings` ещё не
  поступают в live-ingest.

## `1c-mcp-toolkit` extractor

Этот extractor нужен не вместо ClickHouse ETL, а перед ним:

```text
1C + 1c-mcp-toolkit REST
          ↓
etl/extract_1c_mcp_toolkit.py
          ↓
landing/{documents,postings,business_events,document_changes,companies,reglog}
          ↓
etl/build_business_event_exports.py
          ↓
etl/load_1c_exports.py
```

Что он умеет:

- читать `execute_query` для `companies`, `documents`, `postings`,
  `business_events`, `document_changes`;
- читать `get_event_log` для `reglog`;
- вести локальный checkpoint в `state/1c-mcp-toolkit/extract_state.json`;
- поддерживать `channel` isolation;
- работать только по read-only endpoint'ам без `execute_code`.
- включаться в общий ingest-cycle через
  `AW_1C_MCP_TOOLKIT_EXTRACT_BEFORE_INGEST=1`.
- перед live rollout поддерживает:
  - `--validate-config`
  - `--dry-run`
  - `--dataset <name>`

Минимальный production-контракт:

- в 1С-запросах желательно сразу алиасить колонки под поля landing-схемы;
- если это неудобно, использовать `field_map` в `mcp_toolkit.datasets.*`;
- для incremental-query datasets задавать `incremental.since_param` /
  `incremental.until_param`;
- для `reglog` задавать `event_log.static_fields.infobase`, потому что
  `get_event_log` сам по себе не всегда несёт имя базы.
