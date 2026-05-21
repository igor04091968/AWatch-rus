# Retention policy

Минимально разумный retention для файловой 1С analytics stack:

- landing/raw exports: `30` дней
- archived raw files: `90` дней
- raw_* в ClickHouse: `30` дней
- core tables: `365` дней
- detections/cases/timeline: `365` дней или по регламенту ИБ

Если регуляторика требует больше, меняется отдельно от Grafana UI.
