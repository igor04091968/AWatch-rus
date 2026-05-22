# 1C Company Intelligence для AW-rus

Этот слой строится **поверх** `clickhouse-1c/` и не трогает саму 1С.

Его задача:

- анализировать работу с компаниями (`counterparty`);
- показывать, где компании выпали из активности;
- считать простой, объяснимый прогноз по событиям и activity score;
- давать read-only API для AI Investigator и внешних аналитических сервисов.

## Что считается компанией

В file-based Detmir контуре компания = `documents.counterparty`, но это поле
заполняется **не из бухгалтерских проводок**, а из read-only file-base telemetry.

Сейчас есть два связанных слоя:

- `documents`
  - событийный слой;
  - содержит `CompanyActivitySnapshot`;
- `companies`
  - отдельная read-only business-like таблица по каждой файловой базе;
  - хранит `owner_user`, `base_path`, `db_size_bytes`, `reglog_size_bytes`,
    `active_locks`, `temp_db_present`, `scheduler_touched`, `activity_score`.

Для `documents` действует telemetry-модель:

- `counterparty = infobase`;
- `doc_type = CompanyActivitySnapshot`;
- `amount` = интегральный `activity score` по изменению базы, росту reglog,
  lock/temp markers и scheduler activity.

То есть это слой прогноза **операционной активности по компаниям/базам**, а не
финансовый forecast по первичке.

## Что добавлено

### ClickHouse

Файл:

- `clickhouse-1c/clickhouse/init/04_company_intelligence.sql`

Создаёт:

- `v_companies_current`
- `company_forecasts`
- `company_health_signals`
- `v_counterparty_daily`
- `v_counterparty_latest_activity`
- `v_company_forecasts_current`
- `v_company_health_current`
- `v_company_portfolio_overview`

### Forecast refresh

Файл:

- `clickhouse-1c/ai/refresh_company_intelligence.py`

Что делает:

- строит daily series по `counterparty`;
- учитывает текущее состояние файловой базы из `companies`;
- считает базовую линию и линейный тренд;
- материализует прогнозы на `7` и `30` дней;
- создаёт health-signals:
  - `inactive_company`
  - `amount_drop`
  - `docs_stopped`
  - `base_busy`
  - `scheduler_activity`
  - `open_cases`
  - `open_detections`

### Read-only API

Файл:

- `clickhouse-1c/ai/company_intelligence_api.py`

Endpoints:

- `GET /health`
- `GET /api/1/analytics-1c/companies/overview`
- `GET /api/1/analytics-1c/companies/{counterparty}/summary`
- `GET /api/1/analytics-1c/companies/{counterparty}/forecast`
- `GET /api/1/analytics-1c/companies/{counterparty}/timeline`
- `GET /api/1/analytics-1c/manager/brief/latest`
- `GET /api/1/analytics-1c/manager/brief/latest.md`
- `GET /api/1/analytics-1c/manager/brief/history`
- `GET /manager/brief`

`/manager/brief` — это human-facing browser page для руководителя:

- server-rendered HTML;
- без frontend build chain и SPA-зависимостей;
- опирается на уже сгенерированный `latest.json`;
- показывает headline, человеческие комментарии, top risks, top forecasts,
  действия и свежесть источников;
- даёт быстрые ссылки на raw JSON/Markdown и в Grafana.

### Executive brief для руководителя

Файлы:

- `clickhouse-1c/ai/generate_manager_brief.py`
- `clickhouse-1c/ai/manager_brief_prompt.md`
- `clickhouse-1c/ai/manager_brief_schema.json`
- `clickhouse-1c/ops/run_manager_brief.sh`
- `clickhouse-1c/ops/aw-1c-manager-brief.service`
- `clickhouse-1c/ops/aw-1c-manager-brief.timer`

Что делает:

- собирает live context по портфелю компаний из `ClickHouse`;
- вызывает локальный `codex exec` на `10.10.10.2` от пользователя `codex`;
- требует structured JSON по schema, а не свободный текст;
- рендерит итог в `latest.json` и `latest.md`;
- при сбое `codex` даёт deterministic fallback, чтобы контур не оставался пустым.

Важно:

- `tmux` не является production-зависимостью;
- интерактивная сессия `codex` может быть открыта, но pipeline работает через обычный `codex exec`;
- service не трогает `1С`, работает только на уже выгруженных read-only данных.

### Ops

Файлы:

- `clickhouse-1c/ops/run_company_intelligence_refresh.sh`
- `clickhouse-1c/ops/run_company_intelligence_api.sh`
- `clickhouse-1c/ops/aw-1c-company-api.service`

И `run_ingest_cycle.sh` теперь:

1. грузит новые выгрузки;
2. обновляет timeline/detections/cases;
3. применяет `04_company_intelligence.sql`;
4. пересчитывает company forecasts/signals.

## Развёртывание

### 1. Установить зависимости

```bash
cd clickhouse-1c
python3 -m venv .venv
. .venv/bin/activate
pip install -r etl/requirements.txt
pip install -r ai/requirements.txt
```

### 2. Применить schema

```bash
clickhouse-client --queries-file clickhouse/init/04_company_intelligence.sql
```

### 3. Пересчитать company intelligence

```bash
./ops/run_company_intelligence_refresh.sh
```

### 4. Запустить API

```bash
./ops/run_company_intelligence_api.sh
```

### 5. Сформировать manager brief

```bash
./ops/run_manager_brief.sh
```

По умолчанию:

- host: `127.0.0.1`
- port: `8710`

## Переменные окружения

См.:

- `clickhouse-1c/.env.example`

Ключевые:

- `AW_1C_COMPANY_API_HOST`
- `AW_1C_COMPANY_API_PORT`
- `AW_1C_COMPANY_LOOKBACK_DAYS`
- `AW_1C_COMPANY_MIN_DAYS`
- `AW_1C_COMPANY_HORIZONS`
- `AW_1C_MANAGER_BRIEF_STATE_DIR`
- `AW_1C_MANAGER_BRIEF_MODEL`
- `AW_1C_MANAGER_BRIEF_CODEX_USER`
- `AW_1C_MANAGER_BRIEF_CODEX_BIN`
- `AW_1C_MANAGER_BRIEF_WORKDIR`
- `AW_1C_MANAGER_BRIEF_TOP_LIMIT`
- `AW_1C_MANAGER_BRIEF_FRESHNESS_HOURS`
- `AW_1C_MANAGER_BRIEF_TIMEOUT_SEC`
- `AW_1C_MANAGER_BRIEF_RUN_AFTER_INGEST`

## Что уже есть в live payload

В live `overview/summary` теперь идут не только прогнозы, но и текущее
состояние файловой базы компании:

- `company_name`
- `owner_user`
- `base_path`
- `current_status`
- `db_size_bytes`
- `reglog_size_bytes`
- `active_locks`
- `current_activity_score`

## Что прогноз реально означает

Это не black-box ML и не «магический AI».

Сейчас используется объяснимый MVP:

- daily baseline;
- linear trend;
- confidence;
- health signals на простых правилах.

Этого достаточно для:

- раннего обнаружения выпадения компаний из потока;
- ранжирования портфеля;
- AI summary поверх уже объяснимых чисел.

## Ограничения

- в file-based telemetry `amount` означает `activity score`, а не деньги;
- прогноз показывает тенденцию активности компании/базы, а не финансовое обещание;
- API строго read-only;
- никакой записи обратно в 1С нет.
