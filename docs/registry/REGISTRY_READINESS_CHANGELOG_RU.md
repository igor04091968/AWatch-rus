# Registry readiness changelog

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
