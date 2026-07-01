# Disaster Recovery Proof

Date: 2026-07-01

Scope: evidence-backed disaster recovery validation for the current
AWatch-rus / DetMir production deployment, based only on repository contents.
This document does not introduce new recovery procedures and does not change
production behavior.

## Result

Current proof level: partially proven.

Recovery confidence: 48 / 100.

The repository contains deployable components, restart/health checks, Windows
collector recovery automation, ActivityWatch DB merge support, retention guard
rails, and validation scripts. The repository does not yet contain evidence of
a completed separate-host restore for the primary repository/Gitea backup, and
does not implement fully automated restore for all runtime data stores.

Production 1.0 release implication: DR is not blocked by missing documentation,
but remains blocked by missing restore evidence for the repository/Gitea backup
and by missing automated restore coverage for several runtime stores.

## Status Definitions

- Implemented: repository contains executable automation or a concrete
  validation script for the step.
- Partially implemented: repository contains part of the mechanism, but not an
  end-to-end proven path.
- Manual: repository documents the step, but operator execution and evidence
  capture are required.
- Missing: repository evidence shows the step is not implemented.
- Unsupported: repository code explicitly rejects or excludes the operation.

## Evidence Inventory

| Area | Status | Evidence |
| --- | --- | --- |
| General backup/recovery model | Manual | `docs/BACKUP_AND_RECOVERY_RU.md:59-71` defines the generic stop, preserve state, restore, start, health, smoke, and record-result sequence. |
| Production health endpoints | Implemented | `docs/OPERATIONS_RUNBOOK_RU.md:5-13` defines `/healthz`, `/readyz`, and `/metrics` checks; `docs/OPERATIONS_RUNBOOK_RU.md:18-47` defines expected semantics. |
| Post-recovery smoke | Implemented | `docs/OPERATIONS_RUNBOOK_RU.md:49-64` lists smoke scripts and requires them after recovery. |
| Operational maturity validation | Implemented | `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md:49-59` requires `scripts/public_secret_pattern_check.py`, `scripts/operational-maturity-check.mjs`, and `quality-gate`. |
| Browser/operator validation | Manual | `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md:64-93` lists required operator pages and secret-handling rules. |
| DetMir production smoke | Implemented | `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md:95-119` defines `check-aw-full.sh`, `check-aw-data.sh`, and stable `SHARKON2025` host id handling. |
| ActivityWatch service startup | Implemented | `aw-server/activitywatch-server.service:6-14` defines env file, DB path, web path, restart policy, and restart delay. |
| ActivityWatch DB maintenance | Implemented | `aw-server/aw-db-maintenance.service:29-32` and `aw-server/aw-db-maintenance.timer:34-42` define guarded maintenance. |
| Readiness evidence bundle | Implemented | `aw-server/detmir-readiness.service:48-59` and `aw-server/detmir-readiness.timer:60-71` generate daily readiness bundles. |
| Server deploy and API startup validation | Implemented | `ansible/deploy_aw_server.yml:2365-2380` restarts `aw-server/activitywatch-server.service` and waits for `/api/0/info`. |
| ActivityWatch DB merge backup | Partially implemented | `ansible/deploy_aw_server.yml:2222-2350` checks/install merge binary, backs up target and legacy DBs, merges, and installs merged DB when enabled. |
| Rust restore planner | Partially implemented | `scripts/prod_backup_restore.sh:14-29` requires the Rust planner; `adk-rust/crates/prod-backup-restore/src/main.rs:100-105` explicitly rejects `--apply`. |
| Windows package rollback backup | Implemented | `windows/ActivityWatch.Windows.Common.psm1:148-247` backs up install root before replacement and cleans old install backups. |
| Windows recovery loop | Implemented | `windows/ActivityWatch.Windows.Common.psm1:1576-1597` writes recovery script; `windows/ActivityWatch.Windows.Common.psm1:2114-2167` runs the recovery loop. |
| Windows recovery scheduled task | Implemented | `windows/ActivityWatch.Windows.Common.psm1:2447-2495` registers `ActivityWatch Recovery`; `windows/ActivityWatch.Windows.Common.psm1:2656-2672` starts launch tasks and recovery task. |
| Windows deployment validation | Implemented | `ansible/deploy_aw_windows.yml:501-545` checks ActivityWatch API buckets; `ansible/deploy_aw_windows.yml:547-565` runs and fetches endpoint validation. |
| Windows post-deploy validation | Implemented | `ansible/post_validate_aw_windows.yml:55-120` starts recovery/launch tasks and waits for fresh worktime events. |
| Gitea backup | Manual / partially implemented | `docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md:7-23` documents path, script, timer, format, checksum, retention, and restore-tested status. |
| Gitea restore proof | Missing | `docs/registry/registry-evidence-manifest.json:26-40` records `restore_tested=false` and `production_ready=false`. |
| ClickHouse 1C runtime store | Partially implemented | `clickhouse-1c/docker-compose.yml:1-19` defines the service and persistent Docker volume; no restore automation is present in the repository. |
| ClickHouse Workforce runtime store | Partially implemented | `clickhouse-workforce/docker-compose.yml:1-17` defines the service and persistent Docker volume; no restore automation is present in the repository. |
| Grafana/Prometheus runtime store | Partially implemented | `grafana-1c/docker-compose.yml:36-79` defines Prometheus/Grafana volumes and Prometheus retention; no Grafana data restore automation is present in the repository. |
| Retention boundaries | Implemented for documented cleanup scopes | `docs/RETENTION_POLICY_RU.md:65-102` documents retention and recovery impact for persistent stores. |

## End-to-End Recovery Chain

| Step | Status | Current evidence | Proof conclusion |
| --- | --- | --- | --- |
| Repository | Partially implemented | Primary self-hosted Gitea repository is recorded in `docs/registry/registry-evidence-manifest.json:6-10`; backup configuration is recorded in `docs/registry/registry-evidence-manifest.json:26-40`. | Source repository is identified and backup metadata exists, but restore proof is missing. |
| Configuration | Manual | Git stores sanitized templates and explicitly excludes secrets/live DBs in `docs/BACKUP_AND_RECOVERY_RU.md:29-46`. | Configuration can be reconstructed from repository templates plus customer secret store, but secrets are intentionally external. |
| Deployment | Implemented | Server deployment restarts ActivityWatch and waits for API readiness in `ansible/deploy_aw_server.yml:2365-2380`; Windows deployment runs package install, recovery tasks, and smoke checks in `ansible/deploy_aw_windows.yml:279-565`. | Server and Windows deployment are executable from repository playbooks. |
| Restore | Partially implemented | AW DB merge path exists in `ansible/deploy_aw_server.yml:2222-2350`; Rust planner emits a plan but rejects `--apply` in `adk-rust/crates/prod-backup-restore/src/main.rs:100-105`; Gitea restore is manual and untested per `docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md:42-100`. | Restore is only partially proven. Several runtime stores require manual or future automation. |
| Startup | Implemented | `aw-server/activitywatch-server.service` has restart policy in `aw-server/activitywatch-server.service:6-14`; Windows recovery task and loop are implemented in `windows/ActivityWatch.Windows.Common.psm1:2114-2167` and `:2447-2495`. | Repository supports service startup and collector recovery startup. |
| Health | Implemented | `/healthz`, `/readyz`, `/metrics` are documented in `docs/OPERATIONS_RUNBOOK_RU.md:5-47`; server deployment waits for `/api/0/info` in `ansible/deploy_aw_server.yml:2372-2380`. | Health checks are present and part of deploy/recovery validation. |
| Operational validation | Implemented | Operational gates are documented in `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md:49-59`; production smoke is documented in `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md:95-119`. | Repository has repeatable operational validation commands. |
| Pilot validation | Implemented | `docs/OPERATIONS_RUNBOOK_RU.md:49-64` lists pilot/demo/deployment smoke scripts after recovery; `scripts/pilot-validation-smoke.mjs` and `scripts/deployment-readiness-smoke.mjs` exist. | Pilot/deployment smoke validation is implemented as repository scripts. |
| Ready | Manual / conditional | General recovery procedure requires result recording in `docs/BACKUP_AND_RECOVERY_RU.md:59-71`; Gitea manifest still says `production_ready=false` in `docs/registry/registry-evidence-manifest.json:37-40`. | Ready can only be claimed after external restore evidence is captured and manifest gaps are closed. |

## Current Recovery Capability

### Server

Implemented:

- ActivityWatch server restart and API readiness wait through Ansible.
- Systemd restart-on-failure for `aw-server/activitywatch-server.service`.
- Guarded SQLite maintenance and daily readiness bundle timers.
- Limited AW DB merge/recovery-like flow with pre-merge backups.

Partially implemented:

- Generic production restore planning through `prod-backup-restore`, because the
  Rust binary builds a plan but rejects apply mode.
- ActivityWatch DB restore, because the repository implements legacy DB merge,
  but not a generic restore-selected-backup command.

Missing:

- Evidence of a completed end-to-end AW DB restore drill.
- Automated rollback from a failed DB merge to the pre-merge backup.

### Windows/RDP

Implemented:

- Package replacement with install-root backup.
- Recovery script generation.
- Scheduled `ActivityWatch Recovery` task.
- Long-running recovery loop that restarts collectors/tasks for live sessions.
- Post-deploy validation that waits for fresh ActivityWatch worktime events.

Partially implemented:

- Full Windows state restore, because install backups exist but
  `C:\ProgramData\AWatch-rus` restore as a whole is not implemented.

### Repository/Gitea

Manual / partially implemented:

- Backup target, script name, systemd unit/timer names, format, checksum, and
  retention are documented.
- Restore procedure is documented as an outline for a separate server.

Missing:

- Actual separate-host restore evidence.
- Manifest update proving `restore_tested=true`.
- Offsite copy evidence.

### ClickHouse, Grafana, Prometheus

Partially implemented:

- Docker Compose files define persistent volumes and restart policies.
- Prometheus retention is configurable through compose.
- Retention policy documents recovery impact for ClickHouse, Grafana, and
  Prometheus stores.

Missing:

- Automated backup and restore for ClickHouse 1C volume.
- Automated backup and restore for ClickHouse Workforce volume.
- Automated Grafana data restore.
- End-to-end restore evidence for these stores.

## Current Limitations

1. Gitea backup cannot be called production-ready until a separate-host restore
   is performed and recorded. Evidence: `docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md:3-5`,
   `docs/registry/registry-evidence-manifest.json:37-40`.
2. `prod-backup-restore --apply` is unsupported by code. Evidence:
   `adk-rust/crates/prod-backup-restore/src/main.rs:100-105`.
3. ActivityWatch DB recovery is limited to legacy-root merge and does not prove
   generic restore from a selected backup file. Evidence:
   `ansible/deploy_aw_server.yml:2222-2350`.
4. ClickHouse/Grafana/Prometheus data volumes are declared, but repository
   restore automation is not present. Evidence:
   `clickhouse-1c/docker-compose.yml:13-19`,
   `clickhouse-workforce/docker-compose.yml:11-17`,
   `grafana-1c/docker-compose.yml:36-79`.
5. Customer secrets are intentionally outside Git. Recovery therefore requires
   access to the customer secret store and cannot be proven from repository
   contents alone. Evidence: `docs/BACKUP_AND_RECOVERY_RU.md:29-46`.
6. Evidence/customer data must not be placed in the public repository. Evidence:
   `docs/BACKUP_AND_RECOVERY_RU.md:91-98`.

## Estimated Recovery Sequence

This sequence is the current evidence-backed chain. Steps marked manual require
operator execution and external evidence capture.

1. Repository availability: clone the primary Gitea repository or validated
   mirror. Status: partially implemented. Evidence:
   `docs/registry/registry-evidence-manifest.json:6-10`.
2. Repository restore, if primary Gitea is lost: follow the Gitea restore
   outline on a separate host, verify checksum, run Gitea checks, and record
   evidence. Status: manual / missing proof. Evidence:
   `docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md:42-100`.
3. Configuration recovery: restore sanitized templates from Git and secrets
   from the approved customer secret store. Status: manual. Evidence:
   `docs/BACKUP_AND_RECOVERY_RU.md:29-46`.
4. Server deployment: run server deployment playbook and wait for
   `/api/0/info`. Status: implemented. Evidence:
   `ansible/deploy_aw_server.yml:2365-2380`.
5. AW DB merge path, only when legacy-root merge is explicitly enabled: back up
   target and legacy DBs, merge, install merged DB. Status: partially
   implemented. Evidence: `ansible/deploy_aw_server.yml:2222-2350`.
6. Windows/RDP deployment: run Windows deployment playbook, deploy package,
   configure recovery tasks, and validate bucket events. Status: implemented.
   Evidence: `ansible/deploy_aw_windows.yml:279-565`.
7. Windows post-restore validation: start recovery/launch tasks and wait for
   worktime events. Status: implemented. Evidence:
   `ansible/post_validate_aw_windows.yml:55-120`.
8. Service startup validation: check systemd status, `/healthz`, `/readyz`,
   `/metrics`, and `/api/0/info`. Status: implemented. Evidence:
   `docs/OPERATIONS_RUNBOOK_RU.md:5-47`,
   `ansible/deploy_aw_server.yml:2372-2380`.
9. Production smoke: run `check-aw-full.sh`, `check-aw-data.sh`, and contour
   smoke with stable `SHARKON2025` logical host id. Status: implemented.
   Evidence: `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md:95-119`.
10. Operational maturity validation: run secret scan, operational maturity
    check, and quality gate. Status: implemented. Evidence:
    `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md:49-59`.
11. Pilot/deployment readiness validation: run pilot and deployment smoke
    scripts after recovery. Status: implemented. Evidence:
    `docs/OPERATIONS_RUNBOOK_RU.md:49-64`.
12. Ready decision: record recovery result and do not claim production-ready DR
    until missing restore evidence is closed. Status: manual / conditional.
    Evidence: `docs/BACKUP_AND_RECOVERY_RU.md:59-71`,
    `docs/registry/registry-evidence-manifest.json:37-40`.

## Evidence Gaps

| Gap | Status | Required evidence before closing |
| --- | --- | --- |
| Gitea separate-host restore | Missing | Backup filename, SHA256 verification output, Gitea version, restore duration, post-restore checks, clone/access proof, and manifest update. |
| Generic AW DB restore | Missing | Tested command or runbook restoring a selected backup into active DB with checksum and rollback evidence. |
| ClickHouse 1C restore | Missing | Tested volume/table backup and restore evidence for `clickhouse_1c_data`. |
| ClickHouse Workforce restore | Missing | Tested volume/table backup and restore evidence for `clickhouse_workforce_data`. |
| Grafana data restore | Missing | Tested restore evidence for `grafana-data` or explicit proof that provisioned dashboards plus documented credentials are sufficient. |
| Off-host backup copy | Missing | Repository evidence of destination, retention, checksum policy, access policy, and restore test. |

## Recommended Future Automation

These are future tasks, not implemented by this document:

1. Add a non-destructive restore drill checklist artifact that records exact
   backup filename, checksum, host, duration, and post-restore checks.
2. Add a separate-host Gitea restore evidence template and only then update
   `restore_tested` in `docs/registry/registry-evidence-manifest.json`.
3. Add tested backup/restore automation for ClickHouse 1C and Workforce volumes
   or explicitly document that those stores are rebuilt from source exports.
4. Add a generic ActivityWatch DB restore runbook with rollback command and
   checksum verification.
5. Add a single post-restore validation command that runs health, operational
   maturity, pilot validation, and deployment readiness in the documented order.

## Release Decision

DR proof is sufficient to show that the repository contains significant
recoverability mechanisms and validation gates.

DR proof is not sufficient to claim full Production 1.0 disaster recovery until
the missing separate-host restore evidence and runtime store restore gaps are
closed.
