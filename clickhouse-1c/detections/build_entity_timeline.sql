INSERT INTO analytics_1c.entity_timeline
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
    doc_id AS ref_id,
    concat('Документ ', doc_type, ' №', doc_number, ' статус=', status) AS summary
FROM analytics_1c.documents;

INSERT INTO analytics_1c.entity_timeline
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
    concat(user, ':', toString(toUnixTimestamp(ts))) AS ref_id,
    message AS summary
FROM analytics_1c.reglog_events;

INSERT INTO analytics_1c.entity_timeline
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
    concat(object_type, ':', object_id, ':', toString(toUnixTimestamp(ts))) AS ref_id,
    concat('Audit action ', action, ' risk=', risk_tag) AS summary
FROM analytics_1c.audit_events;
