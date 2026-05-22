INSERT INTO analytics_1c.entity_timeline
SELECT *
FROM (
    SELECT
        ts,
        'document' AS entity_type,
        doc_id AS entity_id,
        infobase,
        author AS actor,
        'documents' AS source,
        concat('document:', doc_type) AS event_type,
        if(posted = 1, 'low', 'medium') AS severity,
        if(posted = 1, 5, 20) AS score,
        concat(doc_id, ':', toString(toUnixTimestamp(ts)), ':', doc_type) AS ref_id,
        concat('Документ ', doc_type, ' №', doc_number, ' статус=', status) AS summary
    FROM analytics_1c.documents
) AS src
WHERE src.ref_id NOT IN (SELECT ref_id FROM analytics_1c.entity_timeline);

INSERT INTO analytics_1c.entity_timeline
SELECT *
FROM (
    SELECT
        ts,
        'counterparty' AS entity_type,
        counterparty AS entity_id,
        infobase,
        author AS actor,
        'documents' AS source,
        concat('counterparty:', operation_type) AS event_type,
        if(status = 'busy', 'medium', 'low') AS severity,
        greatest(10, toUInt32(round(amount))) AS score,
        concat('counterparty:', counterparty, ':', doc_id, ':', toString(toUnixTimestamp(ts))) AS ref_id,
        concat('Активность компании ', counterparty, ': ', doc_type, ' score=', toString(amount), ' status=', status) AS summary
    FROM analytics_1c.documents
    WHERE counterparty != ''
) AS src
WHERE src.ref_id NOT IN (SELECT ref_id FROM analytics_1c.entity_timeline);

INSERT INTO analytics_1c.entity_timeline
SELECT *
FROM (
    SELECT
        ts,
        'counterparty' AS entity_type,
        company_name AS entity_id,
        infobase,
        owner_user AS actor,
        'companies' AS source,
        'company_snapshot' AS event_type,
        if(status = 'busy' OR active_locks > 0 OR temp_db_present = 1, 'medium', 'low') AS severity,
        greatest(10, toUInt32(round(activity_score))) AS score,
        concat('company:', infobase, ':', toString(toUnixTimestamp(ts))) AS ref_id,
        concat('Company snapshot ', company_name, ': status=', status, ' locks=', toString(active_locks), ' score=', toString(activity_score)) AS summary
    FROM analytics_1c.companies
) AS src
WHERE src.ref_id NOT IN (SELECT ref_id FROM analytics_1c.entity_timeline);

INSERT INTO analytics_1c.entity_timeline
SELECT *
FROM (
    SELECT
        ts,
        'user' AS entity_type,
        user AS entity_id,
        infobase,
        user AS actor,
        'reglog' AS source,
        event_name AS event_type,
        if(level IN ('error', 'warn'), 'medium', 'low') AS severity,
        if(level IN ('error', 'warn'), 25, 5) AS score,
        concat(user, ':', toString(toUnixTimestamp(ts)), ':', event_name) AS ref_id,
        message AS summary
    FROM analytics_1c.reglog_events
) AS src
WHERE src.ref_id NOT IN (SELECT ref_id FROM analytics_1c.entity_timeline);

INSERT INTO analytics_1c.entity_timeline
SELECT *
FROM (
    SELECT
        ts,
        object_type AS entity_type,
        object_id AS entity_id,
        infobase,
        user AS actor,
        'audit' AS source,
        action AS event_type,
        if(risk_tag != '', 'high', 'medium') AS severity,
        if(risk_tag != '', 60, 30) AS score,
        concat(object_type, ':', object_id, ':', toString(toUnixTimestamp(ts)), ':', action) AS ref_id,
        concat('Audit action ', action, ' risk=', risk_tag) AS summary
    FROM analytics_1c.audit_events
) AS src
WHERE src.ref_id NOT IN (SELECT ref_id FROM analytics_1c.entity_timeline);
