CREATE TABLE IF NOT EXISTS analytics_1c.company_forecasts
(
    generated_at DateTime,
    as_of_date Date,
    infobase LowCardinality(String),
    counterparty String,
    horizon_days UInt16,
    metric LowCardinality(String),
    baseline_daily Float64,
    trend_slope Float64,
    predicted_daily Float64,
    predicted_total Float64,
    confidence Float32,
    model LowCardinality(String),
    source_days UInt16,
    note String
)
ENGINE = MergeTree
ORDER BY (generated_at, infobase, counterparty, metric, horizon_days);

CREATE TABLE IF NOT EXISTS analytics_1c.company_health_signals
(
    generated_at DateTime,
    infobase LowCardinality(String),
    counterparty String,
    signal_id String,
    severity LowCardinality(String),
    score UInt32,
    signal_type LowCardinality(String),
    summary String,
    amount_7d Float64,
    amount_prev_7d Float64,
    docs_7d UInt32,
    docs_prev_7d UInt32,
    days_since_last_activity UInt16,
    open_cases_total UInt32,
    detections_total UInt32
)
ENGINE = MergeTree
ORDER BY (generated_at, severity, infobase, counterparty, signal_id);

CREATE OR REPLACE VIEW analytics_1c.v_counterparty_daily AS
SELECT
    toDate(ts) AS d,
    infobase,
    organization,
    counterparty,
    count() AS docs_total,
    sum(amount) AS amount_total,
    countIf(posted = 1) AS posted_docs_total,
    countIf(posted = 0) AS unposted_docs_total,
    countIf(status = 'busy') AS busy_docs_total,
    countIf(status = 'online') AS online_docs_total,
    uniqExact(doc_type) AS doc_types_total
FROM analytics_1c.documents
WHERE counterparty != ''
GROUP BY d, infobase, organization, counterparty;

CREATE OR REPLACE VIEW analytics_1c.v_counterparty_latest_activity AS
SELECT
    infobase,
    organization,
    counterparty,
    max(ts) AS last_seen_at,
    argMax(doc_type, ts) AS last_doc_type,
    argMax(operation_type, ts) AS last_operation_type,
    argMax(status, ts) AS last_status,
    argMax(amount, ts) AS last_amount,
    count() AS docs_lifetime,
    sum(amount) AS amount_lifetime
FROM analytics_1c.documents
WHERE counterparty != ''
GROUP BY infobase, organization, counterparty;

CREATE OR REPLACE VIEW analytics_1c.v_company_forecasts_current AS
SELECT *
FROM analytics_1c.company_forecasts
WHERE generated_at = (SELECT max(generated_at) FROM analytics_1c.company_forecasts);

CREATE OR REPLACE VIEW analytics_1c.v_company_health_current AS
SELECT *
FROM analytics_1c.company_health_signals
WHERE generated_at = (SELECT max(generated_at) FROM analytics_1c.company_health_signals);

CREATE OR REPLACE VIEW analytics_1c.v_companies_current AS
SELECT
    infobase,
    argMax(company_name, ts) AS company_name,
    argMax(organization, ts) AS organization,
    argMax(owner_user, ts) AS owner_user,
    argMax(base_id, ts) AS base_id,
    argMax(base_path, ts) AS base_path,
    argMax(status, ts) AS current_status,
    argMax(db_size_bytes, ts) AS db_size_bytes,
    argMax(reglog_size_bytes, ts) AS reglog_size_bytes,
    argMax(active_locks, ts) AS active_locks,
    argMax(temp_db_present, ts) AS temp_db_present,
    argMax(scheduler_touched, ts) AS scheduler_touched,
    argMax(activity_score, ts) AS current_activity_score,
    max(ts) AS last_company_snapshot_at
FROM analytics_1c.companies
GROUP BY infobase;

CREATE OR REPLACE VIEW analytics_1c.v_company_portfolio_overview AS
WITH
base AS
(
    SELECT *
    FROM analytics_1c.v_counterparty_latest_activity
),
d7 AS
(
    SELECT
        infobase,
        counterparty,
        sum(docs_total) AS docs_7d,
        sum(amount_total) AS amount_7d
    FROM analytics_1c.v_counterparty_daily
    WHERE d >= today() - 7
    GROUP BY infobase, counterparty
),
d30 AS
(
    SELECT
        infobase,
        counterparty,
        countDistinct(d) AS active_days_30d,
        sum(docs_total) AS docs_30d,
        sum(amount_total) AS amount_30d,
        sum(busy_docs_total) AS busy_docs_30d
    FROM analytics_1c.v_counterparty_daily
    WHERE d >= today() - 30
    GROUP BY infobase, counterparty
),
company_state AS
(
    SELECT *
    FROM analytics_1c.v_companies_current
),
signals AS
(
    SELECT
        infobase,
        counterparty,
        max(score) AS signal_score,
        argMax(severity, score) AS signal_severity,
        argMax(summary, score) AS top_signal
    FROM analytics_1c.v_company_health_current
    GROUP BY infobase, counterparty
),
amount_forecast AS
(
    SELECT
        infobase,
        counterparty,
        predicted_total AS amount_forecast_30d,
        confidence AS amount_forecast_confidence
    FROM analytics_1c.v_company_forecasts_current
    WHERE metric = 'amount_total'
      AND horizon_days = 30
),
docs_forecast AS
(
    SELECT
        infobase,
        counterparty,
        predicted_total AS docs_forecast_30d,
        confidence AS docs_forecast_confidence
    FROM analytics_1c.v_company_forecasts_current
    WHERE metric = 'docs_total'
      AND horizon_days = 30
),
cases_current AS
(
    SELECT
        infobase,
        entity_id AS counterparty,
        countIf(status != 'closed') AS open_cases_total
    FROM analytics_1c.cases
    WHERE entity_type = 'counterparty'
    GROUP BY infobase, counterparty
),
detections_current AS
(
    SELECT
        infobase,
        entity_id AS counterparty,
        count() AS detections_total
    FROM analytics_1c.detections
    WHERE entity_type = 'counterparty'
      AND status != 'closed'
    GROUP BY infobase, counterparty
)
SELECT
    base.infobase AS infobase,
    if(company_state.organization != '', company_state.organization, base.organization) AS organization,
    base.counterparty AS counterparty,
    if(company_state.company_name != '', company_state.company_name, base.counterparty) AS company_name,
    ifNull(company_state.owner_user, '') AS owner_user,
    ifNull(company_state.base_id, '') AS base_id,
    ifNull(company_state.base_path, '') AS base_path,
    base.last_seen_at,
    company_state.last_company_snapshot_at,
    base.last_doc_type,
    base.last_operation_type,
    base.last_status,
    ifNull(company_state.current_status, base.last_status) AS current_status,
    ifNull(company_state.db_size_bytes, 0) AS db_size_bytes,
    ifNull(company_state.reglog_size_bytes, 0) AS reglog_size_bytes,
    ifNull(company_state.active_locks, 0) AS active_locks,
    ifNull(company_state.temp_db_present, 0) AS temp_db_present,
    ifNull(company_state.scheduler_touched, 0) AS scheduler_touched,
    ifNull(company_state.current_activity_score, 0) AS current_activity_score,
    dateDiff('day', toDate(base.last_seen_at), today()) AS days_since_last_activity,
    ifNull(d7.docs_7d, 0) AS docs_7d,
    ifNull(d7.amount_7d, 0) AS amount_7d,
    ifNull(d30.active_days_30d, 0) AS active_days_30d,
    ifNull(d30.docs_30d, 0) AS docs_30d,
    ifNull(d30.amount_30d, 0) AS amount_30d,
    ifNull(d30.busy_docs_30d, 0) AS busy_docs_30d,
    ifNull(amount_forecast.amount_forecast_30d, 0) AS amount_forecast_30d,
    ifNull(amount_forecast.amount_forecast_confidence, 0) AS amount_forecast_confidence,
    ifNull(docs_forecast.docs_forecast_30d, 0) AS docs_forecast_30d,
    ifNull(docs_forecast.docs_forecast_confidence, 0) AS docs_forecast_confidence,
    ifNull(cases_current.open_cases_total, 0) AS open_cases_total,
    ifNull(detections_current.detections_total, 0) AS detections_total,
    ifNull(signals.signal_severity, 'none') AS signal_severity,
    ifNull(signals.signal_score, 0) AS signal_score,
    ifNull(signals.top_signal, '') AS top_signal
FROM base
LEFT JOIN company_state ON company_state.infobase = base.infobase
LEFT JOIN d7 ON d7.infobase = base.infobase AND d7.counterparty = base.counterparty
LEFT JOIN d30 ON d30.infobase = base.infobase AND d30.counterparty = base.counterparty
LEFT JOIN signals ON signals.infobase = base.infobase AND signals.counterparty = base.counterparty
LEFT JOIN amount_forecast ON amount_forecast.infobase = base.infobase AND amount_forecast.counterparty = base.counterparty
LEFT JOIN docs_forecast ON docs_forecast.infobase = base.infobase AND docs_forecast.counterparty = base.counterparty
LEFT JOIN cases_current ON cases_current.infobase = base.infobase AND cases_current.counterparty = base.counterparty
LEFT JOIN detections_current ON detections_current.infobase = base.infobase AND detections_current.counterparty = base.counterparty;
