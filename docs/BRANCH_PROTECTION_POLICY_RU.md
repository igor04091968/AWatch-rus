# AWatch-rus: advisory branch protection policy

Дата: 2026-06-23

Статус: recommended policy. Этот документ описывает целевую настройку GitHub
branch protection для публичного зеркала. Он не утверждает, что branch
protection уже включен.

GitHub остается public mirror validation surface. Primary registry-readiness
contour остается Russian Gitea plus Russian build-runner release evidence.

## Scope

- Branch: `main`.
- Platform: GitHub public mirror.
- Purpose: visible review discipline, status-check discipline and public
  engineering maturity signal.
- Registry release evidence: out of scope for GitHub Actions.

## Recommended rules

- Require pull request before merge.
- Require at least one approving review for non-emergency changes.
- Require status checks before merge.
- Require `CI` workflow.
- Require `Security` workflow.
- Require `Coverage` workflow as baseline visibility; no coverage threshold is
  enforced yet.
- Require conversation resolution before merge.
- Restrict force push.
- Restrict branch deletion.
- Require linear history if compatible with the maintainer workflow.
- Administrator bypass should be emergency-only and documented after the fact.

## Recommended GitHub Branch Protection Settings

Recommended settings for `main` on the GitHub public mirror:

- Require pull request before merging.
- Required approvals: `1`.
- Dismiss stale approvals when new commits are pushed.
- Require review from CODEOWNERS if available on the current GitHub plan.
- Require status checks to pass before merging.
- Require branches to be up to date before merging if this does not block the
  current maintainer workflow.
- Restrict force pushes.
- Restrict deletions.
- Allow administrators bypass: documented decision only; stricter mode should
  keep bypass disabled unless repository recovery requires it.

Recommended required checks, using current workflow/job names:

- `CI / Rust checks`
- `CI / Docs and registry checks`
- `CI / Smoke checks`
- `Coverage / Coverage baseline`
- `Security / Cargo audit`
- `Security / Cargo deny`
- `Security / Secret pattern check`
- `Security / Dependency review`

Before verification, maintainer must compare these names with the exact check
names displayed by GitHub. If GitHub displays different names, update this
document and `docs/BRANCH_PROTECTION_EVIDENCE_RU.md` before recording evidence.

Current evidence status is tracked in
`docs/BRANCH_PROTECTION_EVIDENCE_RU.md` and remains
`pending_manual_verification` until maintainer evidence is recorded.

## Review expectations

- CODEOWNERS routes changes to the current maintainer.
- External visible peer review is still pending and should be introduced through
  public pull requests.
- Review approval is not a warranty of security, fitness for production or
  legal readiness.
- Contributors remain responsible for the safety and accuracy of their changes.

## Registry and security guardrails

- Do not claim FSTEC/FSB certification.
- Do not claim completed Russian software registry submission.
- Do not claim SIEM/DLP replacement.
- Do not publish secrets, personal data, employee data or customer
  infrastructure identifiers.
- Do not claim Gitea restore test completed until evidence exists.
- Do not claim Russian build-runner ready until provisioning evidence exists.

## Emergency bypass

Emergency administrator bypass may be used only for urgent repository recovery,
blocked release hygiene or security containment. The follow-up record should
state:

- reason for bypass;
- commits affected;
- checks run after bypass;
- rollback or follow-up action;
- whether registry-readiness claims changed.
