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

CREATE TABLE IF NOT EXISTS analytics_1c.raw_1c_business_events
(
    ingested_at DateTime DEFAULT now(),
    source_file String,
    payload String
)
ENGINE = MergeTree
ORDER BY (ingested_at, source_file);

CREATE TABLE IF NOT EXISTS analytics_1c.raw_1c_document_changes
(
    ingested_at DateTime DEFAULT now(),
    source_file String,
    payload String
)
ENGINE = MergeTree
ORDER BY (ingested_at, source_file);

CREATE TABLE IF NOT EXISTS analytics_1c.raw_1c_companies
(
    ingested_at DateTime DEFAULT now(),
    source_file String,
    payload String
)
ENGINE = MergeTree
ORDER BY (ingested_at, source_file);

CREATE TABLE IF NOT EXISTS analytics_1c.raw_1c_company_registry
(
    ingested_at DateTime DEFAULT now(),
    source_file String,
    source_sheet String,
    payload String
)
ENGINE = MergeTree
ORDER BY (ingested_at, source_file, source_sheet);

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
