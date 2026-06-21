# Registry readiness changelog

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
