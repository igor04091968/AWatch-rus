CREATE VIEW IF NOT EXISTS analytics_1c.v_documents_daily AS
SELECT
    toDate(ts) AS d,
    infobase,
    organization,
    doc_type,
    count() AS docs_total,
    sum(amount) AS amount_total,
    sumIf(amount, posted = 0) AS unposted_amount
FROM analytics_1c.documents
GROUP BY d, infobase, organization, doc_type;

CREATE VIEW IF NOT EXISTS analytics_1c.v_detections_daily AS
SELECT
    toDate(ts) AS d,
    infobase,
    severity,
    count() AS detections_total,
    sum(score) AS score_total
FROM analytics_1c.detections
GROUP BY d, infobase, severity;

CREATE VIEW IF NOT EXISTS analytics_1c.v_open_cases AS
SELECT *
FROM analytics_1c.cases
WHERE status != 'closed';
