# 1C Company Intelligence для AW-rus

Этот слой строится **поверх** `clickhouse-1c/` и не трогает саму 1С.

Его задача:

- анализировать работу с компаниями (`counterparty`);
- показывать, где компании выпали из активности;
- считать простой, объяснимый прогноз по событиям и activity score;
- давать read-only API для AI Investigator и внешних аналитических сервисов.

## Что считается компанией

В file-based Detmir контуре компания = `documents.counterparty`, но это поле
заполняется **не из бухгалтерских проводок**, а из read-only file-base telemetry:

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
- считает базовую линию и линейный тренд;
- материализует прогнозы на `7` и `30` дней;
- создаёт health-signals:
  - `inactive_company`
  - `amount_drop`
  - `docs_stopped`
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
