INSERT INTO aw_workforce.dim_workstation_user
    (host_name, user_login, user_domain, employee_id, employee_name, department, branch, position, source)
VALUES
    ('SHARKON2025', 'user1', 'sharkon2025', 'sharkon2025\\user1', 'user1', 'tsj', 'tsj', 'RDP user', 'manual-p3');

SYSTEM RELOAD DICTIONARY aw_workforce.dict_workstation_user;
