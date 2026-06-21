# AWatch-rus Roadmap

This roadmap is public planning. It does not claim completion of unverified
work and does not replace `docs/registry/` evidence for registry-readiness.

## Registry-readiness

- Maintain `docs/registry/` as the authoritative registry-readiness
  documentation package.
- Keep conservative product claims and explicit remaining gaps.
- Prepare final rightsholder confirmation and legal review separately.

## Russian Git/build contour

- Keep self-hosted Gitea as the target Russian Git contour for
  registry-readiness.
- Keep GitHub as public mirror validation only.
- Provision `awatch-build-01` as a separate Russian build-runner.

## Release evidence

- Run release candidate checks on the Russian build-runner.
- Generate source archive, binary archive, SBOM, SHA256SUMS, smoke logs and
  release evidence manifest.
- Keep public GitHub Actions separate from registry release evidence.

## Backup/restore test

- Complete a test restore of Gitea backup on a separate server.
- Keep `restore_tested=false` until evidence exists.
- Document offsite backup in RF before registry submission.

## Coverage and CI

- Use public CI for engineering transparency.
- Track coverage baseline without enforcing a threshold at first.
- Add coverage threshold after baseline review.

## Security scanning

- Maintain cargo audit, cargo deny, dependency review and secret-pattern checks.
- Treat public security checks as advisory validation.
- Produce registry release security evidence in the Russian build contour.

## Russian OS compatibility

- Validate deployment and agent behavior on target Russian OS variants.
- Document unsupported combinations explicitly.

## Pilot hardening

- Keep demo data anonymized.
- Improve smoke coverage for install kit and operational reports.
- Preserve clear rollback and backup-first operational procedures.

## Future UI

- Future UI work remains planned unless backed by implemented code and tests.
- Public roadmap entries are not product claims.

## Not claimed / out of scope

- No claim of FSTEC/FSB certification.
- No claim of replacing DLP or SIEM.
- No claim of ML/LLM-based detection.
- No claim of automatic remediation.
- No claim of legal completion of Russian software registry registration.
