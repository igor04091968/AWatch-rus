CREATE TABLE IF NOT EXISTS analytics_1c.raw_1c_documents
(
    ingested_at DateTime DEFAULT now(),
    source_file String,
    payload String
)
ENGINE = MergeTree
ORDER BY (ingested_at, source_file);

CREATE TABLE IF NOT EXISTS analytics_1c.raw_1c_postings
(
    ingested_at DateTime DEFAULT now(),
    source_file String,
    payload String
)
ENGINE = MergeTree
ORDER BY (ingested_at, source_file);

CREATE TABLE IF NOT EXISTS analytics_1c.raw_reglog
(
    ingested_at DateTime DEFAULT now(),
    source_file String,
    payload String
)
ENGINE = MergeTree
ORDER BY (ingested_at, source_file);

CREATE TABLE IF NOT EXISTS analytics_1c.raw_audit
(
    ingested_at DateTime DEFAULT now(),
    source_file String,
    payload String
)
ENGINE = MergeTree
ORDER BY (ingested_at, source_file);

CREATE TABLE IF NOT EXISTS analytics_1c.raw_host_metrics
(
    ingested_at DateTime DEFAULT now(),
    source_file String,
    payload String
)
ENGINE = MergeTree
ORDER BY (ingested_at, source_file);
