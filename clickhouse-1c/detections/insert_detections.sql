INSERT INTO analytics_1c.detections
SELECT
    ts,
    concat('after_hours_login:', infobase, ':', user, ':', toString(toUnixTimestamp(ts))) AS detection_id,
    infobase,
    'after_hours_login' AS rule_id,
    'Вход вне рабочего времени' AS rule_title,
    'user' AS entity_type,
    user AS entity_id,
    'medium' AS severity,
    35 AS score,
    concat('Пользователь ', user, ' выполнил вход вне рабочего времени') AS summary,
    'open' AS status
FROM analytics_1c.reglog_events
WHERE event_name ILIKE '%login%'
  AND toHour(ts) NOT BETWEEN 8 AND 20;

INSERT INTO analytics_1c.detections
SELECT
    max(ts) AS ts,
    concat('failed_login_burst:', infobase, ':', user, ':', toString(toUnixTimestamp(max(ts)))) AS detection_id,
    infobase,
    'failed_login_burst' AS rule_id,
    'Всплеск ошибок входа' AS rule_title,
    'user' AS entity_type,
    user AS entity_id,
    'high' AS severity,
    65 AS score,
    concat('У пользователя ', user, ' более 5 ошибок входа за 15 минут') AS summary,
    'open' AS status
FROM analytics_1c.reglog_events
WHERE level IN ('error', 'warn')
  AND (event_name ILIKE '%login%' OR message ILIKE '%парол%' OR message ILIKE '%auth%')
GROUP BY infobase, user, toStartOfFifteenMinutes(ts)
HAVING count() >= 5;

INSERT INTO analytics_1c.detections
SELECT
    max(ts) AS ts,
    concat('disk_latency_high:', host, ':', toString(toUnixTimestamp(max(ts)))) AS detection_id,
    '' AS infobase,
    'disk_latency_high' AS rule_id,
    'Высокая задержка диска' AS rule_title,
    'host' AS entity_type,
    host AS entity_id,
    'high' AS severity,
    65 AS score,
    concat('На хосте ', host, ' задержка диска превышает 50 мс') AS summary,
    'open' AS status
FROM analytics_1c.host_events
GROUP BY host, toStartOfHour(ts)
HAVING avg(disk_latency_ms) > 50;
