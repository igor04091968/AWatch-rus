# 1C Business Event Layer

Это следующий production-шаг поверх уже работающего контура
`file 1C -> ClickHouse -> Grafana -> AI Investigator`.

Цель не в том, чтобы заменить текущий `company intelligence`, а в том, чтобы
добавить **read-only business-event слой**, пригодный для финансовых
расследований, explainability и rule-based detections по бухгалтерскому смыслу.

## Почему этот слой нужен

Текущий контур уже решает:

- operational telemetry;
- timeline по reglog/audit;
- detections/cases;
- company portfolio intelligence;
- manager/recovery briefs.

Но он ещё не даёт полноценного ответа на вопросы уровня:

- какие проводки дали вклад в аномалию;
- кто и когда перепровёл документ;
- какие изменения реквизитов повлияли на результат;
- почему вырос НДС, возвраты или нетипичные движения.

Для этого нужен отдельный канонический слой бизнес-событий.

## Слои данных

### 1. `documents`

Карточки документов и базовые агрегаты по документам.

### 2. `postings`

Лёгкий слой проводок/движений. Уже есть в контуре, но он недостаточен как
канонический event stream.

### 3. `business_events`

Новый целевой канонический слой. Один ряд = одно бизнес-событие, пригодное для:

- timeline;
- detections;
- explainability;
- AI investigations;
- correlation с reglog/audit/cases.

Текущая schema scaffold:

- `ts`
- `event_id`
- `infobase`
- `company_entity_key`
- `organization`
- `department`
- `document_id`
- `document_number`
- `document_type`
- `registrar`
- `operation_type`
- `event_kind`
- `user`
- `counterparty`
- `counterparty_inn`
- `debit_account`
- `credit_account`
- `amount`
- `currency`
- `line_no`
- `evidence_ref`
- `source_file`

### 4. `document_change_events`

Новый слой изменений документов и реквизитов.

Нужен для:

- расследования перепроведений;
- контроля изменений реквизитов;
- reconstruction narrative;
- объяснения, какие именно изменения дали бизнес-эффект.

Текущая schema scaffold:

- `ts`
- `change_id`
- `infobase`
- `company_entity_key`
- `organization`
- `document_id`
- `document_number`
- `document_type`
- `change_kind`
- `field_name`
- `user`
- `before_value`
- `after_value`
- `risk_tag`
- `evidence_ref`
- `source_file`

## Read-only extraction path

Правильный extraction path такой:

1. Внешний extractor читает только безопасные read-only источники.
2. Формирует `jsonl/csv` в landing-каталоги.
3. `aw-1c-ingest-rust` грузит данные в raw/core ClickHouse tables.
4. Detection/AI слой работает только с ClickHouse.

То есть LLM и manager pages не ходят в 1С напрямую.

## Безопасные источники для extractor

Подходящие:

- регламентированные выгрузки документов/движений;
- журнал регистрации 1С;
- внешние реестры и справочники;
- audit/export критичных изменений;
- отдельные read-only файлы, формируемые рядом с 1С.
- `1c-mcp-toolkit` REST API через `execute_query` и `get_event_log`.

Не подходящие по умолчанию:

- запись в файловую базу;
- опасные `COM`/`Configurator` сценарии;
- любой write-back path в production 1С.

## Почему здесь нужен `company_entity_key`

Этот слой не должен зависеть от того, как компания названа в текущий момент в
1С. Поэтому новые event tables уже сразу завязаны на `company_entity_key`.

Приоритет идентификации:

1. `baseid:<base_id>`
2. `basepath:<normalized path>`
3. только fallback на human name

Это делает timeline и расследования устойчивыми к rename.

## Что extractor должен уметь первым

Минимальный read-only extractor v1 должен уметь:

- выгружать документы;
- выгружать проводки;
- выгружать `business_events`;
- выгружать `document_change_events`;
- стабильно наполнять `company_entity_key`;
- писать `evidence_ref`, чтобы расследование не было бездоказательным.

## Первые детекты на этом слое

После появления business-event выгрузок стоит вводить:

- ночные проводки;
- дробление платежей;
- повторные перепроведения;
- изменение реквизитов перед/после движения;
- возвраты после закрытия периода;
- нехарактерные движения по пользователю;
- циклические движения по контрагенту/счётам.

## Что уже сделано в репо

Уже добавлены:

- raw tables:
  - `raw_1c_business_events`
  - `raw_1c_document_changes`
- core tables:
  - `business_events`
  - `document_change_events`
- ETL support:
  - `landing/business_events`
  - `landing/document_changes`
  - dataset mapping в `aw-1c-ingest-rust`
- built-in normalizer:
  - `etl/build_business_event_exports.py`
  - собирает canonical events из существующих read-only выгрузок
    `documents/postings/audit`
- read-only extractor scaffold:
  - `etl/extract_1c_mcp_toolkit.py`
  - забирает `companies/documents/postings/business_events/document_changes`
    через `execute_query`
  - забирает `reglog` через `get_event_log`
  - пишет в те же `landing/*`, которые уже понимает loader
- ingest wiring:
  - normalizer встроен в production writer `aw-1c-ingest-rust`
- timeline/detection wiring:
  - `business_events` и `document_change_events` уже входят в
    `entity_timeline`
  - на этом слое уже есть первые detections:
    - крупная корректировка проводки
    - рискованное изменение документа

Это уже рабочий v1 normalizer, но ещё не конечный extractor со всей
бухгалтерской глубиной.

## Контракт `1c-mcp-toolkit` extractor

На текущем шаге extractor сознательно ограничен:

- только read-only endpoint'ы;
- без `execute_code`;
- с локальным checkpoint state;
- с `channel` isolation для разделения `dev/prod`.

Практический контракт такой:

1. `execute_query` должен возвращать колонки, алиаснутые под landing-схему.
2. Если это неудобно, используется `field_map` в `etl/config.yml`.
3. Для query-datasets можно включать incremental-window через
   `incremental.since_param` / `incremental.until_param`.
4. Для `reglog` extractor хранит курсор `last_date +
   same_second_offset`.
5. `company_entity_key` достраивается extractor'ом по той же логике,
   что уже используется в built-in normalizer.
6. Перед включением live-ingest query-pack должен проходить:
   - `--validate-config`
   - `--dry-run`
   - targeted `--dataset <name>` probe

## Что делать дальше

1. Подключить production queries под конкретную конфигурацию 1С.
2. Стабильно наполнять `company_entity_key`.
3. Добавить первые SQL detections на `business_events`.
4. Обогащать `entity_timeline` уже не только telemetry/audit, но и
   business-event evidence.
5. Включить AI Investigator narrative по реальным проводкам и изменениям.
