INSERT INTO analytics_1c.detections
SELECT *
FROM (
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
      AND toHour(ts) NOT BETWEEN 8 AND 20
) AS src
WHERE src.detection_id NOT IN (SELECT detection_id FROM analytics_1c.detections);

INSERT INTO analytics_1c.detections
SELECT *
FROM (
    SELECT
        ts,
        concat('large_manual_adjustment:', infobase, ':', document_id, ':', toString(toUnixTimestamp(ts)), ':', toString(line_no)) AS detection_id,
        infobase,
        'large_manual_adjustment' AS rule_id,
        'Крупная корректировка проводки' AS rule_title,
        'document' AS entity_type,
        document_id AS entity_id,
        'high' AS severity,
        75 AS score,
        concat('Крупная корректировка по документу ', document_id, ' amount=', toString(amount), ' счет ', debit_account, ' -> ', credit_account) AS summary,
        'open' AS status
    FROM analytics_1c.business_events
    WHERE event_kind = 'posting'
      AND amount >= 50000
      AND (
          operation_type ILIKE '%adjust%'
          OR document_type ILIKE '%Коррект%'
      )
) AS src
WHERE src.detection_id NOT IN (SELECT detection_id FROM analytics_1c.detections);

INSERT INTO analytics_1c.detections
SELECT *
FROM (
    SELECT
        ts,
        concat('risky_document_change:', infobase, ':', change_id) AS detection_id,
        infobase,
        'risky_document_change' AS rule_id,
        'Рискованное изменение документа' AS rule_title,
        if(document_id != '', 'document', 'counterparty') AS entity_type,
        if(document_id != '', document_id, company_entity_key) AS entity_id,
        'high' AS severity,
        70 AS score,
        concat('Изменение ', change_kind, ' field=', field_name, ' risk=', risk_tag) AS summary,
        'open' AS status
    FROM analytics_1c.document_change_events
    WHERE risk_tag != ''
) AS src
WHERE src.detection_id NOT IN (SELECT detection_id FROM analytics_1c.detections);

INSERT INTO analytics_1c.detections
SELECT *
FROM (
    SELECT
        generated_at AS ts,
        concat('company_signal:', signal_id, ':', toString(toUnixTimestamp(generated_at))) AS detection_id,
        infobase,
        concat('company_', signal_type) AS rule_id,
        'Сигнал активности компании' AS rule_title,
        'counterparty' AS entity_type,
        counterparty AS entity_id,
        severity,
        score,
        summary,
        'open' AS status
    FROM analytics_1c.v_company_health_current
    WHERE severity IN ('medium', 'high', 'critical')
) AS src
WHERE src.detection_id NOT IN (SELECT detection_id FROM analytics_1c.detections);

INSERT INTO analytics_1c.detections
SELECT *
FROM (
    SELECT
        event_ts AS ts,
        concat('failed_login_burst:', infobase, ':', user, ':', toString(toUnixTimestamp(event_ts))) AS detection_id,
        infobase,
        'failed_login_burst' AS rule_id,
        'Всплеск ошибок входа' AS rule_title,
        'user' AS entity_type,
        user AS entity_id,
        'high' AS severity,
        65 AS score,
        concat('У пользователя ', user, ' более 5 ошибок входа за 15 минут') AS summary,
        'open' AS status
    FROM (
        SELECT
            infobase,
            user,
            toStartOfFifteenMinutes(ts) AS window_ts,
            max(ts) AS event_ts,
            count() AS attempts
        FROM analytics_1c.reglog_events
        WHERE level IN ('error', 'warn')
          AND (event_name ILIKE '%login%' OR message ILIKE '%парол%' OR message ILIKE '%auth%')
        GROUP BY infobase, user, window_ts
        HAVING attempts >= 5
    )
) AS src
WHERE src.detection_id NOT IN (SELECT detection_id FROM analytics_1c.detections);

INSERT INTO analytics_1c.detections
SELECT *
FROM (
    SELECT
        event_ts AS ts,
        concat('disk_latency_high:', host, ':', toString(toUnixTimestamp(event_ts))) AS detection_id,
        '' AS infobase,
        'disk_latency_high' AS rule_id,
        'Высокая задержка диска' AS rule_title,
        'host' AS entity_type,
        host AS entity_id,
        'high' AS severity,
        65 AS score,
        concat('На хосте ', host, ' задержка диска превышает 50 мс') AS summary,
        'open' AS status
    FROM (
        SELECT
            host,
            toStartOfHour(ts) AS hour_ts,
            max(ts) AS event_ts,
            avg(disk_latency_ms) AS avg_latency_ms
        FROM analytics_1c.host_events
        GROUP BY host, hour_ts
        HAVING avg_latency_ms > 50
    )
) AS src
WHERE src.detection_id NOT IN (SELECT detection_id FROM analytics_1c.detections);
