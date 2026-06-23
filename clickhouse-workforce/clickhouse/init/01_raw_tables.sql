CREATE TABLE IF NOT EXISTS aw_workforce.aw_window_events
(
    event_time DateTime,
    host_name String,
    user_login String,
    process_name String,
    window_title String,
    duration_sec UInt32,
    source_bucket LowCardinality(String),
    source_event_id String,
    ingested_at DateTime DEFAULT now()
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(event_time)
ORDER BY (event_time, host_name, user_login, process_name, source_event_id);

CREATE TABLE IF NOT EXISTS aw_workforce.aw_browser_events
(
    event_time DateTime,
    host_name String,
    user_login String,
    browser_name String,
    url String,
    title String,
    duration_sec UInt32,
    source_bucket LowCardinality(String),
    source_event_id String,
    ingested_at DateTime DEFAULT now()
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(event_time)
ORDER BY (event_time, host_name, user_login, browser_name, source_event_id);
