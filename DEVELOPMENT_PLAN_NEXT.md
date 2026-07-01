# TASK_065: Production 1.0 development plan

Audit date: 2026-07-01.

Scope: AWatch-rus repository readiness for Production 1.0. This plan is based
only on repository evidence: tracked files, documentation, CI workflows,
Cargo metadata, dependency checks, scripts, deployment assets and operational
maturity contracts.

This is not a feature roadmap. Every task below exists because the current
repository still has a confirmed blocker or material risk for a durable
Production 1.0 release.

## Guardrails

- Do not redesign working subsystems.
- Do not add new product functionality as part of Production 1.0 readiness.
- Preserve backward compatibility for deployed DetMir production.
- Keep DLP runtime conservative: `core_only/disabled` by default; `light` only
  after explicit operator resource preflight.
- Do not enable Loki or always-on Velociraptor as part of Production 1.0.
- Keep PowerShell fallback until Rust parity, canary evidence and rollback
  criteria are proven.
- GitHub remains public mirror validation. Release evidence must be produced in
  the controlled release contour documented under `docs/registry/`.

## Evidence Base

Reviewed repository areas:

- Rust workspace: `adk-rust/`, 58 crates from `adk-rust/crates/*/Cargo.toml`.
- Cargo state: `cargo metadata --locked --format-version 1` returned 58
  workspace members and 349 packages.
- Dependency hygiene: `cargo audit --deny warnings` passed; `cargo machete
  --with-metadata` found no unused dependencies; `cargo deny` passed with
  non-blocking policy warnings; `cargo tree --duplicates --locked` found
  duplicate roots including `bitflags`, `getrandom`, `hashbrown`, `mio`, `zip`.
- CI: `.github/workflows/ci.yml`, `security.yml`, `coverage.yml`,
  `dependency-hygiene.yml`, `operational-maturity.yml`,
  `rust-workspace.yml`, `rust-binary-build.yml`, `release-assets.yml`.
- Operational maturity: `configs/operational-maturity-contract.json`,
  `scripts/operational-maturity-check.mjs`,
  `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`.
- Registry and release evidence docs: `docs/registry/*`,
  `docs/PROJECT_STATUS_RU.md`, `docs/RESIDUAL_RISKS_RU.md`,
  `docs/QUALITY_STATUS_RU.md`, `ROADMAP.md`.
- Runtime/deployment: `ansible/`, `aw-server/`, `windows/`, `proxmox/`,
  `clickhouse-1c/`, `grafana/`, `pfsense/`, root operational wrappers.

Confirmed current strengths:

- Rust-first runtime and wrappers exist.
- Public mirror CI exists.
- Dependency hygiene automation exists.
- Operational maturity offline harness exists.
- DLP resource guardrails and conservative runtime profile exist.
- Security Finding Inbox / Hayabusa / Velociraptor are optional and separated
  from Workforce hot path.
- Production runbooks and DetMir guardrails are documented.

Confirmed Production 1.0 blockers and material risks are organized below.

## P0 - Critical before Production 1.0

### P0-1. Production binary parity gate

Purpose: prove that every binary actually running in production matches the
reviewed release artifact.

Reason: `scripts/check_detmir_rust_release_artifacts.sh` verifies local release
artifacts, but Production 1.0 also needs a verified mapping from deployed unit,
timer or Windows task to exact production SHA256 and source crate.

Operational impact: prevents stale binary drift and makes rollback decisions
deterministic.

Risk: high. A stale production binary can invalidate test results and hide
regressions.

Estimated effort: 3-5 days.

Affected modules:

- `scripts/check_detmir_rust_release_artifacts.sh`
- `scripts/package_rust_release_binaries.py`
- `scripts/detmir-full-diagnostics/`
- `adk-rust/crates/detmir-readiness/`
- `adk-rust/crates/aw-windows-telemetry/`
- `windows/validate-deployment.ps1`
- `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`

Acceptance criteria:

- Report maps `service/timer/task -> binary path -> crate -> runtime role`.
- Report includes local release SHA256, production SHA256 and git SHA.
- Missing or mismatched binaries fail the gate.
- DLP/Loki/Velociraptor heavy runtime is not enabled by the check.

Validation steps:

- Run the parity gate against current release artifacts.
- Run diagnostics for `10.10.10.2`, `10.10.10.13` and Windows RDP host.
- Confirm stale binary simulation fails.

Expected benefit: release and production state become auditable.

### P0-2. Controlled release evidence build

Purpose: produce Production 1.0 release evidence outside GitHub public mirror.

Reason: `docs/PROJECT_STATUS_RU.md`, `docs/QUALITY_STATUS_RU.md` and
`docs/registry/registry-evidence-manifest.json` state that GitHub Actions are
public validation only and that the Russian build-runner/release evidence path
is still pending.

Operational impact: separates mirror CI from release authority.

Risk: high. Without controlled release evidence, Production 1.0 cannot be
treated as reproducibly built.

Estimated effort: 3-6 days plus infrastructure window.

Affected modules:

- `docs/registry/RU_BUILD_RUNNER_READINESS_RU.md`
- `docs/registry/BUILD_RUNNER_SETUP_RUNBOOK_RU.md`
- `docs/registry/RELEASE_EVIDENCE_RUNBOOK_RU.md`
- `docs/registry/RELEASE_EVIDENCE_MANIFEST_RU.md`
- `scripts/build_release_evidence.sh`
- `scripts/check_release_evidence.sh`
- `scripts/verify_release_assets.sh`

Acceptance criteria:

- Controlled runner has documented OS, toolchain and access model.
- Release evidence includes source archive, binary archive, SBOM if available,
  SHA256SUMS, Cargo metadata/tree, smoke logs and release manifest.
- `scripts/check_release_evidence.sh` passes on produced artifacts.
- GitHub CI is not described as release evidence.

Validation steps:

- Run release evidence script on controlled runner.
- Verify manifest and checksums.
- Record runner environment and commit SHA.

Expected benefit: Production 1.0 release becomes reproducible and auditable.

### P0-3. Disaster recovery restore proof

Purpose: prove that repository backup can be restored before Production 1.0.

Reason: `docs/registry/registry-evidence-manifest.json` records
`restore_tested=false`, and `docs/RESIDUAL_RISKS_RU.md` lists Gitea restore
test as open.

Operational impact: validates recovery from source repository loss.

Risk: high. Untested backup is not a recovery capability.

Estimated effort: 2-4 days.

Affected modules:

- `docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md`
- `docs/registry/registry-evidence-manifest.json`
- `scripts/registry_readiness_check.sh`
- `docs/PROJECT_STATUS_RU.md`
- `docs/RESIDUAL_RISKS_RU.md`

Acceptance criteria:

- Restore is performed on a separate host.
- SHA256 verification, logs, restored repository access and rollback notes are
  captured.
- Manifest is updated only after evidence exists.
- No secrets are copied into repository documentation.

Validation steps:

- Execute restore runbook.
- Run registry readiness check after manifest update.
- Confirm restored repository clone and log evidence.

Expected benefit: repository DR becomes proven, not only documented.

### P0-4. Bounded retention for operational state and evidence

Purpose: prevent disk exhaustion from state, queues, diagnostics, evidence and
forensics artifacts.

Reason: DLP optional runtime docs explicitly state that historical DLP buckets
and artifacts may remain until a separate retention/cleanup procedure. Scripts
and diagnostics also create durable output.

Operational impact: reduces outage risk on Proxmox/AW server/Windows state
paths.

Risk: high. Disk exhaustion can stop ingestion, portal, ClickHouse or
ActivityWatch services.

Estimated effort: 4-7 days.

Affected modules:

- `adk-rust/crates/aw-prune-local-state/`
- `scripts/detmir-full-diagnostics/`
- `scripts/detmir_dlp_warehouse_sync.sh`
- `aw-server/logrotate.conf`
- `windows/validate-deployment.ps1`
- `adk-rust/crates/aw-windows-telemetry/`
- `docs/DLP_OPTIONAL_RUNTIME_RU.md`
- `docs/DLP_RESOURCE_PROFILES_RU.md`
- `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`

Acceptance criteria:

- Retention matrix lists roots, owners, max age, max size and dry-run behavior.
- Cleanup only touches allowlisted roots and refuses traversal/symlink escape.
- Active state is preserved.
- Disabled DLP buckets remain `SKIPPED`, not failure.

Validation steps:

- Run dry-run cleanup on fixture tree.
- Run apply mode on controlled temporary tree.
- Run operational smoke after cleanup.

Expected benefit: long-running production operation has bounded disk behavior.

### P0-5. Remove ClickHouse password exposure from process arguments

Status: addressed by TASK_068. Runtime ClickHouse/1C wrappers keep
`CLICKHOUSE_PASSWORD` in the environment/config path and no longer pass it in
process arguments.

Purpose: keep production credentials out of `ps`/process argv.

Reason: `rg` confirmed `clickhouse-1c/ops/run_*.sh` wrappers pass
`--password "${CLICKHOUSE_PASSWORD}"`.

Operational impact: improves secret handling for the 1C/ClickHouse contour.

Risk: high. Local process listing can reveal ClickHouse credentials.

Estimated effort: 2-4 days.

Affected modules:

- `clickhouse-1c/ops/run_ingest_cycle.sh`
- `clickhouse-1c/ops/run_manager_brief.sh`
- `clickhouse-1c/ops/run_recovery_brief.sh`
- `clickhouse-1c/ops/run_company_registry_bindings_refresh.sh`
- `clickhouse-1c/ops/run_company_intelligence_refresh.sh`
- `clickhouse-1c/ops/check_ingest_freshness.sh`
- `clickhouse-1c/ai/*.py`
- `clickhouse-1c/etl/*.py`
- `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`

Acceptance criteria:

- Runtime wrappers no longer pass password through argv.
- Existing environment-based deployment remains backward compatible.
- Logs redact authentication failures.
- `ps` smoke proves password absence.

Validation steps:

- `bash -n clickhouse-1c/ops/*.sh`
- Run affected wrapper against a test or dry-run configuration.
- Verify `ps` output during execution.
- Run secret-pattern scan.

Expected benefit: production secret exposure surface is reduced.

### P0-6. Portal/worktime hot-path load gate

Purpose: prevent Production 1.0 from regressing under report/prewarm load.

Reason: `docs/PROJECT_STATUS_RU.md` records that full report/snapshot prewarm
can still be CPU/IO expensive.

Operational impact: protects owner/operator portal, worktime reports and AW
query path.

Risk: high. More users or more history can produce slow portal, stale data or
AW datastore pressure.

Estimated effort: 1-2 weeks.

Affected modules:

- `scripts/operational-maturity-check.mjs`
- `scripts/awatch-production-hardening-smoke.mjs`
- `adk-rust/crates/detmir-portal/`
- `adk-rust/crates/worktime-api/`
- `adk-rust/crates/worktime-prewarm/`
- `adk-rust/crates/aw-contour-smoke/`
- `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`

Acceptance criteria:

- Synthetic 5/20/50 user fixtures exist without production data.
- Gate records p95 latency, max RSS, query count and stale-cache behavior.
- Disconnected RDP sessions do not false-fail.
- `AW_DLP_ENABLED=false` semantics remain valid.
- Heavy load job is scheduled/advisory; blocking smoke remains fast.

Validation steps:

- Run offline load harness.
- Confirm configured p95/RSS ceilings.
- Run existing operational maturity smoke.

Expected benefit: Production 1.0 has measurable performance safety.

## P1 - Strongly recommended for the first Production 1.0 release train

### P1-1. Align Rust toolchain across CI

Purpose: remove compiler drift from blocking workflows.

Reason: `rust-toolchain.toml` pins `1.94.0`, but several workflows install
floating `stable`.

Operational impact: improves reproducibility between local, CI and release
contours.

Risk: medium. Toolchain drift can create inconsistent warnings or binaries.

Estimated effort: 1-2 days.

Affected modules:

- `rust-toolchain.toml`
- `.github/workflows/ci.yml`
- `.github/workflows/security.yml`
- `.github/workflows/coverage.yml`
- `.github/workflows/dependency-hygiene.yml`
- `.github/workflows/release-assets.yml`
- `docs/QUALITY_STATUS_RU.md`

Acceptance criteria:

- Blocking Rust workflows use the pinned toolchain.
- Nightly remains limited to advisory `cargo udeps`.
- Required check names do not change.

Validation steps:

- YAML syntax validation.
- Affected workflow dry review.
- Run relevant Rust checks if workflow commands change.

Expected benefit: CI becomes more deterministic.

### P1-2. Dependency warning baseline and future block policy

Purpose: make dependency hygiene fail closed for new risk while preserving
current compatibility.

Reason: `cargo deny` passes but reports 36 non-blocking `bans` warnings;
`cargo tree --duplicates --locked` reports duplicate roots; `serde_yaml` is
documented as deprecated in third-party license docs.

Operational impact: reduces future supply-chain drift.

Risk: medium. Uncontrolled duplicate/deprecated dependency growth increases
maintenance and security load.

Estimated effort: 4-8 days.

Affected modules:

- `adk-rust/Cargo.toml`
- `adk-rust/Cargo.lock`
- `deny.toml`
- `.github/workflows/dependency-hygiene.yml`
- `docs/THIRD_PARTY_LICENSES_RU.md`
- `docs/QUALITY_STATUS_RU.md`

Acceptance criteria:

- Each existing warning is classified: keep, update, remove or defer.
- New duplicate/deprecated dependencies require documented exception.
- `cargo audit`, `cargo deny`, `cargo machete`, `cargo metadata` pass.

Validation steps:

- Run dependency hygiene pipeline.
- Verify policy failure on synthetic unapproved duplicate where practical.

Expected benefit: dependency hygiene remains controlled after 1.0.

### P1-3. Windows Rust validation parity

Purpose: prove Rust validation is equivalent to current PowerShell validation
before reducing fallback reliance.

Reason: `docs/POWERSHELL_SCRIPT_STATUS_MATRIX_RU.md` lists remaining fallback
and runtime PowerShell paths, including validation and Hayabusa upload.

Operational impact: keeps Windows/RDP production recoverable while reducing
runtime drift.

Risk: medium. Premature fallback removal can break localized Windows Server
2019 recovery paths.

Estimated effort: 1-2 weeks.

Affected modules:

- `adk-rust/crates/aw-windows-telemetry/`
- `windows/validate-deployment.ps1`
- `windows/ActivityWatch.Windows.Common.psm1`
- `windows/export-upload-hayabusa-to-aw-server.ps1`
- `ansible/deploy_aw_windows.yml`
- `docs/POWERSHELL_SCRIPT_STATUS_MATRIX_RU.md`
- `docs/POWERSHELL_TO_RUST_ROADMAP_RU.md`

Acceptance criteria:

- Rust validation covers all current production validation sections.
- Localized Windows user/session handling is tested.
- Canary comparison between Rust and PowerShell reports is recorded.
- PowerShell remains documented rollback.

Validation steps:

- Run Rust validation against fixture and live canary.
- Run PowerShell validation against same host.
- Compare normalized reports.

Expected benefit: Windows runtime maturity improves without breaking rollback.

### P1-4. Install kit reproducibility and stale payload gate

Purpose: ensure Windows installer payloads match repository and release
artifacts.

Reason: repository contains install-kit tooling and large installer artifacts;
stale payloads can deploy old collectors while CI is green.

Operational impact: safer Windows upgrades and rollback.

Risk: medium. Mismatched install kit can create production drift.

Estimated effort: 5-8 days.

Affected modules:

- `windows/installkit/innosetup/`
- `adk-rust/crates/check-install-kit-vs-repo/`
- `adk-rust/crates/rebuild-install-kit/`
- `adk-rust/crates/validate-install-kit/`
- `adk-rust/crates/verify-innosetup-installer/`
- `scripts/rebuild_install_kit.sh`
- `docs/INSTALL_KIT_RUNBOOK_RU.md`

Acceptance criteria:

- Installer manifest contains source commit and payload SHA256 values.
- Validation fails on stale collector payload.
- Existing Windows task names and config schema remain compatible.

Validation steps:

- Rebuild install kit on controlled runner.
- Run install-kit validators.
- Compare payload manifest with repository state.

Expected benefit: Windows deployments become reproducible.

### P1-5. Current-state documentation cleanup

Purpose: prevent operators and reviewers from following stale status text.

Reason: current docs include historical statuses, older required check names
and explicit pending sections that must be reconciled with active branch
protection and current workflows.

Operational impact: reduces release and support mistakes.

Risk: medium. Wrong runbook/status interpretation can cause incorrect release
decisions.

Estimated effort: 3-6 days.

Affected modules:

- `docs/PROJECT_STATUS_RU.md`
- `docs/QUALITY_STATUS_RU.md`
- `docs/ROADMAP_CONFORMANCE_AUDIT_RU.md`
- `docs/BRANCH_PROTECTION_POLICY_RU.md`
- `docs/BRANCH_PROTECTION_EVIDENCE_RU.md`
- `README.md`
- `ROADMAP.md`

Acceptance criteria:

- Current docs list actual required check names.
- Historical docs are clearly marked historical.
- Registry/release claims remain conservative.
- No stale instructions contradict Production 1.0 guardrails.

Validation steps:

- Run registry readiness check.
- Run docs smoke/link validation.
- Run secret scan.

Expected benefit: release process is less error-prone.

### P1-6. Reviewed PR and release governance evidence

Purpose: prove review discipline before Production 1.0.

Reason: `docs/RESIDUAL_RISKS_RU.md` records first reviewed PR evidence as
pending even though CODEOWNERS, PR template and ruleset are present.

Operational impact: improves change control for production releases.

Risk: medium. Lack of review evidence weakens release governance.

Estimated effort: 1-3 days after reviewer availability.

Affected modules:

- `.github/CODEOWNERS`
- `.github/pull_request_template.md`
- `docs/PR_REVIEW_WORKFLOW_RU.md`
- `docs/PR_REVIEW_EVIDENCE_RU.md`
- `docs/REVIEW_CHECKLIST_RU.md`
- `docs/RESIDUAL_RISKS_RU.md`

Acceptance criteria:

- At least one PR is reviewed and merged without bypass.
- Evidence records checks, reviewer, approval and merge path.
- Release branch review policy is documented.

Validation steps:

- Verify PR history and ruleset evidence.
- Run registry readiness check after evidence update.

Expected benefit: Production 1.0 has visible governance evidence.

## P2 - Engineering improvements for post-1.0 hardening

### P2-1. Incremental decomposition of large modules

Purpose: reduce review risk in the largest files without behavior changes.

Reason: confirmed hotspots include `detmir-portal/src/main.rs` at 14200 lines,
`aw-windows-telemetry/src/main.rs` at 6411 lines,
`proxmox/tsj_guardian_bot.py` at 4610 lines,
`worktime-api/src/main.rs` at 3988 lines and
`ansible/deploy_aw_server.yml` at 3099 lines.

Operational impact: easier reviews and lower regression risk.

Risk: medium. Large files increase accidental coupling.

Estimated effort: 2-4 weeks in small PRs.

Affected modules:

- `adk-rust/crates/detmir-portal/`
- `adk-rust/crates/aw-windows-telemetry/`
- `adk-rust/crates/worktime-api/`
- `proxmox/tsj_guardian_bot.py`
- `ansible/deploy_aw_server.yml`

Acceptance criteria:

- Only extract bounded domains.
- Public API, config, unit and task names remain unchanged.
- Tests before and after remain equivalent.

Validation steps:

- Targeted tests per extracted module.
- Full Rust pipeline for Rust changes.
- Ansible syntax/list-tasks parity for playbook changes.

Expected benefit: maintainability improves without architecture rewrite.

### P2-2. Consolidate exact duplicate diagnostic scripts

Purpose: prevent script drift.

Reason: SHA256 confirms exact duplicates:
`scripts/aw-contour-diag.sh` equals
`scripts/detmir-full-diagnostics/aw-contour-diag.sh`; and
`scripts/check_production_inventory_placeholders.sh` equals
`scripts/detmir-full-diagnostics/check_production_inventory_placeholders.sh`.

Operational impact: diagnostics remain consistent.

Risk: low-medium. Future fixes may land in one copy only.

Estimated effort: 1-2 days.

Affected modules:

- `scripts/aw-contour-diag.sh`
- `scripts/detmir-full-diagnostics/aw-contour-diag.sh`
- `scripts/check_production_inventory_placeholders.sh`
- `scripts/detmir-full-diagnostics/check_production_inventory_placeholders.sh`
- `scripts/detmir-full-diagnostics/detmir-full-diagnostics.sh`

Acceptance criteria:

- Existing paths continue to work.
- One implementation is canonical or drift check is enforced.
- Shell syntax and shellcheck pass.

Validation steps:

- `bash -n` on affected scripts.
- Run diagnostic wrapper in dry-run/smoke mode.

Expected benefit: lower maintenance overhead.

### P2-3. Bound 1C ingest memory profile

Purpose: make 1C ingest safer for larger files.

Reason: `adk-rust/crates/aw-1c-ingest/src/main.rs` reads rows into `Vec` and
builds large JSON batches; acceptable now, but risky as export volume grows.

Operational impact: improves ClickHouse ingestion predictability.

Risk: medium. Large files can create memory spikes and long insert windows.

Estimated effort: 1-2 weeks.

Affected modules:

- `adk-rust/crates/aw-1c-ingest/src/main.rs`
- `clickhouse-1c/etl/config.yml`
- `clickhouse-1c/etl/config.example.yml`
- `clickhouse-1c/sql/`

Acceptance criteria:

- Oversized input fails closed with clear diagnostic.
- Batch size is bounded and configurable.
- Existing small DetMir files produce identical output.

Validation steps:

- Add synthetic large CSV/XLSX fixture.
- Run targeted Rust tests.
- Measure max RSS on fixture.

Expected benefit: safer scaling of 1C analytics.

### P2-4. Command execution boundary audit

Purpose: standardize shell/command execution safety.

Reason: `detmir-portal` already validates shell probe commands and tests
process-tree timeout cleanup, while other operational tools also execute
commands.

Operational impact: prevents future command injection or timeout regressions.

Risk: medium. Config-driven command execution must remain fail-closed.

Estimated effort: 4-7 days.

Affected modules:

- `adk-rust/crates/detmir-portal/src/main.rs`
- `adk-rust/crates/detmir-portal/src/production/limits.rs`
- `adk-rust/crates/aw-slo-monitor/src/main.rs`
- `adk-rust/crates/diag-and-manual-restart/src/main.rs`
- `adk-rust/crates/quality-gate/src/main.rs`

Acceptance criteria:

- Runtime command sources are classified.
- Config-driven commands reject shell control operators where applicable.
- Timeout tests cover child/grandchild cleanup.
- Logs do not expose secrets.

Validation steps:

- Targeted Rust tests.
- Clippy for affected crates.
- Secret scan.

Expected benefit: stronger fail-closed security posture.

## P3 - Long-term improvements after Production 1.0

### P3-1. Russian OS compatibility matrix

Purpose: document supported and unsupported target OS combinations.

Reason: `ROADMAP.md` lists Russian OS compatibility validation as planned.

Operational impact: reduces deployment surprises for new customers.

Risk: low-medium for current DetMir, higher for wider distribution.

Estimated effort: 2-4 weeks depending on test hosts.

Affected modules:

- `docs/registry/`
- `docs/INSTALLATION.md`
- `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`
- `ansible/`
- `windows/`
- `windows/installkit/`

Acceptance criteria:

- Matrix lists OS, role, test date, result and limitations.
- Unsupported combinations are explicit.
- No production defaults are changed just for claims.

Validation steps:

- Run install/deploy smoke per OS.
- Record evidence paths.

Expected benefit: clearer deployment support boundary.

### P3-2. Coverage threshold after baseline review

Purpose: prevent coverage decline after baseline stabilizes.

Reason: `docs/QUALITY_STATUS_RU.md` and `ROADMAP.md` state coverage threshold
is not enforced yet.

Operational impact: improves long-term regression resistance.

Risk: low for current production if kept advisory first.

Estimated effort: 1-2 weeks.

Affected modules:

- `.github/workflows/coverage.yml`
- `docs/QUALITY_STATUS_RU.md`
- `docs/REVIEW_CHECKLIST_RU.md`
- selected `adk-rust/` crates

Acceptance criteria:

- Initial threshold is based on measured baseline.
- Threshold starts advisory and becomes blocking only after stable history.
- Generated/fixture code exclusions are documented.

Validation steps:

- Run coverage workflow locally or in CI.
- Compare summary to baseline.

Expected benefit: gradual improvement in test discipline.

### P3-3. Capacity sizing guide from measured data

Purpose: provide measured sizing guidance beyond the current 5-user DetMir
deployment.

Reason: current production is small; future deployments need measured guidance
for AW SQLite, ClickHouse, Grafana, DLP light profile and Windows collector
load.

Operational impact: safer planning for larger deployments.

Risk: low for current production, medium for growth.

Estimated effort: 2-4 weeks after P0 load gate data exists.

Affected modules:

- `docs/SIZING_GUIDE_RU.md`
- `docs/DLP_RESOURCE_PROFILES_RU.md`
- `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`
- `grafana/`
- `scripts/operational-maturity-check.mjs`

Acceptance criteria:

- Profiles exist for 5, 20, 50 and 100 users.
- Optional DLP/Hayabusa/Velociraptor resource costs are explicit.
- Guidance is based on measured harness output.

Validation steps:

- Run synthetic capacity scenarios.
- Update sizing doc with observed p95/RSS/storage data.

Expected benefit: production planning becomes evidence-based.

## Not Proposed Because Already Implemented

- Rust-first runtime direction.
- Public mirror CI, security scan and dependency review.
- `cargo audit`, `cargo deny`, `cargo machete`, `cargo tree --duplicates` and
  advisory `cargo udeps` workflow coverage.
- DLP `core_only/disabled` production guardrails and load guard.
- Optional Security Finding Inbox / Hayabusa / Velociraptor separation from
  Workforce hot path.
- Operational maturity offline harness.
- Branch protection, CODEOWNERS, PR template and review checklist mechanisms.
- Unused dependency cleanup: current `cargo machete --with-metadata` reports no
  unused dependencies.
