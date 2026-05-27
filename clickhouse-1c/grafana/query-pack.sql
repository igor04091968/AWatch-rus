-- Executive: sales today
SELECT
    toDate(ts) AS d,
    sum(amount) AS sales_amount
FROM analytics_1c.documents
WHERE doc_type = 'Реализация'
GROUP BY d
ORDER BY d;

-- Executive: overdue receivables
SELECT
    organization,
    sum(amount) AS overdue_receivables
FROM analytics_1c.documents
WHERE status = 'overdue'
GROUP BY organization
ORDER BY overdue_receivables DESC;

-- Operations: host health
SELECT
    ts,
    host,
    cpu_pct,
    ram_pct,
    disk_latency_ms,
    disk_free_gb
FROM analytics_1c.host_events
ORDER BY ts DESC;

-- Audit: after-hours activity
SELECT
    ts,
    infobase,
    user,
    event_name,
    message
FROM analytics_1c.reglog_events
WHERE event_name ILIKE '%login%'
  AND toHour(ts) NOT BETWEEN 8 AND 20
ORDER BY ts DESC;

-- Detections: top rules
SELECT
    rule_title,
    severity,
    count() AS detections_total,
    sum(score) AS score_total
FROM analytics_1c.detections
GROUP BY rule_title, severity
ORDER BY detections_total DESC;

-- Investigation: entity timeline
SELECT
    ts,
    entity_type,
    entity_id,
    actor,
    source,
    event_type,
    severity,
    score,
    summary
FROM analytics_1c.entity_timeline
WHERE entity_type = {entity_type:String}
  AND entity_id = {entity_id:String}
ORDER BY ts;

-- Financial: reporting readiness
SELECT
    readiness_status,
    last_event_at,
    last_ledger_event_at,
    business_events_total,
    ledger_events_total,
    account_lines_total,
    postings_table_rows
FROM analytics_1c.v_financial_reporting_readiness;

-- Financial: daily amount split between ledger and proxy
SELECT
    toDateTime(d) AS time,
    source_kind,
    sum(amount_total) AS value
FROM analytics_1c.v_financial_daily
WHERE d >= today() - 30
GROUP BY time, source_kind
ORDER BY time, source_kind;

-- Financial: top companies by current turnover/proxy
SELECT
    infobase,
    company_name,
    readiness_status,
    ledger_amount_30d,
    proxy_amount_30d,
    risky_changes_30d,
    last_financial_day
FROM analytics_1c.v_financial_company_current
ORDER BY ledger_amount_30d DESC, proxy_amount_30d DESC
LIMIT 20;
