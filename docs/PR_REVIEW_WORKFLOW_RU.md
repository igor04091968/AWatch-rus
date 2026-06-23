# AWatch-rus: PR-based review workflow

Дата: 2026-06-23

Статус: workflow documentation ready; first reviewed PR evidence remains
pending.

GitHub issue: https://github.com/igor04091968/AWatch-rus/issues/48

Этот документ описывает целевой PR-based workflow для публичного GitHub mirror.
Он не утверждает, что external peer review уже выполнен.

## Scope

- Repository: `igor04091968/AWatch-rus`.
- Branch: `main`.
- GitHub role: public mirror validation only.
- Primary registry contour: Russian Gitea plus planned Russian build-runner.

## Workflow

1. Significant changes should be made on a branch and submitted through a pull
   request.
2. Each PR should link the relevant GitHub issue or state why no issue is
   applicable.
3. The PR template must be completed before merge.
4. CODEOWNERS should route review to the responsible maintainer or reviewer.
5. CI, Coverage and Security checks should pass before merge.
6. Any bypass must be documented in the PR or follow-up evidence note.

## Docs-Only Changes

Docs-only governance changes may use a reduced local check set when no product
code changes:

- `python3 scripts/public_secret_pattern_check.py`
- `bash scripts/prepare_public_issues.sh`
- `bash -n scripts/registry_readiness_check.sh`
- `bash scripts/registry_readiness_check.sh`
- `git diff --check`

If shell scripts change, run `bash -n` for each changed shell script.

## Runtime/Product Changes

Runtime, API, UI or product-code changes require a broader validation plan.
Expected checks include:

- `cargo fmt --all --check`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo build --workspace`
- relevant smoke tests for deployment, pilot validation or browser behavior;
- rollback notes when operational behavior changes.

## Security-Sensitive Changes

Security-sensitive changes require:

- public secret-pattern scan;
- review against `SECURITY.md`;
- no secrets, tokens, private keys, recovery codes or customer identifiers;
- no exploit detail in public text before security triage.

## Registry Documentation Changes

Registry docs must preserve conservative claims:

- GitHub Actions is public mirror validation only.
- Registry release evidence requires Russian Gitea and the planned Russian
  build-runner.
- Do not claim completed registry submission.
- Do not claim FSTEC/FSB certification.
- Do not claim SIEM/DLP replacement.
- Do not claim ML/LLM-based detection.
- Do not claim automatic remediation.
- Do not claim branch protection verification until maintainer evidence exists.
- Do not claim external peer review completion until reviewed PR evidence
  exists.

## Required Check Names

Current workflow/job names used for branch protection planning:

- `CI / Rust checks`
- `CI / Docs and registry checks`
- `CI / Smoke checks`
- `Coverage / Coverage baseline`
- `Security / Cargo audit`
- `Security / Cargo deny`
- `Security / Secret pattern check`
- `Security / Dependency review`

These names should be rechecked against GitHub UI before branch protection is
marked verified.

## Evidence

Evidence for the first reviewed PR is tracked in
`docs/PR_REVIEW_EVIDENCE_RU.md`.

## Not Registry Release Evidence

PR review workflow evidence improves public process visibility. It is not
registry release evidence and does not replace release evidence generated on the
Russian build-runner.
