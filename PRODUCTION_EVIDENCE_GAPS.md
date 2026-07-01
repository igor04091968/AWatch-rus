# Production Evidence Gaps

Date: 2026-07-01

Scope: final evidence still missing before Release Candidate readiness for the
current AWatch-rus / DetMir production deployment.

This document does not add functionality, redesign recovery, or define new
tests. It records only evidence gaps confirmed by existing repository backlog,
runbooks, audit reports, validation scripts, and registry-readiness documents.

## Summary

The repository already contains operational validation scripts, pilot/deployment
smoke scripts, retention policy, recovery audit, disaster recovery proof, and
release evidence tooling. The remaining gap is not lack of plans. The remaining
gap is live or controlled-environment evidence proving that the current
production deployment and release candidate can be recovered, traced, rebuilt,
and operated under expected load.

## Missing Evidence Items

| ID | Evidence gap | RC priority | Requires live production environment |
| --- | --- | --- | --- |
| PEG-001 | Production binary parity evidence | P0 | Yes |
| PEG-002 | Separate-host Gitea restore proof | P0 | Separate restore host; production backup input |
| PEG-003 | Runtime data restore proof for production stores | P0 | Yes or isolated restore clone |
| PEG-004 | First controlled release evidence package from `awatch-build-01` | P0 | Controlled build-runner, not production runtime |
| PEG-005 | Portal/worktime hot-path load evidence | P0 | Prefer production-like or live low-risk window |
| PEG-006 | Capacity baseline and headroom evidence | P1 | Yes |
| PEG-007 | Backup verification and off-host backup evidence | P1 | Yes |
| PEG-008 | Install kit reproducibility and stale payload evidence | P1 | Controlled build/staging environment |
| PEG-009 | Reviewed PR / release governance evidence | P1 | GitHub/Gitea governance environment |

## PEG-001: Production Binary Parity Evidence

Why it matters: Release Candidate confidence depends on proving that binaries
actually running in production match reviewed release artifacts. Otherwise test
results can refer to one binary while production runs another.

Current state: Missing. The backlog still lists `P0-1. Production binary parity
gate` as open. The existing local artifact check verifies release artifacts, but
the backlog says Production 1.0 also needs deployed unit/timer/task to
production SHA256 and source crate mapping.

Repository evidence:

- `DEVELOPMENT_PLAN_NEXT.md:65-105`
- `PRODUCTION_READINESS_REPORT.md:209-212`
- `scripts/check_detmir_rust_release_artifacts.sh`

How it could be verified: collect production binary paths from actual
systemd/timer/Windows scheduled task inventory, compute production SHA256,
compare with local release artifact SHA256, and record
`service/timer/task -> binary path -> crate -> runtime role -> production sha256
-> release sha256 -> git sha`.

Expected operational benefit: removes stale-binary ambiguity during incident
response, rollback, and RC approval.

Risk if left unverified: production may run stale or locally patched binaries
while release evidence and tests refer to different artifacts.

## PEG-002: Separate-Host Gitea Restore Proof

Why it matters: Source repository backup is not a recovery capability until a
restore has been proven on a separate host.

Current state: Missing. Registry evidence explicitly records
`restore_tested=false` and `production_ready=false`; the restore runbook states
that backup cannot be production-ready until a separate test restore is
completed.

Repository evidence:

- `DEVELOPMENT_PLAN_NEXT.md:154-188`
- `RECOVERY_AUDIT.md:250-263`
- `DISASTER_RECOVERY_PROOF.md`
- `docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md:42-100`
- `docs/registry/registry-evidence-manifest.json:26-40`

How it could be verified: execute the existing Gitea restore runbook on a
separate host, verify backup SHA256, run `gitea doctor check`, verify restored
repository access/clone, record backup filename, checksum, Gitea version,
restore duration, post-restore output, and rollback notes.

Expected operational benefit: proves the source repository can be recovered
after primary Git/Gitea loss.

Risk if left unverified: repository loss recovery remains assumed rather than
proven.

## PEG-003: Runtime Data Restore Proof for Production Stores

Why it matters: Service restart is not enough for disaster recovery if primary
runtime data stores cannot be restored after corruption or loss.

Current state: Missing / partially implemented. Recovery automation exists for
service restart and Windows collector restart. Generic restore evidence is still
missing for ActivityWatch active SQLite DB, ClickHouse 1C, ClickHouse Workforce,
Grafana data, Prometheus TSDB, DLP state/evidence, Hayabusa archive, Windows
state root, diagnostic bundles, and release evidence.

Repository evidence:

- `RECOVERY_AUDIT.md:272-281`
- `RECOVERY_AUDIT.md:379-428`
- `DISASTER_RECOVERY_PROOF.md`
- `docs/RETENTION_POLICY_RU.md:65-102`

How it could be verified: for each production store, use existing backup or
restore documentation where present, perform restore into an isolated target,
then run existing health and validation scripts. Where the repository says
restore is not currently implemented, record the gap rather than inventing a
procedure.

Expected operational benefit: identifies which production data can actually be
recovered and which data still depends on external/customer backup handling.

Risk if left unverified: an outage may be recoverable at the service level but
not at the data level.

## PEG-004: First Controlled Release Evidence Package from `awatch-build-01`

Why it matters: GitHub Actions are public validation only. RC release authority
requires a controlled build-runner evidence package with source archive, binary
archive, checksums, metadata, logs, and manifest.

Current state: Tooling exists and was strengthened by TASK_070, but the
registry-readiness documents still require the first real release evidence build
on the Russian build-runner. The build-runner status is planned, not proven
production-ready.

Repository evidence:

- `DEVELOPMENT_PLAN_NEXT.md:107-149`
- `docs/registry/RU_BUILD_RUNNER_READINESS_RU.md`
- `docs/registry/BUILD_RUNNER_SETUP_RUNBOOK_RU.md`
- `docs/registry/RELEASE_EVIDENCE_RUNBOOK_RU.md`
- `docs/registry/RELEASE_EVIDENCE_MANIFEST_RU.md`
- `docs/registry/registry-evidence-manifest.json:41-75`
- `scripts/build_release_evidence.sh`
- `scripts/check_release_evidence.sh`

How it could be verified: run `scripts/build_release_evidence.sh` on the
controlled runner for the RC commit, then run
`scripts/check_release_evidence.sh <evidence-dir>` and preserve the generated
manifest, logs, source archive, binary archive, SHA256SUMS, cargo metadata/tree,
and documented skips.

Expected operational benefit: provides auditable release provenance independent
of the public mirror.

Risk if left unverified: RC could be validated only by public mirror CI rather
than by the release authority expected by the registry-readiness contour.

## PEG-005: Portal/Worktime Hot-Path Load Evidence

Why it matters: Production 1.0 should not regress under report, prewarm,
ActivityWatch query, cache, and operator portal load.

Current state: Missing. The backlog still lists `P0-6. Portal/worktime hot-path
load gate`. Existing operational maturity checks include offline bounded-load
validation, but repository evidence still requires an explicit portal/worktime
hot-path gate before Production 1.0.

Repository evidence:

- `DEVELOPMENT_PLAN_NEXT.md:281-319`
- `scripts/operational-maturity-check.mjs`
- `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md:95-119`

How it could be verified: run the existing operational maturity bounded-load
harness and production smoke sequence against a production-like or controlled
live window, record p95 latency, memory growth, cache/stale behavior, AW query
duration, and failure semantics. Keep any heavy job advisory/scheduled rather
than blocking fast smoke.

Expected operational benefit: proves that the current 5-user production
deployment has headroom for normal operator/report usage.

Risk if left unverified: RC may pass functional checks while still being fragile
under repeated report/prewarm/operator access.

## PEG-006: Capacity Baseline and Headroom Evidence

Why it matters: Operators need a measured baseline for CPU, RAM, disk, queue,
ClickHouse, Grafana, and ActivityWatch behavior before scaling beyond the
current small deployment.

Current state: Missing / P1. The roadmap explicitly places capacity monitoring
after P0 load gate data. Retention policy documents storage areas and cleanup
impact, but not production headroom evidence.

Repository evidence:

- `DEVELOPMENT_PLAN_NEXT.md:778-805`
- `docs/RETENTION_POLICY_RU.md:65-102`
- `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md:187-209`

How it could be verified: collect production baseline snapshots for disk usage,
service health, ClickHouse table sizes, queue/backlog state, and operational
smoke output over an agreed observation window.

Expected operational benefit: gives operators thresholds for safe growth and
early warning before disk or load incidents.

Risk if left unverified: capacity problems may be discovered only after
operator-visible degradation.

## PEG-007: Backup Verification and Off-Host Backup Evidence

Why it matters: Retention prevents uncontrolled growth, but backup verification
proves recoverability. A local backup without checksum/off-host evidence can
fail during actual disaster recovery.

Current state: Missing / partial. Gitea backup is documented with checksum and
timer metadata. The recovery audit records missing scheduled full AW DB backup,
off-host backup copy, and restore ownership for several runtime stores.

Repository evidence:

- `RECOVERY_AUDIT.md:128-149`
- `RECOVERY_AUDIT.md:306-321`
- `RECOVERY_AUDIT.md:326-338`
- `docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md:25-40`
- `docs/RETENTION_POLICY_RU.md:65-102`

How it could be verified: use existing backup locations/runbooks where present,
verify checksums, confirm backup age/retention, confirm off-host copy status if
owned outside the repository, and record components that remain without a repo
backup/restore path.

Expected operational benefit: separates "data is retained" from "data can be
restored".

Risk if left unverified: backups may be absent, stale, local-only, or
unreadable when needed.

## PEG-008: Install Kit Reproducibility and Stale Payload Evidence

Why it matters: Windows installer payload drift can deploy old collectors even
when repository scripts and CI are current.

Current state: Missing / P1. The backlog lists install kit reproducibility and
stale payload gate as open.

Repository evidence:

- `DEVELOPMENT_PLAN_NEXT.md:441-475`
- `scripts/rebuild_install_kit.sh`
- `scripts/validate_install_kit.sh`
- `scripts/check_install_kit_vs_repo.sh`
- `adk-rust/crates/rebuild-install-kit/`

How it could be verified: rebuild the install kit on the controlled runner,
generate or verify payload manifest with source commit and SHA256 values, then
compare packaged collector payloads with repository state and release artifacts.

Expected operational benefit: prevents deploying stale Windows collector
payloads during RC rollout.

Risk if left unverified: endpoint deployment can silently diverge from the
reviewed release.

## PEG-009: Reviewed PR / Release Governance Evidence

Why it matters: Production 1.0 closure requires visible evidence that protected
branch, CODEOWNERS, and PR review processes are not only documented but used for
release-quality changes.

Current state: Missing / P1. Branch protection evidence exists, but the backlog
still lists reviewed PR and release governance evidence as open.

Repository evidence:

- `DEVELOPMENT_PLAN_NEXT.md:519-552`
- `docs/BRANCH_PROTECTION_EVIDENCE_RU.md`
- `docs/PR_REVIEW_EVIDENCE_RU.md`
- `scripts/registry_readiness_check.sh`

How it could be verified: record a reviewed PR flow with required checks,
CODEOWNERS/reviewer approval, no admin bypass, and final merge evidence for the
RC branch or release preparation branch.

Expected operational benefit: proves that Production 1.0 changes pass the
intended governance workflow.

Risk if left unverified: release governance remains documented but not proven
on the actual RC workflow.

## Release Candidate Evidence Gate

Release Candidate readiness should not be declared until the P0 evidence gaps
are closed or explicitly accepted by the operator with written risk acceptance:

1. PEG-001 production binary parity evidence.
2. PEG-002 separate-host Gitea restore proof.
3. PEG-003 runtime data restore proof or documented operator acceptance for
   stores without repository restore support.
4. PEG-004 controlled release evidence package from `awatch-build-01`.
5. PEG-005 portal/worktime hot-path load evidence.

P1 evidence can remain after RC only if it is explicitly tracked as release
follow-up and does not invalidate the operator's Production 1.0 risk decision.
