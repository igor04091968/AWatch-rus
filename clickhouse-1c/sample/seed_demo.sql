INSERT INTO analytics_1c.documents (ts, infobase, organization, department, doc_type, doc_id, doc_number, author, counterparty, operation_type, amount, status, posted, source_file) VALUES
('2026-05-21 08:10:00','ТРАНСГАЗ 2026','Трансгаз','Продажи','Реализация','DOC-1','000001','USER1','ООО Альфа','sale',125000.00,'posted',1,'demo-docs.jsonl'),
('2026-05-21 09:40:00','ТРАНСГАЗ 2026','Трансгаз','Продажи','Возврат','DOC-2','000002','USER1','ООО Альфа','return',12000.00,'posted',1,'demo-docs.jsonl'),
('2026-05-21 10:15:00','ФЕЛИЦТ ГРУПП 2026','Фелицт','Бухгалтерия','Корректировка','DOC-3','000003','USER4','ООО Бета','adjustment',54000.00,'overdue',0,'demo-docs.jsonl');

INSERT INTO analytics_1c.postings (ts, infobase, registrar, operation_type, account_dt, account_ct, amount, source_file) VALUES
('2026-05-21 08:11:00','ТРАНСГАЗ 2026','DOC-1','sale','62.01','90.01',125000.00,'demo-postings.jsonl'),
('2026-05-21 10:16:00','ФЕЛИЦТ ГРУПП 2026','DOC-3','adjustment','91.02','62.01',54000.00,'demo-postings.jsonl');

INSERT INTO analytics_1c.reglog_events (ts, infobase, user, host, app, event_name, level, duration_ms, message, source_file) VALUES
('2026-05-21 07:15:00','ТРАНСГАЗ 2026','USER1','HOST-EXAMPLE','1cv8c','Login','warn',0,'Вход вне рабочего времени','demo-reglog.jsonl'),
('2026-05-21 10:20:00','ФЕЛИЦТ ГРУПП 2026','USER4','HOST-EXAMPLE','1cv8c','PostingError','error',4200,'Ошибка проведения документа','demo-reglog.jsonl'),
('2026-05-21 10:25:00','ФЕЛИЦТ ГРУПП 2026','USER4','HOST-EXAMPLE','1cv8c','ExchangeFailure','error',6100,'Ошибка обмена с внешней системой','demo-reglog.jsonl');

INSERT INTO analytics_1c.audit_events (ts, infobase, user, object_type, object_id, action, before_hash, after_hash, risk_tag, source_file) VALUES
('2026-05-21 10:17:00','ФЕЛИЦТ ГРУПП 2026','USER4','document','DOC-3','repost','abc','def','repost','demo-audit.jsonl'),
('2026-05-21 10:18:00','ФЕЛИЦТ ГРУПП 2026','USER4','counterparty','CP-77','change','old','new','critical_ref','demo-audit.jsonl');

INSERT INTO analytics_1c.host_events (ts, host, cpu_pct, ram_pct, disk_free_gb, disk_latency_ms, smb_errors, rdp_sessions, backup_ok, source_file) VALUES
('2026-05-21 10:00:00','HOST-EXAMPLE',41.2,68.4,120.0,12.5,0,4,1,'demo-host.jsonl'),
('2026-05-21 11:00:00','HOST-EXAMPLE',57.8,72.0,118.0,61.0,2,4,0,'demo-host.jsonl');
