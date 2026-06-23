# План внедрения ClickHouse Dictionaries и агрегатов для AWatch-rus / DetMir

Дата: `2026-06-23`

Статус: рабочий архитектурный план.

## 1. Общая цель

Цель внедрения ClickHouse Dictionaries и Materialized Views - подготовить
AWatch-rus / DetMir к росту объема данных и числу пользователей отчетности.

Сырые события ActivityWatch остаются в fact-таблицах. Отчеты и Grafana не
должны постоянно выполнять тяжелые `JOIN` и агрегации по миллионам или
миллиардам строк. Обогащение и схлопывание данных нужно переносить на этап
записи, backfill или scheduled aggregation.

Базовая схема:

```text
raw ActivityWatch events
    -> dimension tables
    -> ClickHouse Dictionaries
    -> Materialized Views / aggregate tables
    -> Grafana / reports
```

Dictionaries не являются источником истины. Источник истины - обычные таблицы
измерений, импорт из NetBox/XLS/инвентаризации или утвержденные ручные
справочники.

## 2. Спринт 1. Оргструктура и слепые зоны

### Цель

Связать сырые события AWatch-rus с оргструктурой компании и выявить слепые зоны
в привязке рабочих мест, пользователей и подразделений.

### Решение

Создаются:

- `dim_workstation_user` - dimension/current snapshot;
- `dict_workstation_user` - словарь для lookup по паре `host_name + user_login`.

Так как ключ составной, используется `COMPLEX_KEY_HASHED()`.

### Source table

```sql
CREATE TABLE IF NOT EXISTS dim_workstation_user
(
    host_name String,
    user_login String,

    employee_name String,
    department String,
    branch String,
    position String,

    source LowCardinality(String),
    is_active UInt8 DEFAULT 1,
    updated_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(updated_at)
ORDER BY (host_name, user_login);
```

### Dictionary

```sql
CREATE DICTIONARY IF NOT EXISTS dict_workstation_user
(
    host_name String,
    user_login String,

    employee_name String,
    department String,
    branch String,
    position String,
    is_active UInt8
)
PRIMARY KEY host_name, user_login
SOURCE(CLICKHOUSE(TABLE 'dim_workstation_user'))
LAYOUT(COMPLEX_KEY_HASHED())
LIFETIME(MIN 3600 MAX 86400);
```

### Отчет по слепым зонам

```sql
SELECT
    host_name,
    user_login,
    count() AS events
FROM aw_raw_events
WHERE dictGetStringOrDefault(
    'dict_workstation_user',
    'employee_name',
    (host_name, user_login),
    ''
) = ''
GROUP BY host_name, user_login
ORDER BY events DESC
LIMIT 100;
```

### Definition of Done

- создана таблица `dim_workstation_user`;
- создан словарь `dict_workstation_user`;
- словарь находится в статусе `LOADED`;
- есть стартовая загрузка данных;
- есть отчет unknown `host_name + user_login`;
- есть первый обогащенный запрос по сырым событиям;
- документирован источник истины для оргструктуры.

## 3. Спринт 2. Продуктивность и классификация desktop software

### Цель

Уйти от анализа миллионов строк процессов к понятным бизнес-метрикам по
использованию ПО на рабочих местах.

Сырые события должны обогащаться не только `process_name`, а нормализованным
именем приложения, категорией, признаком продуктивности и уровнем риска.

### Решение

Создаются:

- `dim_application_category`;
- `dict_application_category`;
- отчет top unknown processes;
- первый отчет продуктивности по отделам и сотрудникам.

### Source table

```sql
CREATE TABLE IF NOT EXISTS dim_application_category
(
    process_name String,

    application_name String,
    vendor String,
    category LowCardinality(String),
    productivity_class LowCardinality(String),
    risk_level LowCardinality(String),

    is_system UInt8 DEFAULT 0,
    is_active UInt8 DEFAULT 1,
    source LowCardinality(String),
    comment String,
    updated_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(updated_at)
ORDER BY process_name;
```

### Dictionary

```sql
CREATE DICTIONARY IF NOT EXISTS dict_application_category
(
    process_name String,

    application_name String,
    vendor String,
    category String,
    productivity_class String,
    risk_level String,
    is_system UInt8,
    is_active UInt8
)
PRIMARY KEY process_name
SOURCE(CLICKHOUSE(TABLE 'dim_application_category'))
LAYOUT(HASHED())
LIFETIME(MIN 3600 MAX 86400);
```

### Базовые классы продуктивности

| Значение | Смысл |
|---|---|
| `productive` | рабочее приложение |
| `neutral` | системное или вспомогательное приложение |
| `non_productive` | явно нерабочее использование |
| `risky` | потенциальный риск ИБ или нежелательное ПО |
| `unknown` | нет классификации |

### Отчет unknown processes

```sql
SELECT
    process_name,
    count() AS events,
    sum(duration_sec) AS duration_sec
FROM aw_window_events
WHERE dictGetStringOrDefault(
    'dict_application_category',
    'category',
    process_name,
    ''
) = ''
GROUP BY process_name
ORDER BY duration_sec DESC
LIMIT 100;
```

### Definition of Done

- создана таблица `dim_application_category`;
- создан словарь `dict_application_category`;
- загружен стартовый список известных процессов;
- есть top unknown processes;
- есть отчет продуктивности в разрезе отдела/сотрудника;
- классификация не используется как дисциплинарный вывод без ручной проверки;
- назначен владелец справочника классификации ПО.

## 4. Спринт 3. Веб-аналитика и глубинная фильтрация

### Цель

Перейти от анализа заголовков окон браузера и полных URL к нормальной
веб-аналитике: домены, категории сайтов, рабочая/нерабочая активность,
рисковые ресурсы и unknown domains.

### Решение

Создаются:

- нормализация URL в `domain_name`;
- `dim_domain_category`;
- `dict_domain_category`;
- top unknown domains;
- browser productivity report;
- задел под path-level правила.

### Нормализация URL

```sql
SELECT
    url,
    lowerUTF8(
        parseURL(
            if(position(url, '://') = 0, concat('http://', url), url),
            'host'
        )
    ) AS domain_name
FROM aw_browser_events;
```

Если в данных уже есть корректный полный URL, можно использовать `domain(url)`.
Если URL спрятан только в title, regex parsing допускается как fallback, но не
как основной путь.

### Source table

```sql
CREATE TABLE IF NOT EXISTS dim_domain_category
(
    domain String,

    site_name String,
    category LowCardinality(String),
    productivity_class LowCardinality(String),
    risk_level LowCardinality(String),
    business_allowed UInt8 DEFAULT 0,

    source LowCardinality(String),
    comment String,
    is_active UInt8 DEFAULT 1,
    updated_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(updated_at)
ORDER BY domain;
```

### Dictionary

```sql
CREATE DICTIONARY IF NOT EXISTS dict_domain_category
(
    domain String,

    site_name String,
    category String,
    productivity_class String,
    risk_level String,
    business_allowed UInt8,
    is_active UInt8
)
PRIMARY KEY domain
SOURCE(CLICKHOUSE(TABLE 'dim_domain_category'))
LAYOUT(HASHED())
LIFETIME(MIN 3600 MAX 86400);
```

### Начальные категории доменов

| Категория | Смысл |
|---|---|
| `internal_service` | внутренние корпоративные сервисы |
| `banking` | банковские и финансовые ресурсы |
| `government` | государственные сервисы |
| `work_service` | рабочие SaaS/порталы/документация |
| `developer` | Git, документация, package registry |
| `mail` | почтовые сервисы |
| `messenger` | web-мессенджеры |
| `cloud_storage` | облачные хранилища |
| `search` | поисковые системы |
| `news` | новости |
| `social` | социальные сети |
| `media` | видео/аудио/стриминг |
| `shopping` | покупки и маркетплейсы |
| `job_search` | сайты поиска работы |
| `ai_service` | внешние AI-сервисы |
| `unknown` | не классифицировано |
| `risky` | рискованный или нежелательный ресурс |

### Path-level правила

Классификация по домену не всегда достаточна. Для `github.com`, `youtube.com`,
`docs.google.com`, `mail.ru` может потребоваться учет path или контекста.

Задел под будущую детализацию:

```sql
CREATE TABLE IF NOT EXISTS dim_url_rule
(
    rule_id String,
    domain String,
    path_pattern String,

    category LowCardinality(String),
    productivity_class LowCardinality(String),
    risk_level LowCardinality(String),

    priority UInt16 DEFAULT 100,
    is_active UInt8 DEFAULT 1,
    comment String,
    updated_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(updated_at)
ORDER BY (domain, priority, rule_id);
```

На спринте 3 `dim_url_rule` можно вести как backlog правил и применять точечно.

### Definition of Done

- создана таблица `dim_domain_category`;
- создан словарь `dict_domain_category`;
- есть нормализация URL в `domain_name`;
- есть стартовая классификация top domains;
- есть top unknown domains;
- есть browser productivity report;
- ограничения доменной модели и необходимость path-level правил задокументированы.

## 5. Спринт 4. Enterprise-масштабирование и стабильность

### Цель

Гарантировать, что система не ляжет при росте компании и объема событий.
Grafana не должна при каждом открытии dashboard на лету агрегировать миллиарды
сырых строк.

### Ключевое архитектурное правило

Materialized Views должны сразу складывать в агрегированные таблицы уже
обогащенные измерения, полученные через dictionaries.

То есть Grafana должна читать не `user_id` и не `process_name`, требующие
дальнейшего JOIN, а готовые бизнес-срезы:

```text
Дата / Час
Отделение
Отдел
Сотрудник
Категория продуктивности
Категория приложения или домена
Сумма секунд
Количество событий
```

Это переносит CPU-нагрузку с момента открытия dashboard на момент insert/backfill
и практически убирает тяжелые вычисления из пользовательских запросов Grafana.

### Общая схема

```text
raw events
    -> Materialized View с dictGet* enrichment
    -> SummingMergeTree / AggregatingMergeTree aggregate table
    -> Grafana читает готовые агрегаты
```

Raw tables остаются для расследований и drill-down. Штатные dashboards должны
читать агрегаты.

### Выбор движка

| Движок | Где использовать |
|---|---|
| `SummingMergeTree` | суммы duration/count, основные dashboards |
| `AggregatingMergeTree` | uniq, quantile, topK, сложные агрегатные состояния |

Рекомендация: начинать с `SummingMergeTree`. `AggregatingMergeTree` подключать
только при доказанной необходимости.

### Desktop hourly aggregate

```sql
CREATE TABLE IF NOT EXISTS agg_aw_desktop_hourly
(
    date Date,
    hour DateTime,

    branch String,
    department String,
    employee_name String,
    host_name String,
    user_login String,

    application_name String,
    app_category String,
    productivity_class String,
    risk_level String,

    duration_sec UInt64,
    event_count UInt64
)
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(date)
ORDER BY
(
    date,
    hour,
    branch,
    department,
    employee_name,
    productivity_class,
    app_category,
    application_name,
    host_name,
    user_login
);
```

```sql
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_aw_desktop_hourly
TO agg_aw_desktop_hourly
AS
SELECT
    toDate(event_time) AS date,
    toStartOfHour(event_time) AS hour,

    dictGetStringOrDefault('dict_workstation_user', 'branch', (host_name, user_login), 'unknown') AS branch,
    dictGetStringOrDefault('dict_workstation_user', 'department', (host_name, user_login), 'unknown') AS department,
    dictGetStringOrDefault('dict_workstation_user', 'employee_name', (host_name, user_login), 'unknown') AS employee_name,

    host_name,
    user_login,

    dictGetStringOrDefault('dict_application_category', 'application_name', process_name, process_name) AS application_name,
    dictGetStringOrDefault('dict_application_category', 'category', process_name, 'unknown') AS app_category,
    dictGetStringOrDefault('dict_application_category', 'productivity_class', process_name, 'unknown') AS productivity_class,
    dictGetStringOrDefault('dict_application_category', 'risk_level', process_name, 'unknown') AS risk_level,

    sum(duration_sec) AS duration_sec,
    count() AS event_count
FROM aw_window_events
GROUP BY
    date,
    hour,
    branch,
    department,
    employee_name,
    host_name,
    user_login,
    application_name,
    app_category,
    productivity_class,
    risk_level;
```

### Browser hourly aggregate

```sql
CREATE TABLE IF NOT EXISTS agg_aw_browser_hourly
(
    date Date,
    hour DateTime,

    branch String,
    department String,
    employee_name String,
    host_name String,
    user_login String,

    domain_name String,
    site_name String,
    domain_category String,
    productivity_class String,
    risk_level String,

    duration_sec UInt64,
    event_count UInt64
)
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(date)
ORDER BY
(
    date,
    hour,
    branch,
    department,
    employee_name,
    productivity_class,
    domain_category,
    domain_name,
    host_name,
    user_login
);
```

```sql
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_aw_browser_hourly
TO agg_aw_browser_hourly
AS
WITH
    lowerUTF8(
        parseURL(
            if(position(url, '://') = 0, concat('http://', url), url),
            'host'
        )
    ) AS domain_name
SELECT
    toDate(event_time) AS date,
    toStartOfHour(event_time) AS hour,

    dictGetStringOrDefault('dict_workstation_user', 'branch', (host_name, user_login), 'unknown') AS branch,
    dictGetStringOrDefault('dict_workstation_user', 'department', (host_name, user_login), 'unknown') AS department,
    dictGetStringOrDefault('dict_workstation_user', 'employee_name', (host_name, user_login), 'unknown') AS employee_name,

    host_name,
    user_login,
    domain_name,

    dictGetStringOrDefault('dict_domain_category', 'site_name', domain_name, domain_name) AS site_name,
    dictGetStringOrDefault('dict_domain_category', 'category', domain_name, 'unknown') AS domain_category,
    dictGetStringOrDefault('dict_domain_category', 'productivity_class', domain_name, 'unknown') AS productivity_class,
    dictGetStringOrDefault('dict_domain_category', 'risk_level', domain_name, 'unknown') AS risk_level,

    sum(duration_sec) AS duration_sec,
    count() AS event_count
FROM aw_browser_events
WHERE domain_name != ''
GROUP BY
    date,
    hour,
    branch,
    department,
    employee_name,
    host_name,
    user_login,
    domain_name,
    site_name,
    domain_category,
    productivity_class,
    risk_level;
```

### Daily aggregate для управленческих отчетов

Daily layer лучше строить из hourly aggregates, чтобы не дублировать enrichment
логику и не перечитывать raw events.

```sql
CREATE TABLE IF NOT EXISTS agg_aw_activity_daily
(
    date Date,
    branch String,
    department String,
    source_type LowCardinality(String),
    productivity_class String,
    category String,
    duration_sec UInt64,
    event_count UInt64
)
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(date)
ORDER BY
(
    date,
    branch,
    department,
    source_type,
    productivity_class,
    category
);
```

Пример загрузки из desktop hourly:

```sql
INSERT INTO agg_aw_activity_daily
SELECT
    date,
    branch,
    department,
    'desktop' AS source_type,
    productivity_class,
    app_category AS category,
    sum(duration_sec) AS duration_sec,
    sum(event_count) AS event_count
FROM agg_aw_desktop_hourly
WHERE date = yesterday()
GROUP BY
    date,
    branch,
    department,
    source_type,
    productivity_class,
    category;
```

### Почему обогащение нужно делать до Grafana

Если Grafana читает raw tables и выполняет `dictGet*` или `JOIN` при каждом
открытии панели, CPU ClickHouse будет расходоваться на одни и те же вычисления.
При росте компании это станет узким местом.

Если Materialized View уже положила в агрегат строки вида:

```text
2026-06-23 / Сыктывкар / ОТ / productive / office / 18420 sec
```

то Grafana выполняет простой `SELECT sum(duration_sec) ... GROUP BY ...` по
маленькой таблице. Это дает кратный выигрыш и делает dashboard стабильным.

### Backfill и rebuild

Materialized View обрабатывает только новые вставки после создания MV.
Исторические данные нужно пересчитать отдельно.

Порядок:

1. создать aggregate table;
2. создать materialized view для новых данных;
3. выполнить `INSERT INTO aggregate SELECT ... FROM raw WHERE ...` за историю;
4. сверить суммы raw vs aggregate;
5. переключить Grafana на aggregate;
6. документировать rebuild-период, если справочники изменились.

Важно: если словари поменялись после агрегации, старые агрегаты сами не
пересчитаются. Для строгой актуальности нужен rebuild затронутого периода или
snapshot/history dimensions.

### Coverage aggregate для качества справочников

```sql
CREATE TABLE IF NOT EXISTS agg_aw_dictionary_coverage_daily
(
    date Date,
    source_type LowCardinality(String),
    unknown_type LowCardinality(String),
    unknown_key String,
    events UInt64,
    duration_sec UInt64
)
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(date)
ORDER BY (date, source_type, unknown_type, unknown_key);
```

Эта таблица нужна, чтобы видеть:

- unknown workstations;
- unknown processes;
- unknown domains;
- динамику качества справочников.

### Grafana policy

После спринта 4 штатные dashboards должны читать:

- `agg_aw_desktop_hourly`;
- `agg_aw_browser_hourly`;
- `agg_aw_activity_daily`;
- `agg_aw_dictionary_coverage_daily`.

Raw tables допустимы только для расследований, drill-down и технической
диагностики.

### Monitoring

Минимальные проверки:

```sql
SELECT
    table,
    sum(rows) AS rows,
    formatReadableSize(sum(bytes_on_disk)) AS size
FROM system.parts
WHERE active
GROUP BY table
ORDER BY rows DESC;
```

```sql
SELECT
    name,
    status,
    element_count,
    last_exception
FROM system.dictionaries
WHERE name LIKE 'dict_%';
```

### Retention

Начальная рекомендация:

| Слой | Retention |
|---|---|
| raw events | 90-180 дней или по политике проекта |
| hourly aggregates | 12-24 месяца |
| daily aggregates | 3-5 лет или по требованиям отчетности |
| dictionary coverage | 12 месяцев |

TTL включать только после согласования требований отчетности и хранения.

### Definition of Done

Спринт 4 считается завершенным, когда:

- создана `agg_aw_desktop_hourly`;
- создана `mv_aw_desktop_hourly`;
- создана `agg_aw_browser_hourly`;
- создана `mv_aw_browser_hourly`;
- создан daily aggregate layer;
- создан coverage aggregate для unknown значений;
- выполнен backfill хотя бы за один исторический период;
- сверены суммы raw vs aggregate;
- минимум один Grafana dashboard переключен на aggregate table;
- Grafana не выполняет штатные отчеты по raw events;
- документирован rebuild/backfill runbook;
- есть мониторинг таблиц, словарей и unknown coverage.

### Риски

| Риск | Митигация |
|---|---|
| MV не обработала исторические данные | обязательный backfill |
| Ошибка классификации попала в агрегаты | rebuild затронутого периода |
| Слишком высокая кардинальность aggregate | не тащить URL/title в основные агрегаты |
| Grafana продолжает читать raw tables | dashboard review и policy запрета raw для штатных панелей |
| Словари изменились после агрегации | rebuild или snapshot/history dimensions |
| Слишком много parts | контролировать batch insert, partitioning, merge health |
| Сложность `AggregatingMergeTree` | начинать с `SummingMergeTree` |
