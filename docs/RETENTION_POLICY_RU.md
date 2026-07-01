# Retention and Cleanup Policy

Дата актуализации: 2026-07-01

Документ фиксирует production-политику хранения и очистки для AWatch-rus /
DetMir. Цель политики - остановить неконтролируемый рост диска без неожиданного
удаления operational, audit, forensic или rollback-данных.

## Принципы

- Cleanup должен быть конфигурируемым через env, Ansible vars, compose vars или
  customer config без recompilation.
- Dry-run обязателен перед изменением сроков хранения или включением нового
  cleanup scope.
- Очистка не удаляет configuration, dashboards, rollback backups, release
  manifests, private env files и secrets.
- Forensic/security evidence не удаляется автоматически, если в репозитории нет
  явной безопасной политики и подтвержденного customer retention решения.
- DLP runtime в DetMir production остается light/disabled по текущим guardrails;
  retention policy не включает heavy DLP, Loki или always-on Velociraptor.

## Inventory persistent storage

Фактические источники найдены по Rust defaults, systemd/Ansible, compose,
PowerShell deployment config и существующим runbooks.

| Component | Data type | Path / storage | Existing cleanup |
| --- | --- | --- | --- |
| ActivityWatch server | primary event DB | `aw_server_db_path`, `/var/lib/activitywatch/aw-server-rust/sqlite.db` in Ansible | `aw-db-maintenance` removes only old process-level session events; no broad bucket deletion |
| ActivityWatch DB backups | SQLite backup files | `/var/lib/activitywatch/backups/db` | `aw-prune-local-state` via Ansible timer |
| ActivityWatch local backup metadata | JSON/root backup files | `/var/lib/activitywatch/backups` | `aw-prune-local-state` via Ansible timer |
| Worktime report cache | generated report cache | `/var/lib/activitywatch/worktime-report-cache` | TTL on read in `worktime-api`; proactive prune in `aw-prune-local-state` |
| Worktime management history | daily aggregate trend points | `/var/lib/activitywatch/worktime-management-history` | `worktime-api` prunes by date |
| Browser smoke artifacts | screenshots/result dirs/cache | `/var/lib/activitywatch/browser-smoke` | `aw-prune-local-state` |
| Temporary release/webui artifacts | temporary archives/scripts | `/tmp/activitywatch-*.zip`, `/tmp/hayabusa-*.zip`, webui temp files | `aw-prune-local-state` |
| Service logs | file logs | `/var/log/activitywatch/*.log` | `aw-server/logrotate.conf` |
| Journald | service journal | journald persistent/runtime storage | Ansible journald size caps and `journalctl --vacuum-size` |
| DetMir readiness bundle | signed readiness archives | `/var/lib/activitywatch/health/readiness-bundle` | `detmir-readiness` archive retention |
| DetMir auto/check state | check/report runs | `/var/lib/detmir-ai` | `detmir-auto` retains generated runs/files by `DETMIR_AI_RETAIN_DAYS` |
| Windows collector state | deployment config, queues, markers, logs | `C:\ProgramData\AWatch-rus` | no generic cleanup; queues are health-checked, not pruned |
| Windows EVTX exports | zipped forensic EVTX batches | `C:\ProgramData\AWatch-rus\forensics\evtx-exports` | `export-evtx-for-hayabusa.ps1` prunes by `retentionDays` |
| Windows incident artifacts | screenshots/evidence files | `C:\ProgramData\AWatch-rus\incident-artifacts`, per-user LocalAppData fallback | no automatic deletion in repo |
| DLP policy DB | policy/audit SQLite | `/var/lib/activitywatch/dlp-policy-engine.sqlite` | no automatic deletion |
| DLP case DB | case/audit SQLite | `/opt/activitywatch/dlp-case-management/cases.db` | no automatic deletion |
| DLP warehouse/evidence | evidence metadata and screenshots | `/var/lib/activitywatch/dlp_warehouse.sqlite`, `/var/lib/detmir-portal/evidence` | no automatic deletion |
| DLP aggregator DB | endpoint/fileops event warehouse | `data/dlp-events.sqlite3` unless overridden | no automatic deletion |
| DLP compliance reports | generated compliance artifacts | `/opt/activitywatch/dlp-compliance/reports` | no automatic deletion |
| Hayabusa server bundle | reports, archived input packages/payloads | `/opt/hayabusa/reports`, `/opt/hayabusa/archive` | archive only; no deletion |
| ClickHouse 1C | raw/core/security/business tables | Docker volume `clickhouse_1c_data` | policy doc exists; init SQL has no TTL |
| ClickHouse 1C landing/archive | exported raw files | `/opt/activitywatch/clickhouse-1c/landing`, `/archive` | ETL moves to archive; no automatic age prune |
| ClickHouse Workforce | raw/aggregate workforce tables | Docker volume `clickhouse_workforce_data` | no TTL in init SQL |
| Workforce ingest state | incremental loader state | `/var/lib/aw-workforce-ingest/state.json` | no cleanup needed; single state file |
| Prometheus | metrics TSDB | Docker volume `prometheus-data` | compose retention time, default `30d` |
| Grafana | SQLite/plugins/session state | Docker volume `grafana-data` | no repo cleanup; dashboards are provisioned read-only |
| Diagnostic bundles | operator support output | `/var/log/detmir-full-diagnostics` by example | no automatic deletion |
| Release evidence / rollout logs | build/release evidence and rollout logs | configured output dirs, `.rollout-logs` | no automatic deletion |
| Gitea registry backup | registry support backup | `/var/backups/gitea` | registry manifest expects `14` days |

## Retention matrix

Значения ниже взяты из текущего репозитория. Если срок в репозитории не задан,
cleanup остается disabled; recommended value фиксируется как operator decision,
а не как придуманное число.

| Component | Data type | Default retention | Minimum | Maximum | Cleanup method | Recovery impact | Disk usage impact | Recommended value |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| ActivityWatch primary DB | primary buckets/events | not globally configured | not configured | customer policy | no broad cleanup; only targeted `aw-db-maintenance` | broad deletion can break reports/audit | highest growth source | do not delete globally before customer data policy |
| AW process session events | process start/stop noise in session bucket | `7` days | `1` day in Rust validation | not enforced | `aw-db-maintenance --apply --json`, weekly timer | low; targeted process-level data only | medium on busy RDP hosts | `7` days until measured otherwise |
| AW SQLite vacuum | DB free pages | nightly | n/a | n/a | `aw-db-maintenance --vacuum --apply --json`, stops service via guard | short maintenance window | reduces reclaimed DB space | keep nightly low-load window |
| AW DB backups | SQLite backup files | `7` days, keep last `2` | keep last protects latest even with `0` days | not enforced | `aw-prune-local-state` | old rollback points removed; latest protected | medium | `7` days and keep last `2` |
| AW root backup JSON | backup metadata/root files | `7` days, keep last `2` | keep last protects latest even with `0` days | not enforced | `aw-prune-local-state` | old metadata removed | low | `7` days and keep last `2` |
| Worktime report disk cache | generated report JSON cache | `86400` seconds | `0` disables disk cache prune | not enforced | `aw-prune-local-state`; files under configured cache dir only | none; cache regenerates | medium under report load | `86400` seconds |
| Worktime in-memory events cache | process memory cache | `300` seconds in Ansible | `0` disables | not enforced | in-process TTL | none | RAM bounded by TTL/query limits | `300` seconds |
| Worktime in-memory report cache | process memory cache | `300` seconds in Ansible | `0` disables | not enforced | in-process TTL | none | RAM bounded by TTL/query limits | `300` seconds |
| Worktime stale report cache | stale fallback response | `3600` seconds in Ansible | at least report cache TTL in code | not enforced | in-process TTL | improves degraded-mode availability | low/medium | `3600` seconds |
| Worktime management history | aggregate daily trend points | `120` days | `1` day | `3660` days in code clamp | `worktime-api` date prune | old trend depth removed; raw events untouched | low | `120` days |
| Browser smoke artifacts | smoke runs/screenshots/cache | `1` day, keep `24` runs | `0` days plus keep last | not enforced | `aw-prune-local-state` | none; diagnostic artifacts only | low/medium | `1` day, keep `24` runs |
| Temporary archives | release/Hayabusa tmp archives | `1` day | `0` days | not enforced | `aw-prune-local-state` allowlisted names in `/tmp` | none if release already installed | medium | `1` day |
| Temporary webui files | webui patch temp files | `2` days | `0` days | not enforced | `aw-prune-local-state` allowlisted names in `/tmp` | none | low | `2` days |
| ActivityWatch file logs | `/var/log/activitywatch/*.log` | `30` daily rotations | logrotate policy | logrotate policy | `aw-server/logrotate.conf` | old logs compressed/removed | medium | keep `30` rotations |
| Journald | service journal | size caps `100M` system, `50M` runtime, keep free `500M` | journald policy | journald policy | Ansible drop-in and `journalctl --vacuum-size` | old journal lines removed | high on noisy hosts | keep current caps for small production |
| DetMir readiness archives | signed readiness evidence | `30` days | not enforced by doc; Rust accepts integer days | not enforced | `detmir-readiness`, daily timer | old readiness archives removed; latest files remain | low/medium | `30` days |
| DetMir auto/check state | generated run/report files | `14` days | `0` accepted by code | not enforced | `detmir-auto` cleanup | old diagnostic reports removed | low | `14` days if service is used |
| Windows EVTX exports | forensic EVTX zip/batch dirs | `14` days | `1` day via script max guard | not enforced | `export-evtx-for-hayabusa.ps1` | older EVTX export batches removed | high during incidents | `14` days unless legal hold |
| Windows queues | collector queue JSONL/locks | no retention | n/a | n/a | health/backlog detection only | deletion could lose unsent events | medium when AW unreachable | do not prune automatically |
| Windows incident artifacts | screenshots/evidence | no retention | n/a | n/a | disabled cleanup | deleting can break evidence chain | potentially high | operator/legal decision before cleanup |
| DLP policy DB | SQLite policy/audit | no retention | n/a | n/a | disabled cleanup | audit history loss if deleted | low/medium | retain until DLP policy decision |
| DLP case DB | SQLite cases/comments/audit | no retention | n/a | n/a | disabled cleanup | case/audit loss if deleted | low/medium | retain until case retention policy |
| DLP warehouse/evidence root | metadata/screenshots | no retention | n/a | n/a | disabled cleanup | evidence loss if deleted | high | retain until customer evidence policy |
| DLP compliance reports | generated compliance files | no retention | n/a | n/a | disabled cleanup | compliance evidence loss | medium | retain until compliance policy |
| Hayabusa reports/archive | timelines, packages, sidecars | no retention | n/a | n/a | archive only | forensic evidence loss if deleted | high after incident uploads | retain until operator/legal decision |
| ClickHouse 1C landing files | raw exported files before load | policy doc says `30` days | not enforced | not enforced | ETL archive/delete flags; no age prune | source replay loss if deleted | medium/high | `30` days only after implementing safe prune |
| ClickHouse 1C archived raw files | archived loaded files | policy doc says `90` days | not enforced | not enforced | no age prune in repo | replay/debug loss if deleted | high | `90` days only after implementing safe prune |
| ClickHouse 1C raw tables | `raw_*` tables | policy doc says `30` days | not enforced | not enforced | no TTL in init SQL | raw replay loss if TTL applied | high | `30` days after explicit ClickHouse TTL migration |
| ClickHouse 1C core/security tables | normalized tables/cases/timeline | policy doc says `365` days | not enforced | not enforced | no TTL in init SQL | audit/history loss if TTL applied | high | `365` days after customer approval |
| ClickHouse Workforce tables | raw and aggregate workforce tables | no retention | n/a | n/a | no TTL in init SQL | workforce history loss if TTL applied | high | define after production growth measurement |
| Workforce ingest state | one JSON state file | latest only | n/a | n/a | overwrite/atomic state | reset causes overlap/backfill | negligible | keep latest |
| Prometheus TSDB | metrics samples | `30d` | Prometheus config value | Prometheus config value | `--storage.tsdb.retention.time=${PROMETHEUS_RETENTION_TIME:-30d}` | old metrics removed | high but bounded | `30d` |
| Grafana data volume | dashboards state, users, sqlite | no repo cleanup | n/a | n/a | disabled cleanup | can break dashboards/users | low/medium | backup, do not prune automatically |
| Diagnostic bundles | support logs/reports | no retention | n/a | n/a | disabled cleanup | old diagnostic evidence removed | medium | operator decision per support package |
| Release evidence / rollout logs | release proof and rollout logs | no retention | n/a | n/a | disabled cleanup | auditability loss | low/medium | retain through release/support window |
| Gitea backup | registry backup dump | `14` days | policy-defined | policy-defined | `awatch-gitea-backup.timer` per registry docs | old registry backups removed | medium | `14` days until restore tests define otherwise |

## Cleanup implementation

### Active automatic cleanup

- `aw-prune-local-state.timer` is installed/enabled by
  `ansible/deploy_aw_server.yml` and runs daily at `04:40`.
- `aw-prune-local-state` prunes only allowlisted paths:
  AW backups, browser smoke runs, worktime report disk cache and selected `/tmp`
  artifacts.
- `aw-db-maintenance.timer` deletes only old process-level session events.
- `aw-db-vacuum.timer` performs SQLite vacuum in a low-load window.
- `detmir-readiness.timer` writes signed bundles and prunes old dated archives.
- `export-evtx-for-hayabusa.ps1` prunes old EVTX export folders/zips after a
  new export run.
- Prometheus TSDB is bounded by compose retention time.

### Disabled cleanup by design

No automatic deletion is implemented for ClickHouse 1C/workforce data, Hayabusa
archives, DLP evidence, DLP cases, DLP compliance reports, Windows queues,
Grafana data, release evidence or support diagnostic bundles. These areas can
contain audit, forensic, replay or rollback value. Cleanup must be added only
after operator/customer decision and dry-run validation.

## Configuration examples

Server env example: `aw-server/aw-server.env.example`.

```bash
AW_BACKUP_RETENTION_DAYS=7
AW_BACKUP_KEEP_LAST_DB=2
AW_BACKUP_KEEP_LAST_JSON=2
AW_WORKTIME_REPORT_DISK_CACHE_DIR=/var/lib/activitywatch/worktime-report-cache
AW_WORKTIME_REPORT_DISK_STALE_TTL_SECONDS=86400
AW_BROWSER_SMOKE_RETENTION_DAYS=1
AW_BROWSER_SMOKE_KEEP_RUNS=24
AW_TMP_ARCHIVE_RETENTION_DAYS=1
AW_TMP_WEBUI_RETENTION_DAYS=2
AW_DB_MAINTENANCE_RETENTION_DAYS=7
DETMIR_READINESS_RETENTION_DAYS=30
```

Ansible vars:

```yaml
aw_server_backup_retention_days: 7
aw_server_backup_keep_last_db: 2
aw_server_backup_keep_last_json: 2
aw_worktime_report_disk_cache_dir: "{{ aw_server_data_dir }}/worktime-report-cache"
aw_worktime_report_disk_stale_ttl_seconds: 86400
aw_worktime_management_history_retention_days: 120
aw_windows_evtx_retention_days: 14
```

Prometheus:

```bash
PROMETHEUS_RETENTION_TIME=30d docker compose up -d prometheus
```

Windows EVTX:

```powershell
powershell.exe -ExecutionPolicy Bypass `
  -File C:\ProgramData\AWatch-rus\export-evtx-for-hayabusa.ps1 `
  -RetentionDays 14
```

## Production validation

Dry-run cleanup:

```bash
sudo AW_DATA_DIR=/var/lib/activitywatch \
  AW_WORKTIME_REPORT_DISK_CACHE_DIR=/var/lib/activitywatch/worktime-report-cache \
  AW_WORKTIME_REPORT_DISK_STALE_TTL_SECONDS=86400 \
  /usr/local/bin/aw-prune-local-state-rust --json
```

Apply only after dry-run review:

```bash
sudo /usr/local/bin/aw-prune-local-state-rust --apply --json
```

Timer and logs:

```bash
systemctl list-timers aw-prune-local-state.timer aw-db-maintenance.timer aw-db-vacuum.timer detmir-readiness.timer
journalctl -u aw-prune-local-state.service -u aw-db-maintenance.service -u aw-db-vacuum.service -n 120 --no-pager
```

Disk estimate before/after:

```bash
du -sh /var/lib/activitywatch /var/log/activitywatch /opt/hayabusa 2>/dev/null || true
docker system df 2>/dev/null || true
docker exec aw-rus-1c-clickhouse clickhouse-client --query \
  "SELECT database, table, formatReadableSize(sum(bytes_on_disk)) AS size FROM system.parts WHERE active GROUP BY database, table ORDER BY sum(bytes_on_disk) DESC" 2>/dev/null || true
```

Service safety after cleanup:

```bash
systemctl status activitywatch-server aw-worktime-api --no-pager
curl -fsS http://127.0.0.1:5600/api/0/info >/dev/null
curl -fsS http://127.0.0.1:5610/healthz >/dev/null
```

## Recovery notes

- AW DB backups are under `/var/lib/activitywatch/backups/db`; latest backup
  files are protected by keep-last.
- Worktime disk cache does not need restore; reports regenerate from AW data.
- Browser smoke and `/tmp` artifacts do not need restore.
- Readiness latest files remain in the root of `readiness-bundle`; old dated
  archive directories are non-critical after the retention window.
- Forensic evidence, Hayabusa archives, DLP cases and compliance reports are not
  cleaned automatically and must be recovered from customer backup if operator
  deletes them manually.

## Known limitations

- ClickHouse 1C retention periods exist in `clickhouse-1c/ops/retention-policy.md`,
  but repository init SQL currently has no TTL clauses. Applying TTL to existing
  production tables requires a separate staged migration and customer approval.
- ClickHouse Workforce has no retention policy in repo; do not infer one from 1C
  retention.
- Windows incident screenshots and DLP evidence can grow during incidents.
  Cleanup is intentionally disabled until legal/operator retention is defined.
- Diagnostic bundles and release evidence are intentionally not pruned because
  they are often needed for support and audit.
