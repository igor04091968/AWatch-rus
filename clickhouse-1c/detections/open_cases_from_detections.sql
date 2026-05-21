INSERT INTO analytics_1c.cases
SELECT
    ts AS opened_at,
    detection_id AS case_id,
    infobase,
    concat('1C ', upper(severity), ' · ', rule_title, ' · ', entity_id) AS title,
    severity,
    'open' AS status,
    '' AS assignee,
    detection_id,
    entity_type,
    entity_id,
    summary
FROM analytics_1c.detections
WHERE severity IN ('high', 'critical')
  AND detection_id NOT IN (SELECT case_id FROM analytics_1c.cases);
