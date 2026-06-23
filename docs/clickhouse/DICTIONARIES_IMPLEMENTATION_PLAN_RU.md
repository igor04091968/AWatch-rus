# План внедрения ClickHouse Dictionaries для AWatch-rus / DetMir

Дата: `2026-06-23`

Статус: рабочий архитектурный план. Спринты 1-4 зафиксированы как
последовательность внедрения. Для `SHARKON2025` источники P1 сверены с живым
ActivityWatch API и зафиксированы в
`docs/clickhouse/AW_WORKFORCE_SOURCES_SHARKON2025_RU.md`.

Реализация первого воспроизводимого слоя в репозитории:
`clickhouse-workforce/`. В нем зафиксированы DDL, локальный ClickHouse scaffold,
demo seed и smoke-проверка для dictionaries, materialized views и агрегатов.

P2 реализуется Rust loader-ом `adk-rust/crates/aw-workforce-ingest`, который
читает bounded ranges из ActivityWatch и пишет `JSONEachRow` в
`aw_window_events` / `aw_browser_events`.

P4 добавляет production-режим loader-а: state file с `last_end`, overlap-окно
для защиты от поздних событий, retry/backoff и systemd timer
`clickhouse-workforce/ops/aw-workforce-ingest.timer`.

P5 добавляет администрируемый source of truth для справочников:
`clickhouse-workforce/catalog/*.tsv`. Загрузка выполняется через
`clickhouse-workforce/ops/apply_catalogs.sh`; удаление/отключение строки
делается либо удалением из TSV, либо `is_active=0`. Для уже материализованных
агрегатов после изменения категорий используется
`clickhouse-workforce/admin/rebuild_aggregates.sql`.

## 1. Общий принцип

ClickHouse Dictionaries внедряются как слой быстрого обогащения событий, а не
как новый источник истины.

Источник истины:

```text
исходные справочники / NetBox / XLS / инвентаризация / ручная классификация
        -> dimension tables в ClickHouse
        -> ClickHouse Dictionaries
        -> отчеты, materialized views, Grafana
```

Сырые события AWatch-rus остаются fact-таблицами. Справочники и dictionaries
используются для привязки событий к оргструктуре, приложениям, категориям и
рискам.

## 2. Спринт 1. Оргструктура и слепые зоны

### Цель

Связать сырые события AWatch-rus с оргструктурой компании и выявить слепые зоны
в привязке рабочих мест, пользователей и подразделений.

### Принятое архитектурное решение

Создаются:

- `dim_workstation_user` - dimension/current snapshot, а не таблица фактов;
- `dict_workstation_user` - ClickHouse Dictionary для lookup по паре
  `host_name + user_login`.

Ключ словаря комплексный, поэтому используется `COMPLEX_KEY_HASHED()`, а не
обычный `HASHED()`.

`CACHE()` на первом этапе не используется: справочник рабочих мест должен быть
достаточно небольшим, чтобы держать его целиком в памяти.

### Source table

```sql
CREATE TABLE IF NOT EXISTS dim_workstation_user
(
    host_name String,
    user_login String,
    user_domain String,

    employee_id String,
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
    user_domain String,

    employee_id String,
    employee_name String,
    department String,
    branch String,
    position String,
    is_active UInt8
)
PRIMARY KEY host_name, user_login
SOURCE(CLICKHOUSE(
    TABLE 'dim_workstation_user'
))
LAYOUT(COMPLEX_KEY_HASHED())
LIFETIME(MIN 3600 MAX 86400);
```

### Использование

```sql
SELECT
    event_time,
    host_name,
    user_login,
    dictGetStringOrDefault(
        'dict_workstation_user',
        'employee_name',
        (host_name, user_login),
        'unknown'
    ) AS employee_name,
    dictGetStringOrDefault(
        'dict_workstation_user',
        'department',
        (host_name, user_login),
        'unknown'
    ) AS department,
    dictGetStringOrDefault(
        'dict_workstation_user',
        'branch',
        (host_name, user_login),
        'unknown'
    ) AS branch
FROM aw_raw_events;
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
GROUP BY
    host_name,
    user_login
ORDER BY events DESC
LIMIT 100;
```

### Definition of Done

Спринт 1 считается завершенным, когда:

- создана таблица `dim_workstation_user`;
- создан словарь `dict_workstation_user`;
- словарь находится в статусе `LOADED`;
- есть стартовая загрузка данных;
- есть SQL-отчет по неизвестным `host_name + user_login`;
- есть хотя бы один обогащенный запрос по сырым событиям;
- есть проверка состояния словаря через `system.dictionaries`;
- документирован источник истины для оргструктуры.

## 3. Спринт 2. Продуктивность и классификация софта

### Цель

Уйти от анализа миллионов строк процессов к понятным бизнес-метрикам по
использованию программного обеспечения на рабочих местах.

События рабочего стола должны обогащаться не только именем процесса, а
категорией приложения, признаком продуктивности, уровнем риска и нормализованным
именем приложения.

### Что должно появиться

Создаются:

- `dim_application_category` - справочник классификации процессов и приложений;
- `dict_application_category` - ClickHouse Dictionary для lookup по
  `process_name`;
- отчет по неизвестным процессам;
- первый отчет/витрина продуктивности по отделам и сотрудникам.

### Бизнес-метрики спринта 2

Минимальный набор метрик:

- время в продуктивных приложениях;
- время в условно нейтральных приложениях;
- время в непродуктивных приложениях;
- время в административных/технических инструментах;
- время в неизвестных процессах;
- top unknown processes по числу событий и длительности;
- top risky processes;
- разрез по отделу, филиалу, сотруднику, рабочей станции.

### Категории приложений

Начальный набор категорий:

| Категория | Смысл |
|---|---|
| `office` | офисные приложения, документы, таблицы |
| `browser` | браузеры без оценки домена |
| `1c` | 1C и связанные клиенты |
| `banking` | банковские и внутренние рабочие приложения |
| `messenger` | корпоративные и внешние мессенджеры |
| `mail` | почтовые клиенты |
| `admin_tool` | администрирование, диагностика, удаленный доступ |
| `development` | разработка, скрипты, IDE |
| `security_tool` | средства защиты, DLP, AV, мониторинг |
| `media` | аудио/видео/плееры |
| `archive` | архиваторы, файловые утилиты |
| `system` | системные процессы ОС |
| `unknown` | не классифицировано |
| `risky` | потенциально рискованный или нежелательный софт |

### Признак продуктивности

Поле `productivity_class` используется отдельно от категории.

Начальные значения:

| Значение | Смысл |
|---|---|
| `productive` | рабочее приложение |
| `neutral` | системное или вспомогательное приложение |
| `non_productive` | явно нерабочее использование |
| `risky` | потенциальный риск ИБ или нежелательное ПО |
| `unknown` | нет классификации |

Важно: продуктивность не должна автоматически считаться дисциплинарным выводом.
Это аналитическая классификация для управленческого отчета и выявления слепых
зон.

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

Для спринта 2 используется обычный `HASHED()`, потому что ключ один:
`process_name`.

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
SOURCE(CLICKHOUSE(
    TABLE 'dim_application_category'
))
LAYOUT(HASHED())
LIFETIME(MIN 3600 MAX 86400);
```

### Использование в запросах

```sql
SELECT
    event_time,
    host_name,
    user_login,
    process_name,
    dictGetStringOrDefault(
        'dict_application_category',
        'application_name',
        process_name,
        process_name
    ) AS application_name,
    dictGetStringOrDefault(
        'dict_application_category',
        'category',
        process_name,
        'unknown'
    ) AS app_category,
    dictGetStringOrDefault(
        'dict_application_category',
        'productivity_class',
        process_name,
        'unknown'
    ) AS productivity_class,
    dictGetStringOrDefault(
        'dict_application_category',
        'risk_level',
        process_name,
        'unknown'
    ) AS risk_level
FROM aw_window_events;
```

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

### Пример витрины продуктивности

```sql
SELECT
    toDate(event_time) AS date,
    dictGetStringOrDefault(
        'dict_workstation_user',
        'department',
        (host_name, user_login),
        'unknown'
    ) AS department,
    dictGetStringOrDefault(
        'dict_workstation_user',
        'employee_name',
        (host_name, user_login),
        'unknown'
    ) AS employee_name,
    dictGetStringOrDefault(
        'dict_application_category',
        'productivity_class',
        process_name,
        'unknown'
    ) AS productivity_class,
    sum(duration_sec) AS duration_sec
FROM aw_window_events
GROUP BY
    date,
    department,
    employee_name,
    productivity_class
ORDER BY
    date,
    department,
    employee_name,
    productivity_class;
```

### Definition of Done

Спринт 2 считается завершенным, когда:

- создана таблица `dim_application_category`;
- создан словарь `dict_application_category`;
- словарь находится в статусе `LOADED`;
- загружен стартовый список известных процессов;
- есть отчет top unknown processes;
- есть отчет по продуктивности в разрезе отдела/сотрудника;
- неизвестные процессы видны как отдельная зона качества данных;
- классификация не используется как дисциплинарный вывод без ручной проверки;
- документирован владелец справочника классификации ПО.

## 4. Спринт 3. Веб-аналитика и глубинная фильтрация

### Цель

Перейти от анализа сырых заголовков окон браузера и полных URL к нормальной
веб-аналитике: домены, категории сайтов, признаки рабочей/нерабочей активности,
рисковые ресурсы и слепые зоны классификации.

Для современного сотрудника значительная часть работы идет в браузере, поэтому
без доменной классификации отчеты по продуктивности будут неполными: браузер
сам по себе не показывает, была ли активность рабочей, нейтральной или
нежелательной.

### Что должно появиться

Создаются:

- нормализованное поле `domain` в запросах, view или materialized view;
- `dim_domain_category` - справочник классификации доменов;
- `dict_domain_category` - ClickHouse Dictionary для lookup по `domain`;
- отчет top unknown domains;
- отчет browser activity по категориям сайтов;
- правила глубинной фильтрации для URL/path, если одного домена недостаточно.

### Нормализация URL в домен

Источник может содержать:

- полный URL: `https://github.com/org/repo/issues`;
- адрес без схемы: `github.com/org/repo`;
- заголовок окна с доменом внутри строки;
- browser bucket с отдельным URL-полем;
- пустые или неполные значения.

Базовое правило: сначала извлекать чистый домен, потом классифицировать его
через dictionary.

Пример для поля с полным URL:

```sql
SELECT
    url,
    lowerUTF8(domain(url)) AS domain_name
FROM aw_browser_events;
```

Если `domain()` недостаточно из-за строк без схемы, использовать нормализацию:

```sql
SELECT
    url,
    lowerUTF8(
        domain(
            if(
                position(url, '://') = 0,
                concat('http://', url),
                url
            )
        )
    ) AS domain_name
FROM aw_browser_events;
```

На ClickHouse `24.8` в локальном scaffold проверена функция `domain()`:

```sql
SELECT
    url,
    lowerUTF8(
        domain(if(position(url, '://') = 0, concat('http://', url), url))
    ) AS domain_name
FROM aw_browser_events;
```

Для строк, где URL спрятан в title, допустим отдельный regex-pass, но его не
нужно делать основным путем, если в событиях уже есть URL.

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

Для домена используется один ключ, поэтому достаточно `HASHED()`.

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
SOURCE(CLICKHOUSE(
    TABLE 'dim_domain_category'
))
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
| `developer` | Git, документация, package registry, developer tools |
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

### Признак продуктивности для доменов

Используется тот же подход, что в спринте 2:

| Значение | Смысл |
|---|---|
| `productive` | рабочий ресурс |
| `neutral` | вспомогательный или неоднозначный ресурс |
| `non_productive` | явно нерабочий ресурс |
| `risky` | потенциальный ИБ-риск или нежелательный ресурс |
| `unknown` | нет классификации |

Важно: продуктивность домена зависит от контекста. Например, `youtube.com` для
одного подразделения может быть учебным ресурсом, а для другого -
непродуктивной активностью. В спринте 3 это фиксируется как ограничение модели.
Тонкую контекстную политику по подразделениям лучше выносить в следующий этап.

### Использование в запросах

```sql
WITH
    lowerUTF8(
        domain(if(position(url, '://') = 0, concat('http://', url), url))
    ) AS domain_name
SELECT
    event_time,
    host_name,
    user_login,
    url,
    domain_name,
    dictGetStringOrDefault(
        'dict_domain_category',
        'site_name',
        domain_name,
        domain_name
    ) AS site_name,
    dictGetStringOrDefault(
        'dict_domain_category',
        'category',
        domain_name,
        'unknown'
    ) AS domain_category,
    dictGetStringOrDefault(
        'dict_domain_category',
        'productivity_class',
        domain_name,
        'unknown'
    ) AS productivity_class,
    dictGetStringOrDefault(
        'dict_domain_category',
        'risk_level',
        domain_name,
        'unknown'
    ) AS risk_level
FROM aw_browser_events;
```

### Отчет unknown domains

```sql
WITH
    lowerUTF8(
        domain(if(position(url, '://') = 0, concat('http://', url), url))
    ) AS domain_name
SELECT
    domain_name,
    count() AS events,
    sum(duration_sec) AS duration_sec
FROM aw_browser_events
WHERE domain_name != ''
  AND dictGetStringOrDefault(
        'dict_domain_category',
        'category',
        domain_name,
        ''
      ) = ''
GROUP BY domain_name
ORDER BY duration_sec DESC
LIMIT 100;
```

### Витрина browser productivity

```sql
WITH
    lowerUTF8(
        domain(if(position(url, '://') = 0, concat('http://', url), url))
    ) AS domain_name
SELECT
    toDate(event_time) AS date,
    dictGetStringOrDefault(
        'dict_workstation_user',
        'department',
        (host_name, user_login),
        'unknown'
    ) AS department,
    dictGetStringOrDefault(
        'dict_workstation_user',
        'employee_name',
        (host_name, user_login),
        'unknown'
    ) AS employee_name,
    dictGetStringOrDefault(
        'dict_domain_category',
        'category',
        domain_name,
        'unknown'
    ) AS domain_category,
    dictGetStringOrDefault(
        'dict_domain_category',
        'productivity_class',
        domain_name,
        'unknown'
    ) AS productivity_class,
    sum(duration_sec) AS duration_sec
FROM aw_browser_events
WHERE domain_name != ''
GROUP BY
    date,
    department,
    employee_name,
    domain_category,
    productivity_class
ORDER BY
    date,
    department,
    employee_name,
    duration_sec DESC;
```

### Глубинная фильтрация

Классификация только по домену не всегда достаточна.

Примеры:

- `github.com` может быть рабочим ресурсом, но конкретный path может относиться
  к личным репозиториям;
- `youtube.com` может быть обучением или развлечением;
- `docs.google.com` может быть рабочим или личным ресурсом;
- `mail.ru` может быть почтой, новостями или облаком.

В спринте 3 нужно заложить второй уровень правил, но не усложнять основную
модель. Для этого создается отдельная таблица правил URL/path.

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

На первом этапе `dim_url_rule` можно не подключать как dictionary. Достаточно
вести ее как backlog правил и применять точечно в offline-отчетах или
materialized view, если появится необходимость.

### Definition of Done

Спринт 3 считается завершенным, когда:

- создана таблица `dim_domain_category`;
- создан словарь `dict_domain_category`;
- словарь находится в статусе `LOADED`;
- есть нормализация URL в `domain_name` через `domain()`;
- есть стартовая классификация top domains;
- есть отчет top unknown domains;
- есть отчет browser productivity по отделу/сотруднику;
- известные рабочие ресурсы классифицированы отдельно от неизвестных;
- risky/non-productive домены не используются как дисциплинарный вывод без
  ручной проверки;
- ограничения доменной модели и необходимость path-level правил задокументированы.

### Риски спринта 3

| Риск | Митигация |
|---|---|
| URL не всегда есть в событии | использовать URL-поле, а title parsing оставить fallback |
| Домен не отражает реальное назначение страницы | добавить `dim_url_rule` для path-level правил |
| Ошибочная классификация домена | review справочника и поле `comment` |
| Большая доля unknown domains | ежедневный top unknown отчет |
| Внешние AI/cloud сервисы требуют отдельной политики | вынести `ai_service` и `cloud_storage` в отдельные категории |
| Продуктивность зависит от подразделения | оставить как ограничение спринта 3, контекстную политику вынести позже |

## 5. Спринт 4. Enterprise-масштабирование и стабильность

### Цель

Снять нагрузку с Grafana и ClickHouse при росте объема событий: dashboard'ы не
должны каждый раз агрегировать миллионы или миллиарды строк сырых событий.

Для этого создается слой materialized views, который при вставке новых событий
сразу складывает в агрегированные таблицы уже обогащенные метрики:

```text
raw AWatch-rus events
        -> dictionaries lookup during INSERT
        -> hourly aggregate tables
        -> Grafana queries over small aggregates
```

Ключевое решение: materialized views должны записывать в агрегированную таблицу
не только технические `host_name`, `user_login`, `process_name` или `domain`, а
готовые управленческие разрезы:

- дата/час;
- филиал;
- отделение/подразделение;
- тип активности: `desktop` или `browser`;
- категория приложения или домена;
- класс продуктивности;
- сумма секунд;
- количество событий;
- счетчики unknown-зон.

Это снижает CPU-нагрузку на ClickHouse и делает Grafana предсказуемой по
времени ответа.

### Принятое архитектурное решение

Для первого промышленного слоя используется `SummingMergeTree`, потому что
базовые метрики аддитивны:

- `duration_sec`;
- `event_count`;
- `unknown_subject_events`;
- `unknown_category_events`.

`AggregatingMergeTree` нужен позже, если появятся сложные состояния:
`uniqState`, `quantileState`, percentile latency, distinct users/workstations
или rolling windows. Для Спринта 4 он не является обязательным.

Важно: dictionary lookup внутри materialized view выполняется в момент вставки.
Это хорошо для скорости Grafana, но означает, что исправление справочника не
пересчитает старые агрегаты автоматически. Для этого нужен явный backfill.

### Target table для Grafana

```sql
CREATE TABLE IF NOT EXISTS agg_workforce_productivity_hourly
(
    bucket_start DateTime,
    event_date Date,

    branch LowCardinality(String),
    department LowCardinality(String),

    activity_type LowCardinality(String),
    category LowCardinality(String),
    productivity_class LowCardinality(String),

    duration_sec UInt64,
    event_count UInt64,
    unknown_subject_events UInt64,
    unknown_category_events UInt64
)
ENGINE = SummingMergeTree((
    duration_sec,
    event_count,
    unknown_subject_events,
    unknown_category_events
))
PARTITION BY toYYYYMM(event_date)
ORDER BY (
    event_date,
    bucket_start,
    branch,
    department,
    activity_type,
    productivity_class,
    category
);
```

`bucket_start` хранит часовую гранулярность. Дневные отчеты строятся простой
агрегацией поверх этой таблицы, без чтения raw-событий.

### Materialized view для desktop events

```sql
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_desktop_productivity_hourly
TO agg_workforce_productivity_hourly
AS
SELECT
    toStartOfHour(event_time) AS bucket_start,
    toDate(event_time) AS event_date,

    dictGetStringOrDefault(
        'dict_workstation_user',
        'branch',
        (host_name, user_login),
        'unknown'
    ) AS branch,
    dictGetStringOrDefault(
        'dict_workstation_user',
        'department',
        (host_name, user_login),
        'unknown'
    ) AS department,

    'desktop' AS activity_type,
    dictGetStringOrDefault(
        'dict_application_category',
        'category',
        process_name,
        'unknown'
    ) AS category,
    dictGetStringOrDefault(
        'dict_application_category',
        'productivity_class',
        process_name,
        'unknown'
    ) AS productivity_class,

    toUInt64(sum(duration_sec)) AS duration_sec,
    toUInt64(count()) AS event_count,
    toUInt64(sum(if(
        dictGetStringOrDefault(
            'dict_workstation_user',
            'employee_name',
            (host_name, user_login),
            ''
        ) = '',
        1,
        0
    ))) AS unknown_subject_events,
    toUInt64(sum(if(
        dictGetStringOrDefault(
            'dict_application_category',
            'category',
            process_name,
            ''
        ) = '',
        1,
        0
    ))) AS unknown_category_events
FROM aw_window_events
GROUP BY
    bucket_start,
    event_date,
    branch,
    department,
    activity_type,
    category,
    productivity_class;
```

### Materialized view для browser events

```sql
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_browser_productivity_hourly
TO agg_workforce_productivity_hourly
AS
WITH
    lowerUTF8(
        domain(if(position(url, '://') = 0, concat('http://', url), url))
    ) AS domain_name
SELECT
    toStartOfHour(event_time) AS bucket_start,
    toDate(event_time) AS event_date,

    dictGetStringOrDefault(
        'dict_workstation_user',
        'branch',
        (host_name, user_login),
        'unknown'
    ) AS branch,
    dictGetStringOrDefault(
        'dict_workstation_user',
        'department',
        (host_name, user_login),
        'unknown'
    ) AS department,

    'browser' AS activity_type,
    dictGetStringOrDefault(
        'dict_domain_category',
        'category',
        domain_name,
        'unknown'
    ) AS category,
    dictGetStringOrDefault(
        'dict_domain_category',
        'productivity_class',
        domain_name,
        'unknown'
    ) AS productivity_class,

    toUInt64(sum(duration_sec)) AS duration_sec,
    toUInt64(count()) AS event_count,
    toUInt64(sum(if(
        dictGetStringOrDefault(
            'dict_workstation_user',
            'employee_name',
            (host_name, user_login),
            ''
        ) = '',
        1,
        0
    ))) AS unknown_subject_events,
    toUInt64(sum(if(
        dictGetStringOrDefault(
            'dict_domain_category',
            'category',
            domain_name,
            ''
        ) = '',
        1,
        0
    ))) AS unknown_category_events
FROM aw_browser_events
WHERE domain_name != ''
GROUP BY
    bucket_start,
    event_date,
    branch,
    department,
    activity_type,
    category,
    productivity_class;
```

### Базовый запрос Grafana

Grafana должна читать агрегированную таблицу, а не raw events:

```sql
SELECT
    event_date,
    branch,
    department,
    productivity_class,
    sum(duration_sec) AS duration_sec
FROM agg_workforce_productivity_hourly
WHERE $__timeFilter(bucket_start)
GROUP BY
    event_date,
    branch,
    department,
    productivity_class
ORDER BY
    event_date,
    branch,
    department,
    productivity_class;
```

Для детализации по desktop/browser:

```sql
SELECT
    event_date,
    department,
    activity_type,
    category,
    productivity_class,
    sum(duration_sec) AS duration_sec
FROM agg_workforce_productivity_hourly
WHERE $__timeFilter(bucket_start)
GROUP BY
    event_date,
    department,
    activity_type,
    category,
    productivity_class
ORDER BY duration_sec DESC;
```

### Backfill и пересчет

Так как materialized views фиксируют результат dictionary lookup в момент
вставки, для исправления старых данных нужен регламент:

1. Исправить source dimension table.
2. Выполнить `SYSTEM RELOAD DICTIONARY`.
3. Проверить `system.dictionaries`.
4. Пересобрать нужный период агрегатов из raw-таблиц.

Безопасный промышленный вариант для пересчета:

- создать новую агрегированную таблицу с тем же schema;
- выполнить `INSERT INTO new_agg SELECT ... FROM raw ... WHERE event_date ...`;
- сверить контрольные суммы и строки;
- атомарно переключить имя таблицы через `RENAME TABLE`.

Удаление и повторная вставка по partition допустимы только после отдельной
проверки на тестовом контуре.

### Definition of Done

Спринт 4 считается завершенным, когда:

- создана агрегированная таблица для Grafana;
- materialized view для desktop events пишет в агрегат;
- materialized view для browser events пишет в агрегат;
- dictionary enrichment выполняется при вставке;
- Grafana dashboard'ы читают агрегат, а не raw-события;
- есть запрос контроля unknown-долей по оргструктуре, приложениям и доменам;
- есть регламент backfill после исправления справочников;
- есть smoke-запрос, подтверждающий, что новые raw-события попали в агрегат;
- зафиксировано, что исправление справочника не пересчитывает исторические
  агрегаты без backfill.

### Риски спринта 4

| Риск | Митигация |
|---|---|
| Старые агрегаты не обновляются после правки dictionary | формальный backfill-регламент |
| Grafana продолжает читать raw-события | review dashboard JSON и запрет тяжелых raw-запросов |
| Неверная гранулярность агрегата | хранить hourly, дневные отчеты строить поверх hourly |
| Слишком много cardinality в ORDER BY | не включать `employee_name` в основной агрегат, держать его в drilldown |
| Ошибочная классификация попадает в агрегат | unknown counters, review справочников, backfill |
| Разные raw-схемы desktop/browser | держать отдельные MV, но общий target aggregate |

## 6. Проверка состояния dictionaries

```sql
SELECT
    name,
    status,
    element_count,
    last_exception,
    last_successful_update_time
FROM system.dictionaries
WHERE name IN (
    'dict_workstation_user',
    'dict_application_category',
    'dict_domain_category'
);
```

Ожидаемое состояние перед включением materialized views:

- все dictionaries имеют статус `LOADED`;
- `last_exception` пустой;
- `element_count` не равен нулю для загруженных справочников;
- время `last_successful_update_time` соответствует последней загрузке.

## 7. Ручная перезагрузка

После обновления справочников:

```sql
SYSTEM RELOAD DICTIONARY dict_workstation_user;
SYSTEM RELOAD DICTIONARY dict_application_category;
SYSTEM RELOAD DICTIONARY dict_domain_category;
```

После reload обязательно проверить `system.dictionaries`. Для materialized views
reload влияет только на новые вставки и ручной backfill, а не на уже записанные
агрегаты.

## 8. Контроль качества данных

Минимальный набор контрольных запросов:

```sql
SELECT
    event_date,
    sum(event_count) AS events,
    sum(unknown_subject_events) AS unknown_subject_events,
    round(unknown_subject_events / nullIf(events, 0), 4) AS unknown_subject_ratio,
    sum(unknown_category_events) AS unknown_category_events,
    round(unknown_category_events / nullIf(events, 0), 4) AS unknown_category_ratio
FROM agg_workforce_productivity_hourly
GROUP BY event_date
ORDER BY event_date DESC
LIMIT 14;
```

```sql
SELECT
    event_date,
    activity_type,
    productivity_class,
    sum(duration_sec) AS duration_sec
FROM agg_workforce_productivity_hourly
GROUP BY
    event_date,
    activity_type,
    productivity_class
ORDER BY
    event_date DESC,
    duration_sec DESC;
```

Эти проверки нужны не для дисциплинарных выводов, а для качества данных:
находить незаведенные рабочие станции, неизвестные процессы, неизвестные
домены и ошибки классификации.

## 9. Общие риски

| Риск | Митигация |
|---|---|
| Ошибочная привязка пользователя к оргструктуре | отчет unknown и ручная сверка источника |
| Ошибочная классификация приложения | поле `comment`, владелец справочника, review изменений |
| Ошибочная классификация домена | отдельный review доменного справочника и path-level backlog |
| Потеря историчности | на пилоте принимается current snapshot; для регламентных отчетов проектировать history отдельно |
| Unknown processes/domains растут | регулярный top unknown отчет |
| Dictionary stale | контроль `system.dictionaries` и reload после загрузки |
| Исправления справочников не попадают в старые агрегаты | формальный backfill |
| Использование классификации как наказательного инструмента | явно фиксировать аналитический, а не дисциплинарный характер метрик |
