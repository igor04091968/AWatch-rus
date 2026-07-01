# AWatch-rus Governance

GitHub is the public mirror validation surface. Primary registry release
evidence must be produced on the Russian build-runner and primary operational
context remains the private Gitea contour.

## Production-first standard

AWatch-rus is already deployed in a real company. Production stability has
absolute priority. The canonical engineering standard is:

- [Enterprise quality standard](../docs/ENTERPRISE_QUALITY_STANDARD_RU.md)
- [Review checklist](../docs/REVIEW_CHECKLIST_RU.md)
- [Operational validation runbook](../docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md)
- [Operational maturity harness](../docs/OPERATIONAL_MATURITY_RU.md)

## Required PR evidence

Every PR must state:

- Purpose.
- Operational impact.
- Risk assessment.
- Rollback strategy.
- Validation steps.
- Documentation changes.
- Acceptance criteria.

Documentation-only or governance-only PRs must explicitly state that runtime,
API and UI behavior are unchanged.

## Guardrails

- Prefer additive, backward-compatible changes.
- Do not redesign working subsystems without measured benefit.
- Do not add dependencies without justification and validation.
- Do not weaken authentication, authorization, audit logging, secret handling,
  dependency hygiene or configuration validation.
- Do not enable heavy DLP, Loki or always-on Velociraptor during routine
  recovery, validation or public CI.
- Keep blocking CI fast; keep heavy/load/nightly checks scheduled or advisory.
