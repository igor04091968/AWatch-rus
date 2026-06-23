CREATE TABLE IF NOT EXISTS aw_workforce.agg_workforce_productivity_hourly
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

DROP VIEW IF EXISTS aw_workforce.mv_desktop_productivity_hourly;

CREATE MATERIALIZED VIEW aw_workforce.mv_desktop_productivity_hourly
TO aw_workforce.agg_workforce_productivity_hourly
AS
SELECT
    toStartOfHour(event_time) AS bucket_start,
    toDate(event_time) AS event_date,

    if(
        dictGetUInt8OrDefault('aw_workforce.dict_workstation_user', 'is_active', (host_name, user_login), 0) = 1
        AND dictGetStringOrDefault('aw_workforce.dict_workstation_user', 'branch', (host_name, user_login), '') != '',
        dictGetStringOrDefault('aw_workforce.dict_workstation_user', 'branch', (host_name, user_login), 'unknown'),
        'unknown'
    ) AS branch,
    if(
        dictGetUInt8OrDefault('aw_workforce.dict_workstation_user', 'is_active', (host_name, user_login), 0) = 1
        AND dictGetStringOrDefault('aw_workforce.dict_workstation_user', 'department', (host_name, user_login), '') != '',
        dictGetStringOrDefault('aw_workforce.dict_workstation_user', 'department', (host_name, user_login), 'unknown'),
        'unknown'
    ) AS department,

    'desktop' AS activity_type,
    if(
        dictGetUInt8OrDefault('aw_workforce.dict_application_category', 'is_active', process_name, 0) = 1
        AND dictGetStringOrDefault('aw_workforce.dict_application_category', 'category', process_name, '') != '',
        dictGetStringOrDefault('aw_workforce.dict_application_category', 'category', process_name, 'unknown'),
        'unknown'
    ) AS category,
    if(
        dictGetUInt8OrDefault('aw_workforce.dict_application_category', 'is_active', process_name, 0) = 1
        AND dictGetStringOrDefault('aw_workforce.dict_application_category', 'productivity_class', process_name, '') != '',
        dictGetStringOrDefault('aw_workforce.dict_application_category', 'productivity_class', process_name, 'unknown'),
        'unknown'
    ) AS productivity_class,

    toUInt64(sum(duration_sec)) AS duration_sec,
    toUInt64(count()) AS event_count,
    toUInt64(sum(if(
        dictGetUInt8OrDefault('aw_workforce.dict_workstation_user', 'is_active', (host_name, user_login), 0) = 1
        AND dictGetStringOrDefault('aw_workforce.dict_workstation_user', 'employee_name', (host_name, user_login), '') != '',
        0,
        1
    ))) AS unknown_subject_events,
    toUInt64(sum(if(
        dictGetUInt8OrDefault('aw_workforce.dict_application_category', 'is_active', process_name, 0) = 1
        AND dictGetStringOrDefault('aw_workforce.dict_application_category', 'category', process_name, '') NOT IN ('', 'unknown'),
        0,
        1
    ))) AS unknown_category_events
FROM aw_workforce.aw_window_events
GROUP BY
    bucket_start,
    event_date,
    branch,
    department,
    activity_type,
    category,
    productivity_class;

DROP VIEW IF EXISTS aw_workforce.mv_browser_productivity_hourly;

CREATE MATERIALIZED VIEW aw_workforce.mv_browser_productivity_hourly
TO aw_workforce.agg_workforce_productivity_hourly
AS
WITH
    lowerUTF8(
        domain(if(position(url, '://') = 0, concat('http://', url), url))
    ) AS domain_name
SELECT
    toStartOfHour(event_time) AS bucket_start,
    toDate(event_time) AS event_date,

    if(
        dictGetUInt8OrDefault('aw_workforce.dict_workstation_user', 'is_active', (host_name, user_login), 0) = 1
        AND dictGetStringOrDefault('aw_workforce.dict_workstation_user', 'branch', (host_name, user_login), '') != '',
        dictGetStringOrDefault('aw_workforce.dict_workstation_user', 'branch', (host_name, user_login), 'unknown'),
        'unknown'
    ) AS branch,
    if(
        dictGetUInt8OrDefault('aw_workforce.dict_workstation_user', 'is_active', (host_name, user_login), 0) = 1
        AND dictGetStringOrDefault('aw_workforce.dict_workstation_user', 'department', (host_name, user_login), '') != '',
        dictGetStringOrDefault('aw_workforce.dict_workstation_user', 'department', (host_name, user_login), 'unknown'),
        'unknown'
    ) AS department,

    'browser' AS activity_type,
    if(
        dictGetUInt8OrDefault('aw_workforce.dict_domain_category', 'is_active', domain_name, 0) = 1
        AND dictGetStringOrDefault('aw_workforce.dict_domain_category', 'category', domain_name, '') != '',
        dictGetStringOrDefault('aw_workforce.dict_domain_category', 'category', domain_name, 'unknown'),
        'unknown'
    ) AS category,
    if(
        dictGetUInt8OrDefault('aw_workforce.dict_domain_category', 'is_active', domain_name, 0) = 1
        AND dictGetStringOrDefault('aw_workforce.dict_domain_category', 'productivity_class', domain_name, '') != '',
        dictGetStringOrDefault('aw_workforce.dict_domain_category', 'productivity_class', domain_name, 'unknown'),
        'unknown'
    ) AS productivity_class,

    toUInt64(sum(duration_sec)) AS duration_sec,
    toUInt64(count()) AS event_count,
    toUInt64(sum(if(
        dictGetUInt8OrDefault('aw_workforce.dict_workstation_user', 'is_active', (host_name, user_login), 0) = 1
        AND dictGetStringOrDefault('aw_workforce.dict_workstation_user', 'employee_name', (host_name, user_login), '') != '',
        0,
        1
    ))) AS unknown_subject_events,
    toUInt64(sum(if(
        dictGetUInt8OrDefault('aw_workforce.dict_domain_category', 'is_active', domain_name, 0) = 1
        AND dictGetStringOrDefault('aw_workforce.dict_domain_category', 'category', domain_name, '') NOT IN ('', 'unknown'),
        0,
        1
    ))) AS unknown_category_events
FROM aw_workforce.aw_browser_events
WHERE domain_name != ''
GROUP BY
    bucket_start,
    event_date,
    branch,
    department,
    activity_type,
    category,
    productivity_class;
