# TASK_065: Production readiness report

Audit date: 2026-07-01.

Scope: repository-level engineering and operational audit for AWatch-rus
Production 1.0 readiness.

Production context: the project is already deployed in a small company
environment with approximately 5 RDP users. Stability, rollback, diagnostics
and maintainability have priority over new functionality.

## Method

The audit used repository evidence only. No readiness claim in this report is
based on an unverified assumption.

Reviewed evidence:

- `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, `ROADMAP.md`.
- `docs/PROJECT_STATUS_RU.md`, `docs/RESIDUAL_RISKS_RU.md`,
  `docs/QUALITY_STATUS_RU.md`, `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`,
  `docs/DLP_OPTIONAL_RUNTIME_RU.md`, `docs/DLP_RESOURCE_PROFILES_RU.md`,
  `docs/POWERSHELL_SCRIPT_STATUS_MATRIX_RU.md`.
- `docs/registry/*` including `registry-evidence-manifest.json`.
- `.github/workflows/*`.
- `scripts/*`, `scripts/detmir-full-diagnostics/*`.
- `adk-rust/` workspace manifests and crate tree.
- `ansible/`, `aw-server/`, `windows/`, `proxmox/`, `clickhouse-1c/`,
  `grafana/`, `pfsense/`.
- `configs/operational-maturity-contract.json`.

Commands used for evidence gathering:

```bash
git ls-files | wc -l
find adk-rust/crates -maxdepth 2 -name Cargo.toml
cargo metadata --locked --format-version 1
cargo audit --deny warnings
cargo machete --with-metadata
cargo tree --duplicates --locked
cargo deny check --config ../deny.toml --hide-inclusion-graph --show-stats
wc -l adk-rust/crates/detmir-portal/src/main.rs adk-rust/crates/aw-windows-telemetry/src/main.rs proxmox/tsj_guardian_bot.py adk-rust/crates/worktime-api/src/main.rs ansible/deploy_aw_server.yml
sha256sum scripts/aw-contour-diag.sh scripts/detmir-full-diagnostics/aw-contour-diag.sh scripts/check_production_inventory_placeholders.sh scripts/detmir-full-diagnostics/check_production_inventory_placeholders.sh
rg -n "CLICKHOUSE_PASSWORD|--password|password" clickhouse-1c/ops clickhouse-1c/ai clickhouse-1c/etl
```

## Implemented Capability Inventory

### Core Runtime

- Rust workspace under `adk-rust/` with 58 crates.
- ActivityWatch checks and wrappers: `check-aw-data`, `check-aw-full`,
  `aw-health-check`, `aw-rus-healthd`, `aw-slo-monitor`.
- DetMir portal: Rust HTML/API portal with contracts, metrics, readiness,
  reports, operator/manager/security views and role-based access logic.
- Worktime stack: `worktime-api`, `worktime-prewarm`,
  `worktime-influx-exporter`, `worktime-autoheal`,
  `worktime-ui-bridge`, `rdp-worktime-report`.
- Windows telemetry: `aw-windows-telemetry` for collectors, validation,
  file-1C upload, DLP evidence sync and collector guard paths.
- Release/install tooling: `rebuild-install-kit`, `validate-install-kit`,
  `verify-innosetup-installer`, `check-install-kit-vs-repo`.

### Workforce / 1C / ClickHouse

- `aw-workforce-ingest` and `aw-1c-ingest` exist for workforce and 1C
  ingestion paths.
- `clickhouse-1c/` includes ETL, SQL, Grafana provisioning and operational
  wrappers.
- Grafana dashboards are version-controlled under `grafana/` and related
  ClickHouse/Grafana directories.

### Security / DLP / Forensics

- DLP server-side helpers exist as Rust crates:
  `dlp-policy-engine`, `dlp-case-management`, `dlp-compliance`,
  `dlp-aggregator`, `dlp-health-check`, `dlp-content-analyzer`,
  exporters/senders.
- DLP production runtime is intentionally conservative:
  `core_only/disabled` default with documented `light` profile and load guard.
- Security Finding Inbox and Hayabusa/Velociraptor findings paths are optional
  and separated from Workforce hot path.
- Hayabusa tooling exists under `hayabusa-tools` and `aw-server/hayabusa/`.

### Deployment / Operations

- Ansible deployment exists for server and Windows contours.
- Windows deployment and recovery scripts exist under `windows/`.
- Proxmox/pfSense support assets exist under `proxmox/` and `pfsense/`.
- Operational wrappers exist at root and under `scripts/`.
- `scripts/operational-maturity-check.mjs` validates API compatibility,
  fixtures, fault injection, bounded load, config, systemd, ClickHouse
  migration and observability contracts.

### Governance / Release

- Public CI workflows exist.
- Security workflow includes cargo audit, cargo deny, secret pattern scan and
  dependency review.
- Dependency hygiene workflow includes cargo metadata, machete, duplicates,
  audit, deny and advisory udeps.
- Registry-readiness documentation exists under `docs/registry/`.
- CODEOWNERS, PR template, review checklist and branch protection evidence docs
  exist.

## Confirmed Gap Analysis

### Technical Debt

- Large modules increase review risk:
  - `adk-rust/crates/detmir-portal/src/main.rs`: 14200 lines.
  - `adk-rust/crates/aw-windows-telemetry/src/main.rs`: 6411 lines.
  - `proxmox/tsj_guardian_bot.py`: 4610 lines.
  - `adk-rust/crates/worktime-api/src/main.rs`: 3988 lines.
  - `ansible/deploy_aw_server.yml`: 3099 lines.
- PowerShell fallback remains necessary and documented; it is not dead code, but
  it increases parity and validation burden.

### Duplicated Logic

- Exact duplicate scripts confirmed by SHA256:
  - `scripts/aw-contour-diag.sh`
  - `scripts/detmir-full-diagnostics/aw-contour-diag.sh`
  - `scripts/check_production_inventory_placeholders.sh`
  - `scripts/detmir-full-diagnostics/check_production_inventory_placeholders.sh`

### Outdated Or Pending Documentation

- `docs/PROJECT_STATUS_RU.md` still records first reviewed PR evidence as
  pending and contains historical required check names.
- `docs/RESIDUAL_RISKS_RU.md` records Gitea restore test, build-runner,
  release evidence and legal package as open.
- `ROADMAP.md` records coverage threshold and Russian OS compatibility as
  planned, not complete.

### Obsolete Or Deprecated Components

- Some Ansible DLP roles are explicitly marked deprecated because they deployed
  old service paths.
- `serde_yaml 0.9.34+deprecated` is documented as a medium third-party risk.
- Legacy scripts remain as fallback/reference; they should not be removed
  without parity gates.

### Missing Operational Checks

- No confirmed repository check currently proves production binary SHA parity
  across all actually running units/timers/tasks and local release artifacts.
- Retention/cleanup policy for long-lived state/evidence/diagnostic output is
  not yet complete.
- Existing operational maturity bounded load is useful, but does not yet cover
  Production 1.0 scale scenarios for 5/20/50 users and portal/worktime prewarm.

### Missing Tests

- Load regression tests for portal/worktime full report and prewarm hot paths
  are not yet sufficient for Production 1.0 scale confidence.
- Windows Rust validation parity still needs canary evidence against the
  PowerShell validation path.

### Security Gaps

- ClickHouse/1C ops wrappers pass `CLICKHOUSE_PASSWORD` via `--password`, which
  exposes secrets in process argv.
- `cargo deny` passes but currently permits non-blocking duplicate/wildcard
  dependency warnings. This is not an immediate vulnerability, but it needs a
  Production 1.0 baseline.

## Readiness Scores

Scoring scale:

- 90-100: production-ready with evidence.
- 75-89: strong, but with bounded gaps.
- 60-74: usable in current production, but not yet 1.0 release-grade.
- below 60: material blocker.

| Category | Score | Justification |
|---|---:|---|
| Architecture status | 82 | Rust-first runtime, documented boundaries and conservative DLP separation exist. Large modules remain maintainability risk. |
| Repository health | 80 | 910 tracked files, clear ownership areas and runbooks. Some duplicate scripts and historical docs remain. |
| Dependency health | 78 | Audit and machete pass; 349 packages; cargo deny exits 0. Remaining deny warnings and duplicate roots need baseline policy. |
| CI health | 82 | CI/security/coverage/dependency/operational workflows exist. Toolchain drift remains between pinned `1.94.0` and floating `stable`. |
| Operational maturity | 84 | Offline operational maturity harness covers compatibility, fixtures, fault injection, bounded load, config and observability. Production-scale load gate still needed. |
| Documentation status | 78 | Extensive docs and runbooks exist. Current-state docs need cleanup around historical statuses and pending evidence. |
| Security status | 76 | Secret scan, audit, deny and conservative claims exist. ClickHouse password-in-argv is a direct hygiene gap. |
| Testing status | 80 | Full Rust pipeline recently passed and operational smokes exist. Missing scale/load and Windows parity evidence remain. |
| Deployment readiness | 76 | Ansible, Windows install kit, runbooks and wrappers exist. Install kit stale-payload gate and production binary parity still missing. |
| Upgrade readiness | 72 | Release scripts and rollback docs exist, but controlled release evidence and binary parity are not proven. |
| Recovery readiness | 70 | Recovery runbooks and backups exist; Gitea restore test is not done. |
| Configuration validation | 82 | Operational maturity validates JSON/YAML/systemd/ClickHouse files; production inventory placeholder checks exist. Coverage must be extended to retention/binary parity. |
| Observability | 84 | Metrics contract and operational maturity observability checks exist; capacity metrics need scale scenarios. |
| Support readiness | 78 | Many runbooks exist and DetMir guardrails are strong; docs need current-state cleanup for 1.0. |
| Maintainability | 72 | Strong tests and Rust-first direction, but large files and fallback parity increase maintenance cost. |

Overall Production Readiness Score: **78 / 100**.

Recommended release decision: **not yet Production 1.0**. The project is fit
for the current small production/pilot environment with conservative runtime
guardrails, but Production 1.0 should wait until P0 blockers in
`DEVELOPMENT_PLAN_NEXT.md` are closed and evidenced.

## Risk Assessment

### High Risk

1. Production binary drift
   - Probability: medium.
   - Impact: high.
   - Description: running binaries may not match reviewed release artifacts.
   - Mitigation: implement production binary parity gate.
   - Complexity: medium.

2. Missing controlled release evidence
   - Probability: high.
   - Impact: high.
   - Description: GitHub mirror validation is not release evidence.
   - Mitigation: controlled runner release evidence build.
   - Complexity: medium.

3. Untested repository restore
   - Probability: medium.
   - Impact: high.
   - Description: backup exists, but restore is not proven.
   - Mitigation: separate-host restore drill.
   - Complexity: medium.

4. Unbounded operational artifacts
   - Probability: medium.
   - Impact: high.
   - Description: state/evidence/diagnostic artifacts can accumulate and fill
     disks.
   - Mitigation: allowlisted retention and cleanup policy.
   - Complexity: medium.

5. ClickHouse password in process argv
   - Probability: high on affected scripts.
   - Impact: high.
   - Description: local process listing can expose credentials.
   - Mitigation: remove password from argv and verify with `ps`.
   - Complexity: low-medium.

6. Portal/worktime hot-path overload
   - Probability: medium.
   - Impact: high.
   - Description: full report/snapshot prewarm remains documented as CPU/IO
     expensive.
   - Mitigation: Production 1.0 load gate with synthetic datasets.
   - Complexity: medium-high.

### Medium Risk

1. CI toolchain drift
   - Probability: medium.
   - Impact: medium.
   - Mitigation: align workflows to pinned toolchain.
   - Complexity: low.

2. Dependency warning drift
   - Probability: medium.
   - Impact: medium.
   - Mitigation: baseline current warnings and block new unapproved warnings.
   - Complexity: medium.

3. Windows validation parity gap
   - Probability: medium.
   - Impact: medium-high.
   - Mitigation: Rust/PowerShell canary comparison.
   - Complexity: medium.

4. Install kit stale payload
   - Probability: medium.
   - Impact: medium-high.
   - Mitigation: install kit manifest and validation gate.
   - Complexity: medium.

5. Documentation drift
   - Probability: high.
   - Impact: medium.
   - Mitigation: current-state cleanup and historical labeling.
   - Complexity: low-medium.

6. Large module maintainability
   - Probability: high.
   - Impact: medium.
   - Mitigation: incremental extraction with tests.
   - Complexity: medium-high.

### Low Risk

1. Exact duplicate diagnostic scripts
   - Probability: medium.
   - Impact: low-medium.
   - Mitigation: canonical implementation or drift check.
   - Complexity: low.

2. Coverage threshold not yet enforced
   - Probability: medium.
   - Impact: low for current production, medium long-term.
   - Mitigation: baseline review, advisory threshold, later blocking policy.
   - Complexity: medium.

3. Russian OS compatibility not yet matrixed
   - Probability: low for current DetMir, medium for wider distribution.
   - Impact: medium for new deployments.
   - Mitigation: compatibility matrix with evidence.
   - Complexity: medium-high.

## Known Limitations

- The audit did not claim legal readiness, certification or registry
  submission completion.
- The audit did not validate live production services during this documentation
  update.
- Heavy DLP, Loki and always-on Velociraptor are intentionally not required for
  Production 1.0.
- PowerShell fallback remains a supported rollback/support path until parity
  evidence allows retirement.

## Release Readiness Summary

Production 1.0 should be blocked on:

1. production binary parity evidence;
2. controlled release evidence build;
3. Gitea restore drill;
4. bounded retention/cleanup;
5. ClickHouse password argv fix;
6. portal/worktime hot-path load gate.

After these are complete, the project can reasonably move from current small
production/pilot readiness to Production 1.0 readiness, assuming validation
passes and no new runtime regressions are introduced.
