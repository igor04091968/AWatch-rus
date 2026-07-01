# Retention policy

Минимально разумный retention для файловой 1С analytics stack:

- landing/raw exports: `30` дней
- archived raw files: `90` дней
- raw_* в ClickHouse: `30` дней
- core tables: `365` дней
- detections/cases/timeline: `365` дней или по регламенту ИБ

Важно: это policy target, а не заявление о текущем автоматическом TTL. В
`clickhouse/init/*.sql` сейчас нет TTL clauses, поэтому production cleanup для
ClickHouse должен внедряться отдельной staged migration после backup, dry-run
оценки объема и operator/customer approval.

Сводная политика хранения всего контура: `../../docs/RETENTION_POLICY_RU.md`.

Если регуляторика требует больше, меняется отдельно от Grafana UI.
