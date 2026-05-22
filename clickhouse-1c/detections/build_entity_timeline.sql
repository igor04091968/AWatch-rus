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
        'document' AS entity_type,
        document_id AS entity_id,
        infobase,
        user AS actor,
        'business_events' AS source,
        concat('business:', event_kind, ':', operation_type) AS event_type,
        if(event_kind = 'posting', 'low', 'medium') AS severity,
        greatest(10, toUInt32(round(amount))) AS score,
        concat('business:', event_id) AS ref_id,
        concat('Business event ', event_kind, ' ', document_type, ' №', document_number, ' amount=', toString(amount)) AS summary
    FROM analytics_1c.business_events
    WHERE document_id != ''
) AS src
WHERE src.ref_id NOT IN (SELECT ref_id FROM analytics_1c.entity_timeline);

INSERT INTO analytics_1c.entity_timeline
SELECT *
FROM (
    SELECT
        ts,
        'counterparty' AS entity_type,
        company_entity_key AS entity_id,
        infobase,
        user AS actor,
        'business_events' AS source,
        concat('company:', event_kind) AS event_type,
        if(amount >= 50000, 'medium', 'low') AS severity,
        greatest(10, toUInt32(round(amount))) AS score,
        concat('business-company:', event_id) AS ref_id,
        concat('Business activity ', counterparty, ' ', operation_type, ' amount=', toString(amount)) AS summary
    FROM analytics_1c.business_events
    WHERE company_entity_key != ''
) AS src
WHERE src.ref_id NOT IN (SELECT ref_id FROM analytics_1c.entity_timeline);

INSERT INTO analytics_1c.entity_timeline
SELECT *
FROM (
    SELECT
        documents.ts AS ts,
        'counterparty' AS entity_type,
        ifNull(portfolio.company_entity_key, documents.counterparty) AS entity_id,
        documents.infobase AS infobase,
        documents.author AS actor,
        'documents' AS source,
        concat('counterparty:', documents.operation_type) AS event_type,
        if(documents.status = 'busy', 'medium', 'low') AS severity,
        greatest(10, toUInt32(round(documents.amount))) AS score,
        concat('counterparty:', ifNull(portfolio.company_entity_key, documents.counterparty), ':', documents.doc_id, ':', toString(toUnixTimestamp(documents.ts))) AS ref_id,
        concat('Активность компании ', ifNull(portfolio.company_name, documents.counterparty), ': ', documents.doc_type, ' score=', toString(documents.amount), ' status=', documents.status) AS summary
    FROM analytics_1c.documents AS documents
    LEFT JOIN analytics_1c.v_company_portfolio_overview AS portfolio
        ON portfolio.infobase = documents.infobase
       AND portfolio.source_counterparty = documents.counterparty
    WHERE documents.counterparty != ''
) AS src
WHERE src.ref_id NOT IN (SELECT ref_id FROM analytics_1c.entity_timeline);

INSERT INTO analytics_1c.entity_timeline
SELECT *
FROM (
    SELECT
        ts,
        if(document_id != '', 'document', 'counterparty') AS entity_type,
        if(document_id != '', document_id, company_entity_key) AS entity_id,
        infobase,
        user AS actor,
        'document_changes' AS source,
        concat('change:', change_kind, ':', field_name) AS event_type,
        if(risk_tag != '', 'high', 'medium') AS severity,
        if(risk_tag != '', 70, 35) AS score,
        concat('change:', change_id) AS ref_id,
        concat('Change ', change_kind, ' field=', field_name, ' risk=', risk_tag) AS summary
    FROM analytics_1c.document_change_events
    WHERE (document_id != '' OR company_entity_key != '')
) AS src
WHERE src.ref_id NOT IN (SELECT ref_id FROM analytics_1c.entity_timeline);

INSERT INTO analytics_1c.entity_timeline
SELECT *
FROM (
    SELECT
        last_company_snapshot_at AS ts,
        'counterparty' AS entity_type,
        company_entity_key AS entity_id,
        infobase,
        owner_user AS actor,
        'companies' AS source,
        'company_snapshot' AS event_type,
        if(current_status = 'busy' OR active_locks > 0 OR temp_db_present = 1, 'medium', 'low') AS severity,
        greatest(10, toUInt32(round(current_activity_score))) AS score,
        concat('company:', company_entity_key, ':', toString(toUnixTimestamp(last_company_snapshot_at))) AS ref_id,
        concat('Company snapshot ', company_name, ': status=', current_status, ' locks=', toString(active_locks), ' score=', toString(current_activity_score)) AS summary
    FROM analytics_1c.v_companies_current
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
