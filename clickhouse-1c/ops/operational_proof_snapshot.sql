-- Operational proof snapshot for DetMir/AWatch-rus.
-- Output is aggregate-only: no users, company names, hosts, domains, paths,
-- evidence content, tokens, passwords or raw incident payloads.

SELECT
    database,
    count() AS tables_total,
    sum(total_rows) AS rows_total,
    formatReadableSize(sum(total_bytes)) AS compressed_size,
    formatReadableSize(sum(total_bytes_uncompressed)) AS uncompressed_size
FROM system.tables
WHERE database = 'analytics_1c'
  AND engine NOT IN ('View')
GROUP BY database;

SELECT
    version() AS clickhouse_version,
    uptime() AS uptime_seconds,
    (SELECT count() FROM system.tables WHERE database = 'analytics_1c') AS analytics_objects,
    formatReadableSize((
        SELECT sum(total_bytes)
        FROM system.tables
        WHERE database = 'analytics_1c'
          AND engine NOT IN ('View')
    )) AS analytics_compressed_size;

SELECT
    count() AS queries_logged,
    min(event_time) AS first_query,
    max(event_time) AS last_query,
    quantile(0.5)(query_duration_ms) AS p50_ms,
    quantile(0.95)(query_duration_ms) AS p95_ms,
    max(query_duration_ms) AS max_ms
FROM system.query_log
WHERE event_time >= now() - INTERVAL 7 DAY
  AND type = 'QueryFinish'
  AND current_database = 'analytics_1c';

SELECT 'business_events' AS table_name, count() AS rows, min(ts) AS min_ts, max(ts) AS max_ts
FROM analytics_1c.business_events
UNION ALL
SELECT 'document_change_events', count(), min(ts), max(ts)
FROM analytics_1c.document_change_events
UNION ALL
SELECT 'detections', count(), min(ts), max(ts)
FROM analytics_1c.detections
UNION ALL
SELECT 'cases', count(), min(opened_at), max(opened_at)
FROM analytics_1c.cases
UNION ALL
SELECT 'company_health_signals', count(), min(generated_at), max(generated_at)
FROM analytics_1c.company_health_signals
UNION ALL
SELECT 'company_forecasts', count(), min(generated_at), max(generated_at)
FROM analytics_1c.company_forecasts
ORDER BY table_name;

SELECT
    countDistinct(infobase) AS infobases,
    countDistinct(user) AS users_in_reglog,
    countDistinct(host) AS hosts_in_reglog,
    count() AS reglog_events,
    min(ts) AS min_ts,
    max(ts) AS max_ts
FROM analytics_1c.reglog_events;

SELECT
    countDistinct(infobase) AS infobases,
    countDistinct(company_entity_key) AS company_entities,
    countDistinct(document_id) AS documents,
    countDistinct(user) AS users,
    count() AS business_events,
    min(ts) AS min_ts,
    max(ts) AS max_ts
FROM analytics_1c.business_events;

SELECT
    severity,
    status,
    count() AS detections
FROM analytics_1c.detections
GROUP BY severity, status
ORDER BY detections DESC;

SELECT
    severity,
    status,
    count() AS cases
FROM analytics_1c.cases
GROUP BY severity, status
ORDER BY cases DESC;

SELECT
    countDistinct(company_key) AS registry_companies,
    countIf(key_contour = 1) AS key_contour_rows,
    countDistinct(assignee_name) AS assignees,
    max(ts) AS last_snapshot
FROM analytics_1c.company_registry;
