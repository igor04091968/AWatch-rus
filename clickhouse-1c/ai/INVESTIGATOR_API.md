# AI Investigator API contract

AI Investigator не должен ходить напрямую в файловую 1С.

Его правильный слой:

- ClickHouse (`analytics_1c.*`)
- DLP/forensics case API
- bounded search/timeline endpoints

## Базовые use-cases

- почему выросли ошибки по базе;
- какие пользователи дали риск за сутки;
- что произошло по case `X`;
- собрать summary по entity timeline;
- предложить next steps без write-действий.

## Рекомендуемые API endpoints

### `GET /api/1/analytics-1c/summary`

Параметры:

- `infobase`
- `from`
- `to`

Возвращает:

- sales
- returns
- overdue_receivables
- detections_by_severity
- open_cases

### `GET /api/1/analytics-1c/detections`

Фильтры:

- `infobase`
- `severity`
- `rule_id`
- `entity_type`
- `entity_id`

### `GET /api/1/analytics-1c/timeline`

Фильтры:

- `entity_type`
- `entity_id`
- `from`
- `to`

### `GET /api/1/analytics-1c/cases/{case_id}`

Возвращает:

- case card
- related detections
- related timeline rows

## Guardrails

- read-only SQL;
- whitelist queries;
- no direct write-back into 1С;
- no direct execution of arbitrary SQL from prompt;
- all investigator requests are logged.

## Output style

AI Investigator должен выдавать:

1. краткую суть;
2. что именно найдено;
3. почему это важно;
4. что проверить дальше;
5. ссылки на case/timeline.
