CREATE OR REPLACE VIEW aw_workforce.v_workforce_productivity_daily AS
SELECT
    event_date,
    branch,
    department,
    activity_type,
    category,
    productivity_class,
    sum(duration_sec) AS duration_sec,
    sum(event_count) AS event_count,
    sum(unknown_subject_events) AS unknown_subject_events,
    sum(unknown_category_events) AS unknown_category_events
FROM aw_workforce.agg_workforce_productivity_hourly
GROUP BY
    event_date,
    branch,
    department,
    activity_type,
    category,
    productivity_class;

CREATE OR REPLACE VIEW aw_workforce.v_workforce_unknown_subjects AS
SELECT
    host_name,
    user_login,
    count() AS events,
    sum(duration_sec) AS duration_sec
FROM aw_workforce.aw_window_events
WHERE dictGetUInt8OrDefault(
        'aw_workforce.dict_workstation_user',
        'is_active',
        (host_name, user_login),
        0
      ) != 1
   OR dictGetStringOrDefault(
        'aw_workforce.dict_workstation_user',
        'employee_name',
        (host_name, user_login),
        ''
      ) = ''
GROUP BY
    host_name,
    user_login
ORDER BY duration_sec DESC;

CREATE OR REPLACE VIEW aw_workforce.v_workforce_unknown_processes AS
SELECT
    process_name,
    count() AS events,
    sum(duration_sec) AS duration_sec
FROM aw_workforce.aw_window_events
WHERE dictGetUInt8OrDefault(
        'aw_workforce.dict_application_category',
        'is_active',
        process_name,
        0
      ) != 1
   OR dictGetStringOrDefault(
        'aw_workforce.dict_application_category',
        'category',
        process_name,
        ''
      ) IN ('', 'unknown')
GROUP BY process_name
ORDER BY duration_sec DESC;

CREATE OR REPLACE VIEW aw_workforce.v_workforce_unknown_domains AS
WITH
    lowerUTF8(
        domain(if(position(url, '://') = 0, concat('http://', url), url))
    ) AS domain_name
SELECT
    domain_name,
    count() AS events,
    sum(duration_sec) AS duration_sec
FROM aw_workforce.aw_browser_events
WHERE domain_name != ''
  AND (
      dictGetUInt8OrDefault(
        'aw_workforce.dict_domain_category',
        'is_active',
        domain_name,
        0
      ) != 1
      OR dictGetStringOrDefault(
        'aw_workforce.dict_domain_category',
        'category',
        domain_name,
        ''
      ) IN ('', 'unknown')
  )
GROUP BY domain_name
ORDER BY duration_sec DESC;

CREATE OR REPLACE VIEW aw_workforce.v_workforce_unknown_quality_daily AS
SELECT
    event_date,
    sum(event_count) AS events,
    sum(unknown_subject_events) AS unknown_subject_events,
    round(unknown_subject_events / nullIf(events, 0), 4) AS unknown_subject_ratio,
    sum(unknown_category_events) AS unknown_category_events,
    round(unknown_category_events / nullIf(events, 0), 4) AS unknown_category_ratio
FROM aw_workforce.agg_workforce_productivity_hourly
GROUP BY event_date
ORDER BY event_date DESC;
