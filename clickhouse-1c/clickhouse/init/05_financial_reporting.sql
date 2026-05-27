CREATE OR REPLACE VIEW analytics_1c.v_financial_reporting_readiness AS
SELECT
    max(be.ts) AS last_event_at,
    if(
        countIf(be.event_kind = 'posting' OR be.debit_account != '' OR be.credit_account != '') > 0,
        maxIf(be.ts, be.event_kind = 'posting' OR be.debit_account != '' OR be.credit_account != ''),
        CAST(NULL, 'Nullable(DateTime)')
    ) AS last_ledger_event_at,
    count() AS business_events_total,
    countIf(be.document_type = 'CompanyActivitySnapshot' AND be.event_kind = 'document_snapshot') AS proxy_events_total,
    countIf(be.event_kind = 'posting' OR be.debit_account != '' OR be.credit_account != '') AS ledger_events_total,
    countIf(be.debit_account != '' OR be.credit_account != '') AS account_lines_total,
    (SELECT count() FROM analytics_1c.postings) AS postings_table_rows,
    (SELECT if(count() > 0, max(ts), CAST(NULL, 'Nullable(DateTime)')) FROM analytics_1c.postings) AS postings_table_last_ts,
    multiIf(
        countIf(be.event_kind = 'posting' OR be.debit_account != '' OR be.credit_account != '') > 0 OR (SELECT count() FROM analytics_1c.postings) > 0,
        'ledger_ready',
        countIf(be.document_type = 'CompanyActivitySnapshot' AND be.event_kind = 'document_snapshot') > 0,
        'proxy_only',
        'empty'
    ) AS readiness_status
FROM analytics_1c.business_events AS be;

CREATE OR REPLACE VIEW analytics_1c.v_financial_daily AS
SELECT
    d,
    infobase,
    organization_name AS organization,
    source_kind,
    events_total,
    documents_total,
    amount_total
FROM
(
    SELECT
        toDate(ts) AS d,
        infobase,
        if(organization != '', organization, infobase) AS organization_name,
        'ledger' AS source_kind,
        count() AS events_total,
        uniqExact(document_id) AS documents_total,
        sum(amount) AS amount_total
    FROM analytics_1c.business_events
    WHERE event_kind = 'posting' OR debit_account != '' OR credit_account != ''
    GROUP BY d, infobase, organization_name

    UNION ALL

    SELECT
        toDate(ts) AS d,
        infobase,
        if(organization != '', organization, infobase) AS organization_name,
        'proxy' AS source_kind,
        count() AS events_total,
        uniqExact(document_id) AS documents_total,
        sum(amount) AS amount_total
    FROM analytics_1c.business_events
    WHERE event_kind = 'document_snapshot'
      AND document_type = 'CompanyActivitySnapshot'
    GROUP BY d, infobase, organization_name
);

CREATE OR REPLACE VIEW analytics_1c.v_financial_company_current AS
SELECT
    daily.infobase AS infobase,
    argMax(ifNull(nullIf(companies.company_name, ''), daily.infobase), daily.d) AS company_name,
    argMax(ifNull(nullIf(companies.organization, ''), daily.organization), daily.d) AS organization,
    argMax(ifNull(nullIf(companies.owner_user, ''), ''), daily.d) AS owner_user,
    sumIf(daily.amount_total, daily.source_kind = 'ledger' AND daily.d >= today() - 30) AS ledger_amount_30d,
    sumIf(daily.events_total, daily.source_kind = 'ledger' AND daily.d >= today() - 30) AS ledger_events_30d,
    sumIf(daily.documents_total, daily.source_kind = 'ledger' AND daily.d >= today() - 30) AS ledger_documents_30d,
    sumIf(daily.amount_total, daily.source_kind = 'proxy' AND daily.d >= today() - 30) AS proxy_amount_30d,
    sumIf(daily.events_total, daily.source_kind = 'proxy' AND daily.d >= today() - 30) AS proxy_events_30d,
    sumIf(daily.documents_total, daily.source_kind = 'proxy' AND daily.d >= today() - 30) AS proxy_documents_30d,
    if(
        countIf(daily.source_kind = 'ledger' AND daily.d >= today() - 30) > 0,
        maxIf(daily.d, daily.source_kind = 'ledger' AND daily.d >= today() - 30),
        maxIf(daily.d, daily.source_kind = 'proxy' AND daily.d >= today() - 30)
    ) AS last_financial_day,
    multiIf(
        countIf(daily.source_kind = 'ledger' AND daily.d >= today() - 30) > 0, 'ledger_ready',
        countIf(daily.source_kind = 'proxy' AND daily.d >= today() - 30) > 0, 'proxy_only',
        'empty'
    ) AS readiness_status,
    ifNull(changes.changes_30d, 0) AS changes_30d,
    ifNull(changes.risky_changes_30d, 0) AS risky_changes_30d
FROM analytics_1c.v_financial_daily AS daily
LEFT JOIN analytics_1c.v_companies_current AS companies ON companies.infobase = daily.infobase
LEFT JOIN
(
    SELECT
        infobase,
        count() AS changes_30d,
        countIf(risk_tag != '') AS risky_changes_30d
    FROM analytics_1c.document_change_events
    WHERE ts >= now() - INTERVAL 30 DAY
    GROUP BY infobase
) AS changes ON changes.infobase = daily.infobase
GROUP BY daily.infobase, ifNull(changes.changes_30d, 0), ifNull(changes.risky_changes_30d, 0);

CREATE OR REPLACE VIEW analytics_1c.v_financial_document_types_30d AS
SELECT
    source_kind,
    document_type,
    operation_type,
    count() AS events_total,
    uniqExact(infobase) AS infobases_total,
    sum(amount) AS amount_total
FROM
(
    SELECT
        ts,
        infobase,
        document_type,
        operation_type,
        amount,
        'ledger' AS source_kind
    FROM analytics_1c.business_events
    WHERE ts >= now() - INTERVAL 30 DAY
      AND (event_kind = 'posting' OR debit_account != '' OR credit_account != '')

    UNION ALL

    SELECT
        ts,
        infobase,
        document_type,
        operation_type,
        amount,
        'proxy' AS source_kind
    FROM analytics_1c.business_events
    WHERE ts >= now() - INTERVAL 30 DAY
      AND event_kind = 'document_snapshot'
      AND document_type = 'CompanyActivitySnapshot'
)
GROUP BY source_kind, document_type, operation_type;

CREATE OR REPLACE VIEW analytics_1c.v_financial_accounts_30d AS
SELECT
    side,
    account,
    count() AS entries_total,
    uniqExact(infobase) AS infobases_total,
    sum(amount) AS amount_total
FROM
(
    SELECT
        infobase,
        amount,
        'debit' AS side,
        debit_account AS account
    FROM analytics_1c.business_events
    WHERE ts >= now() - INTERVAL 30 DAY
      AND debit_account != ''

    UNION ALL

    SELECT
        infobase,
        amount,
        'credit' AS side,
        credit_account AS account
    FROM analytics_1c.business_events
    WHERE ts >= now() - INTERVAL 30 DAY
      AND credit_account != ''
)
GROUP BY side, account;
