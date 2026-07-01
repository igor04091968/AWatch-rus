## Summary

Describe what changed and why.

## Purpose

State the production-quality purpose of the change. Prefer reliability,
operational maturity, security, maintainability, reproducibility, performance
or simplicity over new functionality.

## Impact

- Runtime impact: `none / changed / not applicable`
- API impact: `none / changed / not applicable`
- UI impact: `none / changed / not applicable`
- Documentation impact: `none / changed / not applicable`
- Rollback impact: `none / documented / not applicable`
- Evidence impact: `none / registry docs updated / release evidence required`

## Operational Impact

Describe deployment, upgrade, rollback, observability, diagnostics,
configuration, recovery, performance or dependency-hygiene impact.

## Risk Assessment

List production risks and why the change is backward-compatible. For
documentation-only or governance-only changes, state that runtime/API/UI behavior
is unchanged.

## Rollback Strategy

State how to revert the change. Runtime, automation, config and dependency
changes need an explicit rollback path.

## Validation

List commands executed. Use `skipped: <reason>` when a check requires a live
stand or unavailable tool.

## Documentation Changes

List README/runbook/architecture/governance updates, or state `not applicable`
with a reason.

## Acceptance Criteria

List concrete conditions that make the PR safe to merge.

## Review Checklist

- [ ] Linked issue is provided, or the PR explains why no issue is applicable.
- [ ] Purpose, operational impact, risk assessment, rollback strategy,
      validation steps, documentation changes and acceptance criteria are stated.
- [ ] Change is additive/backward-compatible, or breaking impact is explicitly
      blocked for this stage.
- [ ] Production stability is preserved for existing deployments.
- [ ] No working subsystem is redesigned without measured benefit.
- [ ] Runtime/API/UI impact is stated.
- [ ] Registry claims are checked and remain conservative.
- [ ] Secrets, PII, employee logs and customer identifiers are absent.
- [ ] Tests/checks executed are listed, or skipped checks have reasons.
- [ ] Evidence docs are updated when the change affects governance, registry
      readiness or release evidence.
- [ ] GitHub Actions are public validation only, not registry release evidence.
- [ ] I checked that this PR does not publish secrets, tokens, passwords,
      private keys, recovery codes or live credentials.
- [ ] I checked that this PR does not publish personal data, real employee data,
      customer logs or customer infrastructure identifiers.
- [ ] I checked registry claims: no completed registry submission, no
      FSTEC/FSB certification claim, no SIEM/DLP replacement claim.
- [ ] I ran relevant checks or documented why a check was skipped.
- [ ] I checked dependency impact: no unnecessary dependency was added, and no
      unused dependency remains in touched crates.
- [ ] I stated runtime/API/UI impact.
- [ ] I stated documentation impact.
- [ ] I stated smoke-test result or why smoke testing is not applicable.
- [ ] I stated rollback and evidence impact.
- [ ] I checked that GitHub Actions remains public mirror validation only.
- [ ] I checked that registry release evidence still requires the Russian
      build-runner.

## Registry / Public Mirror Scope

- GitHub is public mirror validation only.
- Primary registry release evidence must be produced on the Russian
  build-runner.
- Update `docs/registry/` when registry-readiness behavior or evidence changes.

## Governance

- Production-first standard: `.github/GOVERNANCE.md`.
- Canonical review checklist: `docs/REVIEW_CHECKLIST_RU.md`.
- Canonical validation runbook: `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`.

## Safety

- No secrets, tokens, passwords or private keys.
- No personal data.
- No real employee logs.
- No customer evidence unless anonymized.
- No unsupported claims about certification, DLP/SIEM replacement or legal
  registry completion.
