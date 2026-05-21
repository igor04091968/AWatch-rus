CREATE TABLE IF NOT EXISTS analytics_1c.documents
(
    ts DateTime,
    infobase LowCardinality(String),
    organization String,
    department String,
    doc_type LowCardinality(String),
    doc_id String,
    doc_number String,
    author String,
    counterparty String,
    operation_type String,
    amount Decimal(18, 2),
    status LowCardinality(String),
    posted UInt8,
    source_file String
)
ENGINE = MergeTree
ORDER BY (infobase, ts, doc_type, doc_id);

CREATE TABLE IF NOT EXISTS analytics_1c.postings
(
    ts DateTime,
    infobase LowCardinality(String),
    registrar String,
    operation_type String,
    account_dt String,
    account_ct String,
    amount Decimal(18, 2),
    source_file String
)
ENGINE = MergeTree
ORDER BY (infobase, ts, registrar);

CREATE TABLE IF NOT EXISTS analytics_1c.reglog_events
(
    ts DateTime,
    infobase LowCardinality(String),
    user String,
    host String,
    app String,
    event_name String,
    level LowCardinality(String),
    duration_ms UInt32,
    message String,
    source_file String
)
ENGINE = MergeTree
ORDER BY (infobase, ts, user, event_name);

CREATE TABLE IF NOT EXISTS analytics_1c.audit_events
(
    ts DateTime,
    infobase LowCardinality(String),
    user String,
    object_type String,
    object_id String,
    action String,
    before_hash String,
    after_hash String,
    risk_tag String,
    source_file String
)
ENGINE = MergeTree
ORDER BY (infobase, ts, object_type, object_id);

CREATE TABLE IF NOT EXISTS analytics_1c.host_events
(
    ts DateTime,
    host String,
    cpu_pct Float32,
    ram_pct Float32,
    disk_free_gb Float32,
    disk_latency_ms Float32,
    smb_errors UInt32,
    rdp_sessions UInt32,
    backup_ok UInt8,
    source_file String
)
ENGINE = MergeTree
ORDER BY (host, ts);

CREATE TABLE IF NOT EXISTS analytics_1c.entity_timeline
(
    ts DateTime,
    entity_type LowCardinality(String),
    entity_id String,
    infobase LowCardinality(String),
    actor String,
    source LowCardinality(String),
    event_type String,
    severity LowCardinality(String),
    score UInt32,
    ref_id String,
    summary String
)
ENGINE = MergeTree
ORDER BY (entity_type, entity_id, ts);

CREATE TABLE IF NOT EXISTS analytics_1c.detections
(
    ts DateTime,
    detection_id String,
    infobase LowCardinality(String),
    rule_id String,
    rule_title String,
    entity_type LowCardinality(String),
    entity_id String,
    severity LowCardinality(String),
    score UInt32,
    summary String,
    status LowCardinality(String) DEFAULT 'open'
)
ENGINE = MergeTree
ORDER BY (severity, ts, rule_id, entity_type, entity_id);

CREATE TABLE IF NOT EXISTS analytics_1c.cases
(
    opened_at DateTime,
    case_id String,
    infobase LowCardinality(String),
    title String,
    severity LowCardinality(String),
    status LowCardinality(String),
    assignee String,
    detection_id String,
    entity_type String,
    entity_id String,
    summary String
)
ENGINE = MergeTree
ORDER BY (opened_at, case_id);
