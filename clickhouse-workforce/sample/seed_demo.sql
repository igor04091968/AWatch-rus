INSERT INTO aw_workforce.dim_workstation_user
    (host_name, user_login, user_domain, employee_id, employee_name, department, branch, position, source)
VALUES
    ('ws-001', 'ivanov', 'corp', 'E001', 'Иванов И.И.', 'Бухгалтерия', 'Филиал 1', 'Бухгалтер', 'demo'),
    ('ws-002', 'petrova', 'corp', 'E002', 'Петрова П.П.', 'Операционный отдел', 'Филиал 1', 'Оператор', 'demo');

INSERT INTO aw_workforce.dim_application_category
    (process_name, application_name, vendor, category, productivity_class, risk_level, is_system, source, comment)
VALUES
    ('1cv8.exe', '1C:Enterprise', '1C', '1c', 'productive', 'low', 0, 'demo', 'core business app'),
    ('chrome.exe', 'Google Chrome', 'Google', 'browser', 'neutral', 'low', 0, 'demo', 'domain classified separately'),
    ('soffice.bin', 'LibreOffice', 'The Document Foundation', 'office', 'productive', 'low', 0, 'demo', 'office suite');

INSERT INTO aw_workforce.dim_domain_category
    (domain, site_name, category, productivity_class, risk_level, business_allowed, source, comment)
VALUES
    ('intranet.local', 'Internal portal', 'internal_service', 'productive', 'low', 1, 'demo', 'internal work portal'),
    ('github.com', 'GitHub', 'developer', 'productive', 'low', 1, 'demo', 'developer workflow'),
    ('youtube.com', 'YouTube', 'media', 'neutral', 'medium', 0, 'demo', 'context-dependent media');

SYSTEM RELOAD DICTIONARY aw_workforce.dict_workstation_user;
SYSTEM RELOAD DICTIONARY aw_workforce.dict_application_category;
SYSTEM RELOAD DICTIONARY aw_workforce.dict_domain_category;

INSERT INTO aw_workforce.aw_window_events
    (event_time, host_name, user_login, process_name, window_title, duration_sec, source_bucket, source_event_id)
VALUES
    (now() - INTERVAL 20 MINUTE, 'ws-001', 'ivanov', '1cv8.exe', '1C - документы', 900, 'demo-window', 'w-001'),
    (now() - INTERVAL 15 MINUTE, 'ws-002', 'petrova', 'soffice.bin', 'Отчет', 600, 'demo-window', 'w-002'),
    (now() - INTERVAL 10 MINUTE, 'ws-unknown', 'unknown', 'unknown.exe', 'Unknown tool', 120, 'demo-window', 'w-003');

INSERT INTO aw_workforce.aw_browser_events
    (event_time, host_name, user_login, browser_name, url, title, duration_sec, source_bucket, source_event_id)
VALUES
    (now() - INTERVAL 9 MINUTE, 'ws-001', 'ivanov', 'chrome.exe', 'https://intranet.local/tasks', 'Tasks', 300, 'demo-browser', 'b-001'),
    (now() - INTERVAL 8 MINUTE, 'ws-002', 'petrova', 'chrome.exe', 'https://github.com/igor04091968/AWatch-rus', 'AWatch-rus', 240, 'demo-browser', 'b-002'),
    (now() - INTERVAL 7 MINUTE, 'ws-002', 'petrova', 'chrome.exe', 'https://unknown.example/path', 'Unknown', 90, 'demo-browser', 'b-003');
