# 1C AI Investigator Runtime для AW-rus

Этот документ фиксирует **что уже реально сделано** в production-контуре
файловой 1С на базе `AW-rus`.

Речь не о целевой идее и не о roadmap, а о текущем рабочем состоянии.

## Что уже есть

Сейчас в `AW-rus` уже существует замкнутый контур:

```text
File 1C / Windows RDP host
  -> read-only export / telemetry
  -> ClickHouse on 10.10.10.2
  -> detections / cases / timeline / company intelligence
  -> manager briefs / recovery briefs / weekly digest
  -> Grafana on 10.10.10.11
  -> browser pages and read-only API
```

Это уже не просто сбор метрик.

Это production-слой:

- расследований;
- executive summaries;
- company intelligence;
- explainable forecasting;
- AI-assisted manager output.

## Production topology

### Источник

- `192.168.100.18`
  - файловая 1С;
  - scheduled task `\ActivityWatch File1C Upload`;
  - read-only export без записи в `1Cv8.1CD`.

### Analytics node

- `10.10.10.2`
  - `ClickHouse`;
  - ETL/ingest;
  - company intelligence refresh;
  - manager brief generation;
  - weekly digest generation;
  - recovery brief generation;
  - read-only API.

### Visualization

- `10.10.10.11`
  - `Grafana`;
  - folder `file-1c`;
  - dashboards для audit, detections, timeline, company intelligence.

## Что подтверждено в live

### Ingest и telemetry

Подтверждены рабочие слои:

- `documents`
- `reglog_events`
- `audit_events`
- `host_events`
- `entity_timeline`
- `detections`
- `cases`
- `companies`
- `company_registry`
- `company_forecasts`
- `company_health_signals`

Контур уже умеет:

- грузить read-only snapshots;
- собирать timeline;
- открывать detections/cases;
- считать сигналы и прогнозы;
- формировать human-facing manager output.

### Company intelligence

Подтверждён production-layer по компаниям:

- `overview`
- `summary`
- `forecast`
- `timeline`
- `priority / delta / weekly trend`
- `recovery brief`

### Browser pages для руководителя

На `10.10.10.2:8710` уже работают human-facing страницы:

- `/manager/brief`
- `/manager/changes`
- `/manager/trends/weekly`
- `/manager/digest/weekly`
- `/manager/problematic?days=1`
- `/manager/problematic?days=7`
- `/manager/recovery`
- `/manager/briefs`
- `/manager/company/{company}`

Это не raw JSON.

Это страницы для руководителя с:

- headline;
- summary;
- top risks;
- top changes;
- weekly digest;
- recovery actions;
- приоритетами;
- пояснением, почему компания красная.

### Grafana

Подтверждены рабочие dashboards:

- `1C File - Audit Overview`
- `1C File - Company Intelligence`
- `1C File - Data Quality`
- `1C File - Detections`
- `1C File - Executive Summary`
- `1C File - Investigation Timeline`
- `1C File - Operations Health`

## Что делает AI-слой

Сейчас AI в этом контуре не заменяет расчёты и не лезет напрямую в 1С.

Он работает поверх уже собранной аналитики:

1. ingest грузит read-only данные;
2. `ClickHouse` строит canonical marts и signals;
3. forecasting layer считает `7/30 day` expectations;
4. local `codex` на `10.10.10.2` превращает это в:
   - manager brief;
   - recovery brief;
   - weekly digest;
   - human-facing company pages.

То есть текущий AI-слой — это:

- explanation;
- prioritization;
- recovery guidance;
- executive narrative.

## Ключевое инженерное решение

Контур больше не опирается на “название компании в 1С” как на единственный ключ.

Введён `company_entity_key`:

- сначала `baseid:<base_id>`;
- потом `basepath:<normalized path>`;
- потом только fallback `infobase:<...>`.

Это значит:

- runtime не ломается от rename;
- forecasts/signals/cases/timeline живут вокруг technical identity;
- human labels остались, но перестали быть единственной опорой.

## Что именно уже автоматизировано

На `10.10.10.2` автоматизированы:

- ingest cycle;
- post-ingest refresh manager brief;
- post-ingest refresh recovery brief;
- periodic proofcheck;
- weekly digest timer;
- browser/API access to latest artifacts.

Контур уже умеет:

- не пустеть при сбое LLM;
- отдавать deterministic fallback;
- хранить history;
- строить delta между brief'ами;
- считать weekly trend;
- ранжировать top changes по приоритету.

## Что это пока НЕ делает

Нужно говорить жёстко.

Это ещё **не полноценная финансовая расследовательная платформа по проводкам**.

Текущие ограничения:

- file-based telemetry слой не равен бухгалтерскому ledger;
- `amount` в company intelligence сейчас может быть `activity score`, а не деньги;
- часть company-layer пока telemetry-derived;
- без отдельного read-only business extract не будет честного проводочного расследования по дебету/кредиту.

То есть сейчас это уже:

- `AI Operational / Company Investigation Platform`

но ещё не полностью:

- `AI Financial Investigation Platform` по бухгалтерским проводкам.

## Где смотреть

### Grafana

- `http://10.10.10.11:3000/dashboards/f/file-1c/?orgId=1`
- company intelligence:
  - `http://10.10.10.11:3000/d/1c-file-companies/1c-file-company-intelligence`

### Manager UI

- `http://10.10.10.2:8710/manager/brief`
- `http://10.10.10.2:8710/manager/changes`
- `http://10.10.10.2:8710/manager/trends/weekly`
- `http://10.10.10.2:8710/manager/digest/weekly`
- `http://10.10.10.2:8710/manager/recovery`

### API

- `GET /health`
- `GET /api/1/analytics-1c/companies/overview`
- `GET /api/1/analytics-1c/companies/{company}/summary`
- `GET /api/1/analytics-1c/companies/{company}/forecast`
- `GET /api/1/analytics-1c/manager/brief/latest`
- `GET /api/1/analytics-1c/manager/brief/delta/latest`
- `GET /api/1/analytics-1c/manager/trends/weekly`
- `GET /api/1/analytics-1c/manager/digest/weekly/latest`
- `GET /api/1/analytics-1c/manager/recovery/latest`

## Главная граница

Контур deliberately построен как `read-only`.

Он:

- не пишет обратно в 1С;
- не трогает `1Cv8.1CD`;
- не требует `COM/Configurator/Designer`;
- не делает destructive actions внутри production 1С.

## Следующий логичный шаг

Если нужен уже настоящий `AI Financial Investigation Platform`, а не только
`company/telemetry intelligence`, следующий слой один:

- read-only business event extraction:
  - документы;
  - движения;
  - проводки;
  - журнал регистрации;
  - изменения реквизитов.

Только после этого можно честно перейти к:

- debit/credit investigations;
- VAT anomaly narratives;
- split-payment detection;
- return/refund fraud patterns;
- full financial AI investigations.
