# Registry readiness changelog

## 2026-06-20 / 2026-06-21

Added:

- REG.RU/Gitea Russian Git contour documentation.
- Self-hosted Gitea repository reference:
  `https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus`.
- GitHub role as public mirror only.
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
- Documented access control policy.
- Documented backup offsite copy.
- Final registry legal review.
