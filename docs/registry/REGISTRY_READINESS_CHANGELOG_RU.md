# Registry readiness changelog

## 2026-06-23 branch protection and PR review evidence package

Added:

- `docs/BRANCH_PROTECTION_EVIDENCE_RU.md` with
  `pending_manual_verification` status for GitHub branch protection evidence.
- `docs/PR_REVIEW_WORKFLOW_RU.md` with PR-based review workflow rules.
- `docs/PR_REVIEW_EVIDENCE_RU.md` with evidence criteria for the first reviewed
  public PR.
- Public issues manifest links issue #48 to PR review evidence and issue #49 to
  branch protection evidence.

Changed:

- `docs/BRANCH_PROTECTION_POLICY_RU.md` now lists recommended settings and real
  current GitHub Actions check names.
- `.github/pull_request_template.md` includes compact governance/evidence
  checklist items.
- `.github/CODEOWNERS` has clearer zones for workflows/security/governance,
  registry docs, scripts, Rust workspace, demo/screenshots/docs.
- Project status and residual risks now distinguish prepared governance
  evidence from pending verification.

Runtime impact:

- No runtime/product code changes.
- No API changes.
- No UI changes.
- No business logic changes.

Guardrails:

- Branch protection verification remains pending until maintainer records
  repository settings evidence.
- External peer review is not claimed completed until real reviewed PR evidence
  exists.
- GitHub remains public mirror validation only.
- Russian Gitea plus planned Russian build-runner remains the primary registry
  contour.

## 2026-06-23 public roadmap issues created and linked

Changed:

- Created 12 public roadmap/governance GitHub issues from
  `docs/public-issues/`.
- Recorded issue URLs, `created_at` timestamps and `created_by=maintainer` in
  `docs/public-issues/public-issues-manifest.json`.
- Updated project status, public issue plan and residual risk register to
  distinguish created public issues from actual task completion evidence.
- Registry readiness checks now validate created issue URL/status consistency.

Runtime impact:

- No runtime/product code changes.
- No API changes.
- No UI changes.
- No business logic changes.

Guardrails:

- Public roadmap issues are development visibility evidence only.
- GitHub remains public mirror validation only.
- Russian Gitea plus planned Russian build-runner remains the primary registry
  contour.
- Created issues do not prove restore completion, build-runner readiness,
  release evidence production, external peer review, branch protection
  enablement or community adoption.

## 2026-06-23 public issue creation package

Added:

- `docs/public-issues/` with public issue templates for the planned governance,
  registry, QA, security, compatibility, ops, demo and pilot tasks.
- `docs/public-issues/public-issues-manifest.json` with `ready_to_create`
  status and `github_issue_url: null` until real issue URLs are recorded.
- `docs/PUBLIC_ISSUES_CREATION_RUNBOOK_RU.md` for manual and opt-in `gh` CLI
  issue creation.
- `scripts/prepare_public_issues.sh` as a dry-run validation and command
  preparation script.
- `scripts/create_public_issues_from_manifest.sh` as an opt-in helper that
  requires `CONFIRM_CREATE_GITHUB_ISSUES=YES`.

Changed:

- `docs/PUBLIC_ISSUES_PLAN_RU.md`, project status, residual risks and README now
  distinguish prepared issue templates from real created GitHub issues.
- Registry readiness checks now verify the public issue package and pending URL
  status.

Runtime impact:

- No runtime/product code changes.
- No API changes.
- No UI changes.
- No business logic changes.

Guardrails:

- GitHub remains public mirror validation only.
- Russian Gitea plus planned Russian build-runner remains the primary registry
  contour.
- Real GitHub issue creation remains manual/opt-in.
- GitHub issue URLs remain pending until created and recorded in the manifest.

## 2026-06-22 review governance and branch protection policy

Added:

- `.github/CODEOWNERS` for public review routing and engineering ownership.
- `docs/REVIEW_CHECKLIST_RU.md` for PR/code review checks.
- `docs/BRANCH_PROTECTION_POLICY_RU.md` as advisory GitHub branch protection
  policy.
- Expanded `docs/PUBLIC_ISSUES_PLAN_RU.md` with governance issues for PR-based
  review workflow and branch protection.
- Registry readiness checks for review/governance documents and false-claim
  guardrails.

Changed:

- PR template now includes compact security, registry-claim, runtime/API/UI,
  smoke-test, rollback and evidence checklist items.
- README and project status now link to review/governance documents.
- Residual risk register now records that visible external code review remains
  pending.

Runtime impact:

- No runtime/product code changes.
- No API changes.
- No UI changes.
- No business logic changes.

Guardrails:

- Branch protection is documented as advisory and is not claimed as enabled.
- External visible peer review is not claimed as active.
- Restore test remains pending.
- Russian build-runner remains planned.
- Registry submission, FSTEC/FSB certification and SIEM/DLP replacement are not
  claimed.

## 2026-06-22 residual risk register and public issue plan

Added:

- `docs/RESIDUAL_RISKS_RU.md` with the remaining governance, public process,
  disaster recovery, build-runner, release evidence and legal package risks.
- `docs/PUBLIC_ISSUES_PLAN_RU.md` with public GitHub issues to create manually.
- Registry readiness checks for residual risk documents and pending-state
  guardrails.

Changed:

- README and project status now link to the residual risk register and public
  issue plan.

Runtime impact:

- No runtime/product code changes.
- No API changes.
- No UI changes.
- No business logic changes.

Guardrails:

- Restore test is not claimed as completed.
- Russian build-runner is not claimed as ready.
- First release evidence build is not claimed as completed.
- Legal rightsholder package remains pending.

## 2026-06-22 public GitHub Actions validation passed

Changed:

- Recorded first public validation passed after
  `4f90aba chore(security): harden public secret scan and document policy`.
- Documented passed public GitHub Actions contours: `CI`, `Coverage` and
  `Security`.
- Documented that the hardened public secret scan passed.
- Reconfirmed that GitHub Actions remains public mirror validation only and is
  not registry release evidence.
- Reconfirmed that registry release evidence still requires the Russian
  build-runner contour.

Runtime impact:

- No runtime/product code changes.
- No API changes.
- No UI changes.
- No business logic changes.

## 2026-06-22 Gitea duplicate status

Changed:

- Documented that `https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus` is the
  created self-hosted Gitea duplicate/mirror of the GitHub repository.
- Documented Gitea operator account name `igor` without storing password,
  tokens, SSH private keys or recovery codes in tracked files.
- Clarified that the current local working copy on this machine still has
  `origin` pointing to GitHub and should use `ru-origin` for direct Gitea push.
- Updated registry-readiness wording from a purely target scheme to the current
  deployed source repository contour.

Runtime impact:

- No runtime/product code changes.
- No API changes.
- No UI changes.
- Rust/runtime checks not required: documentation-only update.

## 2026-06-21 public secret scan hardening

Added:

- `scripts/public_secret_pattern_check.py` as a reproducible local equivalent
  of the public GitHub Actions secret-pattern check.
- `docs/SECURITY_SCANNING_POLICY_RU.md` describing fail-closed public secret
  scanning, dummy values and inline allow comments.
- README link to the public secret scanning policy.
- Registry readiness check integration for the local public secret scanner.

Changed:

- Security workflow now calls `python3 scripts/public_secret_pattern_check.py`
  instead of inline Python.
- Secret scan output remains redacted and reports only `file:line:rule`.
- Cargo deny workflow command now runs from the Rust workspace and checks
  advisories, licenses and sources with the repository `deny.toml`.
- `CDLA-Permissive-2.0` is explicitly allowed for `webpki-roots`; final
  registry submission still requires legal review.

Runtime impact:

- No runtime/product code changes.
- No API changes.
- No UI changes.
- No deployment behavior changes.

Reason:

- First public security workflow exposed false positives on runtime-derived
  values and safe config lookups. The scanner was hardened without disabling
  the check and without broad directory allowlists.

## 2026-06-21 status freeze

Added:

- `docs/PROJECT_STATUS_RU.md` as a single status freeze for the current
  registry-readiness baseline.
- README link to the status freeze document.
- Registry readiness check coverage for the status freeze document.

Baseline:

- Commit:
  `4970d31 chore(public): add CI coverage security and OSS process visibility`.
- Primary Russian Git:
  `https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus`.
- GitHub role:
  public mirror / public validation only.

Runtime impact:

- No runtime/product code changes.
- No API changes.
- No UI changes.
- Rust/runtime checks not required: documentation-only status freeze.

Remaining gaps:

- Gitea restore test.
- Actual `awatch-build-01` provisioning.
- First real release evidence build.
- Release artifacts storage in RF.
- Legal rightsholder confirmation.
- Final legal review.
- Russian OS compatibility testing.

## 2026-06-21 public engineering transparency

Added:

- Public CI workflow for GitHub mirror validation.
- Public coverage baseline workflow.
- Public security workflow with cargo audit, cargo deny, secret-pattern check
  and dependency review for pull requests.
- `SECURITY.md`, `CONTRIBUTING.md`, public `ROADMAP.md`, issue templates and
  pull request template.
- `docs/QUALITY_STATUS_RU.md`.

Changed:

- Registry manifest now records public engineering transparency fields.
- Registry readiness check now validates public CI/security/coverage/process
  files.

Runtime impact:

- No runtime/product code changes.
- No API changes.
- No UI changes.
- GitHub Actions is public mirror validation only.
- Russian build-runner remains required for registry release candidate.

Checks note:

- Rust/runtime checks should run in public CI and on `awatch-build-01`.
- Local Rust checks may be skipped for this documentation/process-only update
  only if the skip reason is recorded in the final report.

Remaining gaps:

- First successful public CI run after push.
- First coverage baseline artifact after push.
- First security scan baseline after push.
- Actual `awatch-build-01` provisioning and registry release evidence run.

## 2026-06-21

Added:

- Russian build-runner readiness docs.
- Build-runner setup runbook.
- Release evidence runbook.
- Release artifacts storage policy.
- `scripts/build_release_evidence.sh`.
- `scripts/check_release_evidence.sh`.

Changed:

- Updated registry evidence manifest with build-runner plan.
- Updated registry readiness checks for build-runner and release evidence
  requirements.

Runtime impact:

- No runtime/product code changes.
- No API changes.
- No UI changes.
- Rust/runtime checks not required: registry documentation and release evidence
  script update only.

Remaining gaps:

- Actual `awatch-build-01` server provisioning.
- Build-runner first successful release candidate build.
- SBOM tool installation decision.
- Release artifacts storage in RF.
- Restore test for Gitea backup.
- Legal rightsholder confirmation.
- Final legal review.

## 2026-06-20 / 2026-06-21

Added:

- REG.RU/Gitea Russian Git contour documentation.
- Documented repository migration to
  `https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus`.
- Self-hosted Gitea repository reference:
  `https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus`.
- GitHub role as public mirror only.
- Gitea Wiki policy as navigation-only, with `docs/registry/` as the
  authoritative registry-readiness documentation package.
- Gitea backup/restore runbook.
- Registry evidence manifest updates for Gitea, backup ZIP, SHA256 checksum,
  systemd timer and restore status.
- Registry readiness check script for the new `docs/registry/` package.

Changed:

- README now contains a short Registry-readiness infrastructure block.

Runtime impact:

- No runtime/product code changes.
- No API changes.
- No UI changes.
- Rust/runtime checks not required: documentation-only change.

Remaining gaps:

- Legal rightsholder confirmation.
- Russian build-runner.
- Release artifacts storage in RF.
- Tested restore procedure.
- Offsite backup in RF.
- Documented access control policy.
- Documented backup offsite copy.
- Final registry legal review.
