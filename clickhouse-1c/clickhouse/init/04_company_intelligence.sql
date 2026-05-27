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

CREATE TABLE IF NOT EXISTS analytics_1c.company_registry_bindings
(
    ts DateTime,
    infobase LowCardinality(String),
    company_entity_key String,
    base_id String,
    base_path String,
    base_path_key String,
    registry_company_key String,
    registry_company_name String,
    binding_source LowCardinality(String),
    note String
)
ENGINE = MergeTree
ORDER BY (company_entity_key, ts);

CREATE OR REPLACE VIEW analytics_1c.v_companies_current AS
SELECT
    infobase,
    company_name,
    organization,
    owner_user,
    base_id,
    base_path,
    trimBoth(replaceRegexpAll(replaceRegexpAll(replaceRegexpAll(upperUTF8(base_path), '[\\\\/]+', '/'), '[^0-9A-ZА-ЯЁ:/._ -]+', ' '), '\\s+', ' ')) AS base_path_key,
    multiIf(
        base_id != '', concat('baseid:', base_id),
        base_path != '', concat('basepath:', trimBoth(replaceRegexpAll(replaceRegexpAll(replaceRegexpAll(upperUTF8(base_path), '[\\\\/]+', '/'), '[^0-9A-ZА-ЯЁ:/._ -]+', ' '), '\\s+', ' '))),
        concat('infobase:', trimBoth(replaceRegexpAll(replaceRegexpAll(replaceRegexpAll(upperUTF8(infobase), '(^|\\s)20[0-9]{2}($|\\s)', ' '), '[^0-9A-ZА-ЯЁ]+', ' '), '\\s+', ' ')))
    ) AS company_entity_key,
    current_status,
    db_size_bytes,
    reglog_size_bytes,
    active_locks,
    temp_db_present,
    scheduler_touched,
    current_activity_score,
    last_company_snapshot_at
FROM
(
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
    GROUP BY infobase
);

CREATE OR REPLACE VIEW analytics_1c.v_company_activity_daily AS
SELECT
    toDate(documents.ts) AS d,
    documents.infobase AS infobase,
    ifNull(companies.organization, documents.organization) AS organization,
    ifNull(companies.company_entity_key, concat('infobase:', trimBoth(replaceRegexpAll(replaceRegexpAll(replaceRegexpAll(upperUTF8(documents.infobase), '(^|\\s)20[0-9]{2}($|\\s)', ' '), '[^0-9A-ZА-ЯЁ]+', ' '), '\\s+', ' ')))) AS company_entity_key,
    argMax(documents.counterparty, documents.ts) AS source_counterparty,
    count() AS docs_total,
    sum(documents.amount) AS amount_total,
    countIf(documents.posted = 1) AS posted_docs_total,
    countIf(documents.posted = 0) AS unposted_docs_total,
    countIf(documents.status = 'busy') AS busy_docs_total,
    countIf(documents.status = 'online') AS online_docs_total,
    uniqExact(documents.doc_type) AS doc_types_total
FROM analytics_1c.documents AS documents
LEFT JOIN analytics_1c.v_companies_current AS companies ON companies.infobase = documents.infobase
WHERE documents.counterparty != ''
GROUP BY d, documents.infobase, ifNull(companies.organization, documents.organization), ifNull(companies.company_entity_key, concat('infobase:', trimBoth(replaceRegexpAll(replaceRegexpAll(replaceRegexpAll(upperUTF8(documents.infobase), '(^|\\s)20[0-9]{2}($|\\s)', ' '), '[^0-9A-ZА-ЯЁ]+', ' '), '\\s+', ' '))));

CREATE OR REPLACE VIEW analytics_1c.v_company_activity_latest AS
SELECT
    documents.infobase AS infobase,
    ifNull(companies.organization, documents.organization) AS organization,
    ifNull(companies.company_entity_key, concat('infobase:', trimBoth(replaceRegexpAll(replaceRegexpAll(replaceRegexpAll(upperUTF8(documents.infobase), '(^|\\s)20[0-9]{2}($|\\s)', ' '), '[^0-9A-ZА-ЯЁ]+', ' '), '\\s+', ' ')))) AS company_entity_key,
    argMax(documents.counterparty, documents.ts) AS source_counterparty,
    max(documents.ts) AS last_seen_at,
    argMax(documents.doc_type, documents.ts) AS last_doc_type,
    argMax(documents.operation_type, documents.ts) AS last_operation_type,
    argMax(documents.status, documents.ts) AS last_status,
    argMax(documents.amount, documents.ts) AS last_amount,
    count() AS docs_lifetime,
    sum(documents.amount) AS amount_lifetime
FROM analytics_1c.documents AS documents
LEFT JOIN analytics_1c.v_companies_current AS companies ON companies.infobase = documents.infobase
WHERE documents.counterparty != ''
GROUP BY documents.infobase, ifNull(companies.organization, documents.organization), ifNull(companies.company_entity_key, concat('infobase:', trimBoth(replaceRegexpAll(replaceRegexpAll(replaceRegexpAll(upperUTF8(documents.infobase), '(^|\\s)20[0-9]{2}($|\\s)', ' '), '[^0-9A-ZА-ЯЁ]+', ' '), '\\s+', ' '))));

CREATE OR REPLACE VIEW analytics_1c.v_company_forecasts_current AS
SELECT *
FROM analytics_1c.company_forecasts
WHERE generated_at = (SELECT max(generated_at) FROM analytics_1c.company_forecasts);

CREATE OR REPLACE VIEW analytics_1c.v_company_health_current AS
SELECT *
FROM analytics_1c.company_health_signals
WHERE generated_at = (SELECT max(generated_at) FROM analytics_1c.company_health_signals);

CREATE OR REPLACE VIEW analytics_1c.v_counterparty_daily AS
SELECT
    d,
    infobase,
    organization,
    company_entity_key AS counterparty,
    docs_total,
    amount_total,
    posted_docs_total,
    unposted_docs_total,
    busy_docs_total,
    online_docs_total,
    doc_types_total
FROM analytics_1c.v_company_activity_daily;

CREATE OR REPLACE VIEW analytics_1c.v_counterparty_latest_activity AS
SELECT
    infobase,
    organization,
    company_entity_key AS counterparty,
    source_counterparty,
    last_seen_at,
    last_doc_type,
    last_operation_type,
    last_status,
    last_amount,
    docs_lifetime,
    amount_lifetime
FROM analytics_1c.v_company_activity_latest;

CREATE OR REPLACE VIEW analytics_1c.v_company_registry_current AS
SELECT
    company_key,
    argMax(company_name, ts) AS company_name,
    argMax(assignee_name, ts) AS assignee_name,
    argMax(registry_status, ts) AS registry_status,
    argMax(share_text, ts) AS share_text,
    argMax(key_contour, ts) AS key_contour,
    argMax(inn, ts) AS inn,
    argMax(kpp, ts) AS kpp,
    max(ts) AS last_registry_snapshot_at
FROM analytics_1c.company_registry
GROUP BY company_key;

CREATE OR REPLACE VIEW analytics_1c.v_company_registry_bindings_current AS
SELECT
    company_entity_key,
    argMax(infobase, ts) AS infobase,
    argMax(base_id, ts) AS base_id,
    argMax(base_path, ts) AS base_path,
    argMax(base_path_key, ts) AS base_path_key,
    argMax(registry_company_key, ts) AS registry_company_key,
    argMax(registry_company_name, ts) AS registry_company_name,
    argMax(binding_source, ts) AS binding_source,
    argMax(note, ts) AS note,
    max(ts) AS last_binding_at
FROM analytics_1c.company_registry_bindings
GROUP BY company_entity_key;

CREATE OR REPLACE VIEW analytics_1c.v_company_registry_alias_map AS
SELECT
    source_company_key,
    target_company_key,
    target_company_name,
    exclude_from_portfolio,
    note
FROM
(
    SELECT
        'ИНФОРМАЦИОННАЯ БАЗА' AS source_company_key,
        '' AS target_company_key,
        'Информационная база' AS target_company_name,
        toUInt8(1) AS exclude_from_portfolio,
        'system_filebase' AS note
    UNION ALL
    SELECT
        'МОСКОТЕЛЬИНКОВ АЛЕНСАНДР',
        'МОСКОТЕЛЬНИКОВ АЛ В',
        'МОСКОТЕЛЬНИКОВ АЛ.В',
        toUInt8(0),
        'manual_alias_typo'
    UNION ALL
    SELECT
        'МОСКОТЕЛЬНИКОВ АЛЕКСАНДР',
        'МОСКОТЕЛЬНИКОВ АЛ В',
        'МОСКОТЕЛЬНИКОВ АЛ.В',
        toUInt8(0),
        'manual_alias_expansion'
    UNION ALL
    SELECT
        'СЕРДИТОВ АНДРЕЙ',
        'СЕРДИТОВ АВ',
        'СЕРДИТОВ АВ',
        toUInt8(0),
        'manual_alias_initials'
    UNION ALL
    SELECT
        'СЕРДИТОВ СЕРГЕЙ',
        'СЕРДИТОВ СВ',
        'СЕРДИТОВ СВ',
        toUInt8(0),
        'manual_alias_initials'
    UNION ALL
    SELECT
        'СОРФИН',
        'СОЛФИН',
        'СОЛФИН',
        toUInt8(0),
        'manual_alias_typo'
);

CREATE OR REPLACE VIEW analytics_1c.v_company_registry_manual_overrides AS
SELECT
    company_key,
    company_name,
    assignee_name,
    registry_status,
    share_text,
    key_contour,
    inn,
    kpp,
    note
FROM
(
    SELECT
        'МАВЕРИК' AS company_key,
        'МАВЕРИК' AS company_name,
        '' AS assignee_name,
        'manual' AS registry_status,
        '' AS share_text,
        toUInt8(0) AS key_contour,
        '' AS inn,
        '' AS kpp,
        'workbook:list-sheet' AS note
    UNION ALL
    SELECT
        'ТСН КОММУНИСТИЧЕСКАЯ 4',
        'ТСН КОММУНИСТИЧЕСКАЯ 4',
        '',
        'manual',
        '',
        toUInt8(0),
        '',
        '',
        'manual:portfolio-carry'
    UNION ALL
    SELECT
        'ФЕЛИЦТ ГРУПП',
        'ФЕЛИЦТ ГРУПП',
        '',
        'manual',
        '',
        toUInt8(0),
        '',
        '',
        'manual:portfolio-carry'
);

CREATE OR REPLACE VIEW analytics_1c.v_company_portfolio_overview AS
WITH
base AS
(
    SELECT
        *,
        trimBoth(replaceRegexpAll(replaceRegexpAll(replaceRegexpAll(upperUTF8(source_counterparty), '(^|\\s)20[0-9]{2}($|\\s)', ' '), '[^0-9A-ZА-ЯЁ]+', ' '), '\\s+', ' ')) AS source_counterparty_key
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
binding_state AS
(
    SELECT *
    FROM analytics_1c.v_company_registry_bindings_current
),
registry_state AS
(
    SELECT *
    FROM analytics_1c.v_company_registry_current
),
alias_state AS
(
    SELECT *
    FROM analytics_1c.v_company_registry_alias_map
),
manual_state AS
(
    SELECT *
    FROM analytics_1c.v_company_registry_manual_overrides
),
signals AS
(
    SELECT
        infobase,
        counterparty AS company_entity_key,
        max(score) AS signal_score,
        argMax(severity, score) AS signal_severity,
        argMax(summary, score) AS top_signal
    FROM analytics_1c.v_company_health_current
    GROUP BY infobase, company_entity_key
),
amount_forecast AS
(
    SELECT
        infobase,
        counterparty AS company_entity_key,
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
        counterparty AS company_entity_key,
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
        entity_id AS company_entity_key,
        countIf(status != 'closed') AS open_cases_total
    FROM analytics_1c.cases
    WHERE entity_type = 'counterparty'
    GROUP BY infobase, company_entity_key
),
detections_current AS
(
    SELECT
        infobase,
        entity_id AS company_entity_key,
        count() AS detections_total
    FROM analytics_1c.detections
    WHERE entity_type = 'counterparty'
      AND status != 'closed'
    GROUP BY infobase, company_entity_key
)
SELECT
    base.infobase AS infobase,
    base.counterparty AS company_entity_key,
    if(company_state.organization != '', company_state.organization, base.organization) AS organization,
    base.source_counterparty AS source_counterparty,
    if(binding_state.registry_company_name != '', binding_state.registry_company_name, if(alias_state.target_company_name != '', alias_state.target_company_name, if(manual_state.company_name != '', manual_state.company_name, if(company_state.company_name != '', company_state.company_name, base.source_counterparty)))) AS counterparty,
    if(binding_state.registry_company_name != '', binding_state.registry_company_name, if(alias_state.target_company_name != '', alias_state.target_company_name, if(manual_state.company_name != '', manual_state.company_name, if(company_state.company_name != '', company_state.company_name, base.source_counterparty)))) AS company_name,
    if(binding_state.registry_company_name != '', binding_state.registry_company_name, if(alias_state.target_company_name != '', alias_state.target_company_name, if(manual_state.company_name != '', manual_state.company_name, base.source_counterparty))) AS normalized_counterparty,
    multiIf(binding_state.registry_company_key != '', 'technical', ifNull(alias_state.exclude_from_portfolio, 0) = 1, 'excluded', alias_state.target_company_key != '' AND registry_state.company_key != '', 'alias', registry_state.company_key != '', 'direct', manual_state.company_key != '', 'manual', 'none') AS registry_match_mode,
    if(binding_state.registry_company_key != '', binding_state.registry_company_key, if(registry_state.company_key != '', registry_state.company_key, ifNull(manual_state.company_key, ''))) AS registry_company_key,
    ifNull(binding_state.binding_source, '') AS registry_binding_source,
    ifNull(binding_state.note, '') AS registry_binding_note,
    if(registry_state.assignee_name != '', registry_state.assignee_name, ifNull(manual_state.assignee_name, '')) AS registry_assignee_name,
    if(registry_state.registry_status != '', registry_state.registry_status, ifNull(manual_state.registry_status, '')) AS registry_status,
    if(registry_state.share_text != '', registry_state.share_text, ifNull(manual_state.share_text, '')) AS registry_share_text,
    if(registry_state.key_contour != 0, registry_state.key_contour, ifNull(manual_state.key_contour, 0)) AS registry_key_contour,
    if(registry_state.inn != '', registry_state.inn, ifNull(manual_state.inn, '')) AS registry_inn,
    if(registry_state.kpp != '', registry_state.kpp, ifNull(manual_state.kpp, '')) AS registry_kpp,
    ifNull(company_state.owner_user, '') AS owner_user,
    ifNull(company_state.base_id, '') AS base_id,
    ifNull(company_state.base_path, '') AS base_path,
    ifNull(company_state.base_path_key, '') AS base_path_key,
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
LEFT JOIN binding_state ON binding_state.company_entity_key = base.counterparty
LEFT JOIN alias_state ON alias_state.source_company_key = base.source_counterparty_key
LEFT JOIN registry_state ON registry_state.company_key = if(binding_state.registry_company_key != '', binding_state.registry_company_key, if(alias_state.target_company_key != '', alias_state.target_company_key, base.source_counterparty_key))
LEFT JOIN manual_state ON manual_state.company_key = if(binding_state.registry_company_key != '', binding_state.registry_company_key, if(alias_state.target_company_key != '', alias_state.target_company_key, base.source_counterparty_key))
LEFT JOIN d7 ON d7.infobase = base.infobase AND d7.counterparty = base.counterparty
LEFT JOIN d30 ON d30.infobase = base.infobase AND d30.counterparty = base.counterparty
LEFT JOIN signals ON signals.infobase = base.infobase AND signals.company_entity_key = base.counterparty
LEFT JOIN amount_forecast ON amount_forecast.infobase = base.infobase AND amount_forecast.company_entity_key = base.counterparty
LEFT JOIN docs_forecast ON docs_forecast.infobase = base.infobase AND docs_forecast.company_entity_key = base.counterparty
LEFT JOIN cases_current ON cases_current.infobase = base.infobase AND cases_current.company_entity_key = base.counterparty
LEFT JOIN detections_current ON detections_current.infobase = base.infobase AND detections_current.company_entity_key = base.counterparty
WHERE ifNull(alias_state.exclude_from_portfolio, 0) = 0;

CREATE OR REPLACE VIEW analytics_1c.v_company_management_actions_current AS
WITH
portfolio AS
(
    SELECT *
    FROM analytics_1c.v_company_portfolio_overview
),
risky_changes_24h AS
(
    SELECT
        infobase,
        company_entity_key,
        count() AS risky_changes_24h,
        max(ts) AS last_risky_change_at,
        arrayStringConcat(groupUniqArray(3)(if(field_name != '', field_name, change_kind)), ', ') AS risky_change_examples
    FROM analytics_1c.document_change_events
    WHERE risk_tag != ''
      AND ts >= now() - INTERVAL 1 DAY
    GROUP BY infobase, company_entity_key
),
large_adjustments_24h AS
(
    SELECT
        infobase,
        company_entity_key,
        count() AS large_adjustments_24h,
        max(ts) AS last_large_adjustment_at,
        max(toFloat64(amount)) AS max_large_adjustment_amount
    FROM analytics_1c.business_events
    WHERE ts >= now() - INTERVAL 1 DAY
      AND event_kind = 'posting'
      AND amount >= 50000
      AND (
          operation_type ILIKE '%adjust%'
          OR document_type ILIKE '%Коррект%'
      )
    GROUP BY infobase, company_entity_key
),
actions AS
(
    SELECT
        now() AS generated_at,
        concat('case_triage:', p.infobase, ':', p.company_entity_key) AS action_id,
        p.infobase AS infobase,
        p.company_entity_key AS company_entity_key,
        p.counterparty AS counterparty,
        p.company_name AS company_name,
        p.organization AS organization,
        if(p.registry_assignee_name != '', p.registry_assignee_name, if(p.owner_user != '', p.owner_user, 'не назначен')) AS owner_name,
        'case_triage' AS action_type,
        multiIf(
            p.signal_severity = 'critical' OR p.open_cases_total >= 10, 'critical',
            p.signal_severity = 'high' OR p.open_cases_total >= 5, 'high',
            'medium'
        ) AS priority_tier,
        toUInt32(greatest(toInt32(p.signal_score), 55) + least(toUInt32(p.open_cases_total), toUInt32(20))) AS priority_score,
        multiIf(p.signal_severity = 'critical' OR p.open_cases_total >= 10, 'today', '24h') AS deadline_hint,
        concat(
            'Открытых кейсов ',
            toString(p.open_cases_total),
            ', active detections ',
            toString(p.detections_total),
            if(p.top_signal != '', concat('. ', p.top_signal), '.')
        ) AS reason,
        'Назначить владельца на разбор кейсов и снять прирост до следующего управленческого цикла.' AS recommended_action,
        concat(
            'severity=', p.signal_severity,
            ', cases=', toString(p.open_cases_total),
            ', detections=', toString(p.detections_total)
        ) AS evidence_summary,
        toUInt32(p.open_cases_total) AS open_cases_total,
        toUInt32(p.detections_total) AS detections_total,
        toUInt32(p.active_locks) AS active_locks,
        toUInt16(p.days_since_last_activity) AS days_since_last_activity,
        toUInt32(ifNull(ch.risky_changes_24h, 0)) AS risky_changes_24h,
        toUInt32(ifNull(adj.large_adjustments_24h, 0)) AS large_adjustments_24h,
        toFloat64(ifNull(adj.max_large_adjustment_amount, 0)) AS max_large_adjustment_amount,
        p.last_seen_at AS last_seen_at
    FROM portfolio AS p
    LEFT JOIN risky_changes_24h AS ch ON ch.infobase = p.infobase AND ch.company_entity_key = p.company_entity_key
    LEFT JOIN large_adjustments_24h AS adj ON adj.infobase = p.infobase AND adj.company_entity_key = p.company_entity_key
    WHERE p.open_cases_total > 0

    UNION ALL

    SELECT
        now() AS generated_at,
        concat('lock_recovery:', p.infobase, ':', p.company_entity_key) AS action_id,
        p.infobase AS infobase,
        p.company_entity_key AS company_entity_key,
        p.counterparty AS counterparty,
        p.company_name AS company_name,
        p.organization AS organization,
        if(p.registry_assignee_name != '', p.registry_assignee_name, if(p.owner_user != '', p.owner_user, 'не назначен')) AS owner_name,
        'lock_recovery' AS action_type,
        multiIf(
            p.active_locks >= 5 OR p.current_status = 'busy', 'critical',
            p.active_locks >= 2, 'high',
            'medium'
        ) AS priority_tier,
        toUInt32(greatest(toInt32(p.signal_score), 50) + least(toUInt32(p.active_locks * 8), toUInt32(40))) AS priority_score,
        'today' AS deadline_hint,
        concat(
            'Файловая база в состоянии ',
            p.current_status,
            ', активных блокировок ',
            toString(p.active_locks),
            '.'
        ) AS reason,
        'Проверить busy/lock-контур, снять застрявшие блокировки и подтвердить стабилизацию базы.' AS recommended_action,
        concat(
            'status=', p.current_status,
            ', locks=', toString(p.active_locks),
            ', activity_score=', toString(p.current_activity_score)
        ) AS evidence_summary,
        toUInt32(p.open_cases_total) AS open_cases_total,
        toUInt32(p.detections_total) AS detections_total,
        toUInt32(p.active_locks) AS active_locks,
        toUInt16(p.days_since_last_activity) AS days_since_last_activity,
        toUInt32(ifNull(ch.risky_changes_24h, 0)) AS risky_changes_24h,
        toUInt32(ifNull(adj.large_adjustments_24h, 0)) AS large_adjustments_24h,
        toFloat64(ifNull(adj.max_large_adjustment_amount, 0)) AS max_large_adjustment_amount,
        p.last_seen_at AS last_seen_at
    FROM portfolio AS p
    LEFT JOIN risky_changes_24h AS ch ON ch.infobase = p.infobase AND ch.company_entity_key = p.company_entity_key
    LEFT JOIN large_adjustments_24h AS adj ON adj.infobase = p.infobase AND adj.company_entity_key = p.company_entity_key
    WHERE p.current_status = 'busy' OR p.active_locks > 0

    UNION ALL

    SELECT
        now() AS generated_at,
        concat('stale_activity:', p.infobase, ':', p.company_entity_key) AS action_id,
        p.infobase AS infobase,
        p.company_entity_key AS company_entity_key,
        p.counterparty AS counterparty,
        p.company_name AS company_name,
        p.organization AS organization,
        if(p.registry_assignee_name != '', p.registry_assignee_name, if(p.owner_user != '', p.owner_user, 'не назначен')) AS owner_name,
        'stale_activity' AS action_type,
        multiIf(
            p.days_since_last_activity >= 14, 'critical',
            p.days_since_last_activity >= 7, 'high',
            'medium'
        ) AS priority_tier,
        toUInt32(greatest(toInt32(p.signal_score), 45) + least(toUInt32(p.days_since_last_activity * 3), toUInt32(40))) AS priority_score,
        multiIf(p.days_since_last_activity >= 14, '24h', '3d') AS deadline_hint,
        concat(
            'Компания молчит ',
            toString(p.days_since_last_activity),
            ' дн., активность 30д ',
            toString(p.amount_30d),
            ', прогноз 30д ',
            toString(p.amount_forecast_30d),
            '.'
        ) AS reason,
        'Подтвердить, это плановая бизнес-пауза или выпадение предприятия из операционного ритма, и назначить владельца на возврат в норму.' AS recommended_action,
        concat(
            'days_since_last_activity=', toString(p.days_since_last_activity),
            ', docs_30d=', toString(p.docs_30d),
            ', forecast_30d=', toString(p.amount_forecast_30d)
        ) AS evidence_summary,
        toUInt32(p.open_cases_total) AS open_cases_total,
        toUInt32(p.detections_total) AS detections_total,
        toUInt32(p.active_locks) AS active_locks,
        toUInt16(p.days_since_last_activity) AS days_since_last_activity,
        toUInt32(ifNull(ch.risky_changes_24h, 0)) AS risky_changes_24h,
        toUInt32(ifNull(adj.large_adjustments_24h, 0)) AS large_adjustments_24h,
        toFloat64(ifNull(adj.max_large_adjustment_amount, 0)) AS max_large_adjustment_amount,
        p.last_seen_at AS last_seen_at
    FROM portfolio AS p
    LEFT JOIN risky_changes_24h AS ch ON ch.infobase = p.infobase AND ch.company_entity_key = p.company_entity_key
    LEFT JOIN large_adjustments_24h AS adj ON adj.infobase = p.infobase AND adj.company_entity_key = p.company_entity_key
    WHERE p.days_since_last_activity >= 7

    UNION ALL

    SELECT
        now() AS generated_at,
        concat('risky_change_review:', p.infobase, ':', p.company_entity_key) AS action_id,
        p.infobase AS infobase,
        p.company_entity_key AS company_entity_key,
        p.counterparty AS counterparty,
        p.company_name AS company_name,
        p.organization AS organization,
        if(p.registry_assignee_name != '', p.registry_assignee_name, if(p.owner_user != '', p.owner_user, 'не назначен')) AS owner_name,
        'risky_change_review' AS action_type,
        multiIf(ch.risky_changes_24h >= 3, 'critical', 'high') AS priority_tier,
        toUInt32(greatest(toInt32(p.signal_score), 60) + least(toUInt32(ch.risky_changes_24h * 12), toUInt32(40))) AS priority_score,
        'today' AS deadline_hint,
        concat(
            'За 24ч зафиксировано рискованных изменений документов: ',
            toString(ch.risky_changes_24h),
            if(ch.risky_change_examples != '', concat(' (', ch.risky_change_examples, ')'), '.')
        ) AS reason,
        'Проверить изменённые реквизиты и связать их с документами, проводками и ответственным пользователем.' AS recommended_action,
        concat(
            'risky_changes_24h=', toString(ch.risky_changes_24h),
            if(ch.risky_change_examples != '', concat(', fields=', ch.risky_change_examples), '')
        ) AS evidence_summary,
        toUInt32(p.open_cases_total) AS open_cases_total,
        toUInt32(p.detections_total) AS detections_total,
        toUInt32(p.active_locks) AS active_locks,
        toUInt16(p.days_since_last_activity) AS days_since_last_activity,
        toUInt32(ch.risky_changes_24h) AS risky_changes_24h,
        toUInt32(ifNull(adj.large_adjustments_24h, 0)) AS large_adjustments_24h,
        toFloat64(ifNull(adj.max_large_adjustment_amount, 0)) AS max_large_adjustment_amount,
        p.last_seen_at AS last_seen_at
    FROM portfolio AS p
    INNER JOIN risky_changes_24h AS ch ON ch.infobase = p.infobase AND ch.company_entity_key = p.company_entity_key
    LEFT JOIN large_adjustments_24h AS adj ON adj.infobase = p.infobase AND adj.company_entity_key = p.company_entity_key

    UNION ALL

    SELECT
        now() AS generated_at,
        concat('large_adjustment_review:', p.infobase, ':', p.company_entity_key) AS action_id,
        p.infobase AS infobase,
        p.company_entity_key AS company_entity_key,
        p.counterparty AS counterparty,
        p.company_name AS company_name,
        p.organization AS organization,
        if(p.registry_assignee_name != '', p.registry_assignee_name, if(p.owner_user != '', p.owner_user, 'не назначен')) AS owner_name,
        'large_adjustment_review' AS action_type,
        multiIf(adj.max_large_adjustment_amount >= 250000, 'critical', 'high') AS priority_tier,
        toUInt32(greatest(toInt32(p.signal_score), 65) + least(toUInt32(adj.large_adjustments_24h * 10), toUInt32(30))) AS priority_score,
        'today' AS deadline_hint,
        concat(
            'За 24ч крупных корректировок/проводок: ',
            toString(adj.large_adjustments_24h),
            ', максимальная сумма ',
            toString(adj.max_large_adjustment_amount),
            '.'
        ) AS reason,
        'Подтвердить основание крупной корректировки, автора и влияние на управленческий результат предприятия.' AS recommended_action,
        concat(
            'large_adjustments_24h=', toString(adj.large_adjustments_24h),
            ', max_amount=', toString(adj.max_large_adjustment_amount)
        ) AS evidence_summary,
        toUInt32(p.open_cases_total) AS open_cases_total,
        toUInt32(p.detections_total) AS detections_total,
        toUInt32(p.active_locks) AS active_locks,
        toUInt16(p.days_since_last_activity) AS days_since_last_activity,
        toUInt32(ifNull(ch.risky_changes_24h, 0)) AS risky_changes_24h,
        toUInt32(adj.large_adjustments_24h) AS large_adjustments_24h,
        toFloat64(adj.max_large_adjustment_amount) AS max_large_adjustment_amount,
        p.last_seen_at AS last_seen_at
    FROM portfolio AS p
    INNER JOIN large_adjustments_24h AS adj ON adj.infobase = p.infobase AND adj.company_entity_key = p.company_entity_key
    LEFT JOIN risky_changes_24h AS ch ON ch.infobase = p.infobase AND ch.company_entity_key = p.company_entity_key
)
SELECT
    *,
    multiIf(priority_tier = 'critical', toUInt8(4), priority_tier = 'high', toUInt8(3), priority_tier = 'medium', toUInt8(2), toUInt8(1)) AS priority_rank
FROM actions;

CREATE OR REPLACE VIEW analytics_1c.v_company_management_companies_current AS
SELECT
    generated_at,
    infobase,
    company_entity_key,
    counterparty,
    company_name,
    organization,
    owner_names,
    priority_rank_max AS priority_rank,
    priority_tier,
    priority_score,
    actions_total,
    critical_actions_total,
    high_actions_total,
    medium_actions_total,
    low_actions_total,
    action_types_summary,
    recommended_action_summary,
    reason_summary,
    evidence_summary,
    open_cases_total,
    detections_total,
    active_locks,
    days_since_last_activity,
    risky_changes_24h,
    large_adjustments_24h,
    max_large_adjustment_amount,
    last_seen_at
FROM
(
    SELECT
        max(a.generated_at) AS generated_at,
        infobase,
        company_entity_key,
        argMax(a.counterparty, tuple(a.priority_rank, a.priority_score, a.generated_at)) AS counterparty,
        argMax(a.company_name, tuple(a.priority_rank, a.priority_score, a.generated_at)) AS company_name,
        argMax(a.organization, tuple(a.priority_rank, a.priority_score, a.generated_at)) AS organization,
        arrayStringConcat(arraySort(groupUniqArray(a.owner_name)), ', ') AS owner_names,
        max(a.priority_rank) AS priority_rank_max,
        argMax(a.priority_tier, tuple(a.priority_rank, a.priority_score, a.generated_at)) AS priority_tier,
        max(a.priority_score) AS priority_score,
        count() AS actions_total,
        countIf(a.priority_tier = 'critical') AS critical_actions_total,
        countIf(a.priority_tier = 'high') AS high_actions_total,
        countIf(a.priority_tier = 'medium') AS medium_actions_total,
        countIf(a.priority_tier = 'low') AS low_actions_total,
        arrayStringConcat(arraySort(groupUniqArray(a.action_type)), ', ') AS action_types_summary,
        arrayStringConcat(groupUniqArray(3)(a.recommended_action), ' | ') AS recommended_action_summary,
        arrayStringConcat(groupUniqArray(3)(a.reason), ' | ') AS reason_summary,
        arrayStringConcat(groupUniqArray(3)(a.evidence_summary), ' | ') AS evidence_summary,
        max(a.open_cases_total) AS open_cases_total,
        max(a.detections_total) AS detections_total,
        max(a.active_locks) AS active_locks,
        max(a.days_since_last_activity) AS days_since_last_activity,
        max(a.risky_changes_24h) AS risky_changes_24h,
        max(a.large_adjustments_24h) AS large_adjustments_24h,
        max(a.max_large_adjustment_amount) AS max_large_adjustment_amount,
        max(a.last_seen_at) AS last_seen_at
    FROM analytics_1c.v_company_management_actions_current AS a
    GROUP BY infobase, company_entity_key
);
