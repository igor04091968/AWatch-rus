CREATE TABLE IF NOT EXISTS analytics_1c.security_findings
(
    ts DateTime64(3, 'UTC'),
    finding_id String,
    host String,
    user String,
    ip String,
    department LowCardinality(String),
    state LowCardinality(String),
    severity LowCardinality(String),
    confidence LowCardinality(String),
    score UInt16,
    source LowCardinality(String),
    rule_id String,
    rule_title String,
    summary String,
    recommended_action LowCardinality(String),
    management_channel_checked UInt8,
    evidence_ref String,
    raw_json String,
    ingested_at DateTime64(3, 'UTC') DEFAULT now64(3)
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(ts)
ORDER BY (state, severity, host, ts, finding_id);

CREATE TABLE IF NOT EXISTS analytics_1c.security_finding_workflow_events
(
    ts DateTime64(3, 'UTC'),
    finding_id String,
    event_type LowCardinality(String),
    status LowCardinality(String),
    actor String,
    comment String,
    decision_status String,
    rollback_plan_id String,
    plan_id String,
    evidence_json String
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(ts)
ORDER BY (finding_id, ts, event_type);

DROP VIEW IF EXISTS analytics_1c.security_finding_inbox;

CREATE VIEW analytics_1c.security_finding_inbox AS
SELECT
    f.finding_id AS finding_id,
    min(f.ts) AS first_seen,
    max(f.ts) AS last_seen,
    argMax(f.host, f.ingested_at) AS host,
    argMax(f.user, f.ingested_at) AS user,
    argMax(f.ip, f.ingested_at) AS ip,
    argMax(f.department, f.ingested_at) AS department,
    argMax(f.state, f.ingested_at) AS state,
    argMax(f.severity, f.ingested_at) AS severity,
    argMax(f.confidence, f.ingested_at) AS confidence,
    argMax(f.score, f.ingested_at) AS score,
    argMax(f.source, f.ingested_at) AS source,
    argMax(f.rule_id, f.ingested_at) AS rule_id,
    argMax(f.rule_title, f.ingested_at) AS rule_title,
    argMax(f.summary, f.ingested_at) AS summary,
    argMax(f.recommended_action, f.ingested_at) AS recommended_action,
    argMax(f.management_channel_checked, f.ingested_at) AS management_channel_checked,
    argMax(f.evidence_ref, f.ingested_at) AS evidence_ref,
    argMax(f.raw_json, f.ingested_at) AS raw_json,
    coalesce(nullIf(w.status, ''), 'new') AS workflow_status,
    coalesce(nullIf(w.event_type, ''), 'created') AS last_workflow_event,
    w.workflow_updated_at AS workflow_updated_at,
    coalesce(w.actor, '') AS workflow_actor,
    coalesce(w.decision_status, '') AS decision_status,
    coalesce(w.rollback_plan_id, '') AS rollback_plan_id,
    coalesce(w.plan_id, '') AS plan_id
FROM analytics_1c.security_findings AS f
LEFT JOIN
(
    SELECT
        finding_id,
        argMax(event_type, ts) AS event_type,
        argMax(status, ts) AS status,
        argMax(actor, ts) AS actor,
        argMax(decision_status, ts) AS decision_status,
        argMax(rollback_plan_id, ts) AS rollback_plan_id,
        argMax(plan_id, ts) AS plan_id,
        max(ts) AS workflow_updated_at
    FROM analytics_1c.security_finding_workflow_events
    GROUP BY finding_id
) AS w USING finding_id
GROUP BY
    f.finding_id,
    w.status,
    w.event_type,
    w.workflow_updated_at,
    w.actor,
    w.decision_status,
    w.rollback_plan_id,
    w.plan_id;
