# Operational Recovery Audit

Date: 2026-07-01

Scope: repository-first audit of the current AWatch-rus / DetMir operational
recovery capability. This document describes only mechanisms already present in
the repository. It does not define new backup or recovery mechanisms.

## Evidence Sources

The audit reviewed these repository sources:

- `docs/BACKUP_AND_RECOVERY_RU.md`
- `docs/RETENTION_POLICY_RU.md`
- `docs/OPERATIONS_RUNBOOK_RU.md`
- `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`
- `docs/DETMIR_RESTORE_BASELINE_2026-06-29_RU.md`
- `scripts/prod_backup_restore.sh`
- `adk-rust/crates/prod-backup-restore/src/main.rs`
- `ansible/deploy_aw_server.yml`
- `ansible/deploy_aw_windows.yml`
- `ansible/post_validate_aw_windows.yml`
- `aw-server/*.service`, `aw-server/*.timer`, `aw-server/*.path`
- `windows/ActivityWatch.Windows.Common.psm1`
- `windows/hardening-recovery.ps1`
- `windows/rebuild-worktime-tasks.ps1`
- `windows/fix-session-watchers.ps1`
- `windows/cleanup-disc-sessions.ps1`
- `clickhouse-1c/docker-compose.yml`
- `clickhouse-workforce/docker-compose.yml`
- `grafana-1c/docker-compose.yml`
- `docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md`
- `docs/registry/registry-evidence-manifest.json`

## Current Recovery Procedure Actually Supported

### Server-side service recovery

The repository supports service restart and health validation through systemd,
Ansible deployment, and smoke checks:

- `activitywatch-server.service` restarts on failure and runs with the configured
  AW server env file. Evidence: `aw-server/activitywatch-server.service`.
- `ansible/deploy_aw_server.yml` installs and restarts
  `activitywatch-server.service`, then waits for `/api/0/info`. Evidence:
  `ansible/deploy_aw_server.yml:2365-2375`.
- The generic operations runbook requires checking `/healthz`, `/readyz`,
  `/metrics`, service status, journal, and smoke scripts after recovery.
  Evidence: `docs/OPERATIONS_RUNBOOK_RU.md:59-71`.
- The production validation runbook defines the DetMir smoke sequence through
  `check-aw-data.sh`, `check-aw-full.sh`, and local contour smoke. Evidence:
  `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`.

### ActivityWatch SQLite maintenance and recovery support

The repository supports guarded maintenance and limited DB merge/migration
flows, not a complete automated restore:

- Weekly guarded SQLite maintenance is installed as
  `aw-db-maintenance.service` / `aw-db-maintenance.timer`. Evidence:
  `aw-server/aw-db-maintenance.service`, `aw-server/aw-db-maintenance.timer`.
- Optional SQLite vacuum is defined as `aw-db-vacuum.service` /
  `aw-db-vacuum.timer`. It is opt-in in Ansible through
  `aw_db_vacuum_timer_enabled`. Evidence:
  `aw-server/aw-db-vacuum.service`, `aw-server/aw-db-vacuum.timer`,
  `ansible/deploy_aw_server.yml:613-649`.
- Legacy root DB merge is implemented in `ansible/deploy_aw_server.yml` when
  `aw_legacy_db_merge_enabled` is true. It stops the service, backs up target
  and legacy DB files, merges them with `merge-aw-server-dbs`, installs the
  merged DB, restarts the service, and waits for API readiness. Evidence:
  `ansible/deploy_aw_server.yml:2248-2375`.
- A separate Rust `prod-backup-restore` binary exists only as a plan/checker.
  It explicitly rejects `--apply`. Evidence:
  `adk-rust/crates/prod-backup-restore/src/main.rs:100-105`.

### Windows collector recovery

The Windows side has the strongest implemented recovery automation:

- Deployment writes `deployment-config.json`, launcher scripts, and
  `recovery-loop.ps1`. Evidence:
  `windows/deploy-domain-users.ps1`, `windows/hardening-recovery.ps1`.
- `Write-ActivityWatchRecoveryScript` generates a script that imports
  `ActivityWatch.Windows.Common.psm1` and calls
  `Invoke-ActivityWatchRecoveryLoop`. Evidence:
  `windows/ActivityWatch.Windows.Common.psm1:1576-1597`.
- `Register-ActivityWatchRecoveryTask` creates the scheduled task
  `ActivityWatch Recovery`, using an interactive user when possible and SYSTEM
  fallback otherwise. Evidence:
  `windows/ActivityWatch.Windows.Common.psm1:2447-2488`.
- `Invoke-ActivityWatchRecoveryLoop` is an actual loop: it uses a lock file,
  cleans non-live session processes, starts the worktime session collector when
  allowed, starts configured live user launch tasks, and uses console fallback.
  Evidence: `windows/ActivityWatch.Windows.Common.psm1:2114-2168`.
- `Start-ActivityWatchTasks` starts launch tasks for live users and starts the
  recovery task. Evidence:
  `windows/ActivityWatch.Windows.Common.psm1:2656-2672`.
- `rebuild-worktime-tasks.ps1`, `fix-session-watchers.ps1`, and
  `cleanup-disc-sessions.ps1` provide manual repair paths for task/script
  regeneration, stale recovery loop restart, and disconnected-session cleanup.

### Hayabusa intake recovery

The repository supports event-driven reprocessing of uploaded Hayabusa packages:

- `aw-hayabusa-drop.path` watches `/opt/activitywatch/aw-rus-ops/drop` for zip
  packages and triggers `aw-hayabusa-drop.service`. Evidence:
  `aw-server/aw-hayabusa-drop.path`.
- `aw-hayabusa-drop.service` runs `/usr/local/bin/aw-hayabusa-autoprocess`.
  Evidence: `aw-server/aw-hayabusa-drop.service`.
- `ansible/deploy_aw_server.yml` creates Hayabusa directories, installs pinned
  Hayabusa, installs Rust helpers, creates the drop zone, enables
  `aw-hayabusa-drop.path`, and runs `aw-hayabusa doctor`. Evidence:
  `ansible/deploy_aw_server.yml:2780-3070`.

### Readiness evidence recovery support

The repository supports periodic readiness bundle generation:

- `detmir-readiness.service` writes readiness output to
  `/var/lib/activitywatch/health/readiness-bundle`. Evidence:
  `aw-server/detmir-readiness.service`.
- `detmir-readiness.timer` runs daily with persistent timer behavior. Evidence:
  `aw-server/detmir-readiness.timer`.

## Current Backup Procedure Actually Supported

### ActivityWatch server local backup artifacts

Implemented:

- `ansible/deploy_aw_server.yml` creates `/var/lib/activitywatch/backups` and
  `/var/lib/activitywatch/backups/db`. Evidence:
  `ansible/deploy_aw_server.yml:80-128`, `ansible/deploy_aw_server.yml:2263-2269`.
- Before legacy DB merge, Ansible copies the target DB and legacy root DB into
  timestamped files under `{{ aw_server_data_dir }}/backups/db`. Evidence:
  `ansible/deploy_aw_server.yml:2274-2294`.
- Before applying server-side settings/views/classes, Ansible copies current
  payloads to timestamped JSON backups under `{{ aw_server_data_dir }}/backups`.
  Evidence: `ansible/deploy_aw_server.yml:2406-2420`.
- `aw-prune-local-state.timer` prunes old local backups using configured
  retention and keep-last values. Evidence:
  `ansible/deploy_aw_server.yml:236-281`, `docs/RETENTION_POLICY_RU.md:70-71`.

Not currently implemented:

- A general scheduled full backup of the active ActivityWatch SQLite DB.
- A repository-defined off-host backup copy for AW DB, configs, Grafana,
  ClickHouse, DLP evidence, Hayabusa archives, or Windows collector state.

### Windows package rollback backup

Implemented:

- `Install-ActivityWatchPackage` backs up the existing install root into
  `install-<timestamp>` before replacing it, keeps only the latest two install
  backups, checks free space, and cleans temporary extraction directories.
  Evidence: `windows/ActivityWatch.Windows.Common.psm1:80-95`,
  `windows/ActivityWatch.Windows.Common.psm1:148-245`.
- `hardening-recovery.ps1` can run `Install-ActivityWatchPackage` when
  `-RepairPackage` is supplied. Evidence: `windows/hardening-recovery.ps1`.

Not currently implemented:

- Automatic restore from the saved Windows `install-*` backup directory.
- Backup of `C:\ProgramData\AWatch-rus` as a whole before repair.

### Gitea registry backup

Implemented as registry-readiness support, not DetMir runtime recovery:

- Registry docs define Gitea backup path, script, systemd service/timer,
  `gitea dump` ZIP format, SHA256 checksum, daily schedule, and 14-day
  retention. Evidence:
  `docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md:7-23`,
  `docs/registry/registry-evidence-manifest.json:26-39`.

Not currently implemented:

- Tested Gitea restore. The manifest explicitly says `restore_tested=false` and
  `production_ready=false`. Evidence:
  `docs/registry/registry-evidence-manifest.json:37-39`.

## Current Restore Procedure

### ActivityWatch DB merge/restore-like flow

Supported:

1. Enable `aw_legacy_db_merge_enabled`.
2. Deploy server playbook.
3. Playbook checks legacy root DB and target DB.
4. Playbook stops `activitywatch-server.service`.
5. Playbook backs up target and legacy DB files.
6. Playbook runs `/usr/local/bin/merge-aw-server-dbs`.
7. Playbook installs the merged DB as active target DB.
8. Playbook restarts `activitywatch-server.service`.
9. Playbook waits for `/api/0/info`.

Evidence: `ansible/deploy_aw_server.yml:2248-2375`.

Not currently implemented:

- A generic "restore selected backup file to active AW DB" command.
- A tested end-to-end AW DB restore runbook.
- Automated checksum verification for AW DB backup files.
- Automated rollback from a failed DB merge to the backup file.

### `prod-backup-restore` plan-only flow

Supported:

1. `scripts/prod_backup_restore.sh` locates the Rust planner binary or exits
   with build instructions. Evidence: `scripts/prod_backup_restore.sh:14-29`.
2. The Rust planner reads `private-config/runtime.env` if available, checks
   required env vars, checks `sshpass`, `ansible-playbook`, inventory, and
   `merge-aw-server-dbs`. Evidence:
   `adk-rust/crates/prod-backup-restore/src/main.rs:111-131`,
   `adk-rust/crates/prod-backup-restore/src/main.rs:175-209`.
3. The planner prints planned commands including remote backup directory
   creation, DB copies, service stop, merge, install, and Ansible validation.
   Evidence: `adk-rust/crates/prod-backup-restore/src/main.rs:211-280`.

Not currently implemented:

- Execution of the planned restore. `--apply` fails by design. Evidence:
  `adk-rust/crates/prod-backup-restore/src/main.rs:100-105`.

### Windows collector recovery

Supported:

1. Run `ActivityWatch Recovery` scheduled task or let it run on its configured
   trigger.
2. Recovery loop enforces a single lock.
3. Recovery loop stops collectors/watchers in non-live sessions.
4. Recovery loop starts the global worktime collector when allowed.
5. Recovery loop starts configured live-user launch tasks.
6. Recovery loop attempts console fallback when no configured live task starts.

Evidence: `windows/ActivityWatch.Windows.Common.psm1:2114-2168`,
`windows/ActivityWatch.Windows.Common.psm1:2447-2488`,
`windows/ActivityWatch.Windows.Common.psm1:2656-2672`.

Not currently implemented:

- Automatic reconstruction of lost Windows state from an external backup.
- Automatic restore of Windows collector queues after corruption or deletion.

### Gitea restore

Supported:

- Manual outline exists: prepare isolated test server, install same Gitea
  version, stop Gitea, verify checksum, unpack dump, restore app/data/repos/db
  according to Gitea official procedure, fix ownership, start Gitea, run
  `gitea doctor check`, regenerate hooks if needed, and run post-restore
  checks. Evidence:
  `docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md:42-90`.

Not currently implemented:

- Tested Gitea restore. Evidence:
  `docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md:92-100`.

## Components That Cannot Yet Be Restored Automatically

The following are confirmed by repository inspection:

| Component | Current state | Evidence |
| --- | --- | --- |
| ActivityWatch active SQLite DB | Backups exist around merge/settings operations, but generic restore is Not currently implemented. | `ansible/deploy_aw_server.yml:2274-2294`, `adk-rust/crates/prod-backup-restore/src/main.rs:100-105` |
| ClickHouse 1C data | Docker volume exists; no backup/restore automation found. Not currently implemented. | `clickhouse-1c/docker-compose.yml:13-19` |
| ClickHouse Workforce data | Docker volume exists; no backup/restore automation found. Not currently implemented. | `clickhouse-workforce/docker-compose.yml:11-17` |
| Grafana data volume | Docker volume exists; no repo cleanup and no restore automation. Not currently implemented. | `grafana-1c/docker-compose.yml:67-79`, `docs/RETENTION_POLICY_RU.md:99` |
| Prometheus TSDB | Retention configured, backup/restore not documented. Not currently implemented. | `grafana-1c/docker-compose.yml:40-49` |
| DLP policy/case/warehouse DBs | Retention doc says no automatic deletion; backup/restore not implemented. Not currently implemented. | `docs/RETENTION_POLICY_RU.md:43-47`, `docs/RETENTION_POLICY_RU.md:87-90` |
| DLP evidence and compliance reports | Cleanup disabled; restore depends on customer backup if manually deleted. Not currently implemented. | `docs/RETENTION_POLICY_RU.md:221-223` |
| Hayabusa reports/archive | Processing is automated; restore of archive/reports is not automated. Not currently implemented. | `docs/RETENTION_POLICY_RU.md:48`, `docs/RETENTION_POLICY_RU.md:91` |
| Windows collector state and queues | Recovery restarts collectors; external backup/restore of state is not implemented. | `docs/RETENTION_POLICY_RU.md:40`, `docs/RETENTION_POLICY_RU.md:85` |
| Diagnostic bundles and release evidence | No automatic backup/restore found. Not currently implemented. | `docs/RETENTION_POLICY_RU.md:100-101` |

## Components Requiring Manual Intervention

- AW DB merge/recovery: operator must enable `aw_legacy_db_merge_enabled`, run
  Ansible, review backup files, and verify API. Evidence:
  `ansible/deploy_aw_server.yml:2248-2375`.
- `prod-backup-restore`: operator can only review a plan; execution is manual
  because `--apply` is disabled. Evidence:
  `adk-rust/crates/prod-backup-restore/src/main.rs:100-105`.
- Windows collector recovery after severe state loss: operator must use
  `hardening-recovery.ps1`, `rebuild-worktime-tasks.ps1`,
  `fix-session-watchers.ps1`, or redeploy. Evidence: `windows/*.ps1`.
- Hayabusa stuck path/service: operator may need to reset failed units, repair
  drop-zone permissions, and rerun processing. Evidence:
  `aw-server/aw-hayabusa-drop.path`, `aw-server/aw-hayabusa-drop.service`.
- Gitea restore: manual isolated test restore is required; tested restore is
  not yet recorded. Evidence:
  `docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md:42-100`.
- ClickHouse/Grafana/Prometheus/DLP data restore: Not currently implemented.

## Missing Documentation

Critical and high-confidence gaps only:

- Exact AW DB restore runbook from `/var/lib/activitywatch/backups/db` to the
  active DB path. Not currently implemented.
- Post-restore verification checklist for AW DB backup restore, including
  checksum, ownership, service restart, bucket freshness, and worktime report
  checks. Not currently implemented.
- Backup inventory mapping each persistent component to an actual backup owner,
  schedule, storage location, retention, and restore command. Partially covered
  by `docs/BACKUP_AND_RECOVERY_RU.md` and `docs/RETENTION_POLICY_RU.md`, but
  operational restore ownership is Not currently implemented.
- ClickHouse 1C and ClickHouse Workforce backup/restore runbooks. Not currently
  implemented.
- Grafana volume restore runbook. Not currently implemented.
- DLP evidence/case/policy restore runbook. Not currently implemented.
- Windows `C:\ProgramData\AWatch-rus` state backup/restore runbook. Not
  currently implemented.
- Gitea restore evidence result. The runbook exists, but restore test is marked
  false. Evidence: `docs/registry/registry-evidence-manifest.json:37-39`.

## Missing Automation

- Automated AW DB restore from a selected backup file. Not currently
  implemented.
- Automated AW DB backup with checksum on a schedule independent of merge
  operations. Not currently implemented.
- Off-host/offline copy for AW DB, ClickHouse volumes, Grafana data, DLP
  evidence, Hayabusa archives, Windows state, and release evidence. Not
  currently implemented.
- ClickHouse backup and restore automation. Not currently implemented.
- Grafana data volume backup and restore automation. Not currently implemented.
- DLP evidence/case/policy backup and restore automation. Not currently
  implemented.
- Windows state backup and restore automation. Not currently implemented.
- Automated restore drill evidence generation. Not currently implemented.

## Operational Risks

| Risk | Severity | Evidence | Impact |
| --- | --- | --- | --- |
| Restore is partially plan-only for AW DB | Critical | `prod-backup-restore` rejects `--apply` | Operator can plan but cannot run a deterministic automated restore through this tool |
| AW DB backups are created around specific operations, not as a general scheduled full backup | Critical | `ansible/deploy_aw_server.yml:2274-2294` | A recent recovery point may be unavailable if no merge/settings operation occurred |
| ClickHouse data has no repo-defined restore path | Critical | ClickHouse Docker volumes only | Loss/corruption of 1C or workforce ClickHouse data requires ad hoc operator recovery |
| DLP/Hayabusa evidence has no automated restore | High | `docs/RETENTION_POLICY_RU.md:87-91`, `docs/RETENTION_POLICY_RU.md:221-223` | Forensic/case continuity depends on external/customer backup |
| Grafana data volume has no repo-defined backup/restore | High | `grafana-1c/docker-compose.yml:67-79` | Dashboard DB/users/session state may require manual reconstruction even though provisioned dashboards exist |
| Gitea restore is documented but untested | High | `restore_tested=false` | Source-control recovery confidence remains limited |
| Windows recovery restarts collectors but does not restore deleted state | Medium | Windows recovery loop evidence | Collector state/queues/logs can be lost if state root is deleted |
| Generic backup document is intentionally high-level | Medium | `docs/BACKUP_AND_RECOVERY_RU.md:5-6` | Operators need component-specific procedures during incidents |
| Hayabusa path recovery depends on service/path health and permissions | Medium | `aw-hayabusa-drop.path`, `aw-hayabusa-drop.service` | Drop backlog or permission drift can stall forensic intake |
| Recovery smoke is documented but not tied to a single recovery command | Low | `docs/OPERATIONS_RUNBOOK_RU.md`, `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md` | Operator can validate, but command sequencing remains manual |

## Recovery Confidence Score

Overall score: **48 / 100**

Justification:

- Windows collector process recovery is mature: scheduled task, lock, live
  session handling, cleanup of non-live sessions, and task restart are
  implemented.
- Server service recovery and smoke validation are present.
- Local maintenance and retention are present.
- AW DB merge has safety backups and API validation, but generic restore is not
  implemented.
- `prod-backup-restore` is explicitly plan-only.
- ClickHouse, Grafana, Prometheus, DLP evidence/cases, Hayabusa archive, Windows
  state, diagnostic bundles, and release evidence do not have automated restore
  procedures in the repository.
- Gitea backup is documented with checksum and timer metadata, but restore is
  marked untested.

## Prioritized Confirmed Gaps

### Critical

1. Generic AW DB restore from backup is Not currently implemented.
   Evidence: backup files are created by Ansible, but `prod-backup-restore`
   refuses `--apply`.

2. Scheduled full AW DB backup independent of merge/settings changes is Not
   currently implemented.
   Evidence: backups are tied to merge/settings operations and pruning exists,
   but no scheduled full DB backup unit is present.

3. ClickHouse 1C and Workforce backup/restore are Not currently implemented.
   Evidence: both stacks persist to Docker volumes; no backup/restore runbook or
   automation is present.

### High

4. DLP evidence/case/policy restore is Not currently implemented.
   Evidence: retention policy explicitly leaves DLP evidence/cases without
   automatic cleanup and says recovery depends on customer backup after manual
   deletion.

5. Hayabusa reports/archive restore is Not currently implemented.
   Evidence: intake processing is automated, but archive/report restore is not.

6. Grafana data volume backup/restore is Not currently implemented.
   Evidence: Grafana uses `grafana-data`; no repo restore procedure exists.

7. Gitea restore test is Not currently implemented.
   Evidence: registry manifest has `restore_tested=false`.

### Medium

8. Windows state root backup/restore is Not currently implemented.
   Evidence: Windows recovery restarts collectors and repairs tasks, but does
   not restore `C:\ProgramData\AWatch-rus` from backup.

9. Component-specific recovery ownership matrix is Not currently implemented.
   Evidence: generic backup doc says component list must be refined by release
   profile and customer infrastructure.

10. Restore drill evidence generation is Not currently implemented.
    Evidence: validation commands exist, but no restore-drill artifact workflow
    is present.

### Low

11. Prometheus TSDB restore is Not currently implemented.
    Evidence: compose config bounds retention, but no backup/restore procedure
    is present.

12. Diagnostic bundle and release evidence restore is Not currently
    implemented.
    Evidence: retention policy intentionally avoids pruning, but does not define
    backup or restore automation.

## Release-Relevant Conclusion

The repository currently supports operational restart, Windows collector
self-healing, local maintenance, retention, readiness evidence, and limited
backup-before-mutation behavior.

It does not yet support deterministic full production recovery for all persisted
state. The most important gap is not service restart; it is data restore:
ActivityWatch DB, ClickHouse volumes, Grafana state, DLP/Hayabusa evidence, and
Windows state are not covered by a tested automated restore process in the
repository.
