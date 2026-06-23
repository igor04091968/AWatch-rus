# AWatch-rus: branch protection evidence

Дата: 2026-06-23

branch_protection_status: "pending_manual_verification"

GitHub issue: https://github.com/igor04091968/AWatch-rus/issues/49

Этот документ является evidence template для ручной проверки GitHub branch
protection на публичном зеркале. Он не утверждает, что branch protection уже
настроена или подтверждена.

## Target

- Repository: `igor04091968/AWatch-rus`.
- Platform role: GitHub public mirror validation only.
- Protected branch: `main`.
- Policy source: `docs/BRANCH_PROTECTION_POLICY_RU.md`.
- Evidence owner: maintainer.

## Settings To Verify

Maintainer должен вручную проверить, что для `main` configured rule включает:

- require pull request before merging;
- require approvals: `1`;
- dismiss stale approvals when new commits are pushed;
- require review from CODEOWNERS, if available on the current GitHub plan;
- require status checks to pass before merging;
- require branches to be up to date before merging, if compatible with current
  maintainer workflow;
- restrict force pushes;
- restrict deletions;
- administrator bypass decision documented, preferably disabled for stricter
  mode.

## Required Checks

Expected required checks are based on current workflow/job names:

- `CI / Rust checks`
- `CI / Docs and registry checks`
- `CI / Smoke checks`
- `Coverage / Coverage baseline`
- `Security / Cargo audit`
- `Security / Cargo deny`
- `Security / Secret pattern check`
- `Security / Dependency review`

If GitHub displays a different context name, record the exact displayed name and
update `docs/BRANCH_PROTECTION_POLICY_RU.md` before marking verification done.

## Manual Verification Procedure

1. Open repository settings for `igor04091968/AWatch-rus`.
2. Open branch protection or repository rules for branch `main`.
3. Compare enabled settings against this document and
   `docs/BRANCH_PROTECTION_POLICY_RU.md`.
4. Verify required status-check names exactly as GitHub displays them.
5. Capture screenshot evidence without private account data or tokens.
6. Record evidence fields below.
7. Only after verification, update `branch_protection_status` from
   `"pending_manual_verification"` to `"verified"` in a follow-up change.

## Evidence Record

- Screenshot filename placeholder:
  `docs/evidence/github-branch-protection-main-YYYY-MM-DD.png`
- Date: `YYYY-MM-DD`
- Maintainer: `maintainer`
- Repository: `igor04091968/AWatch-rus`
- Protected branch: `main`
- Required checks verified: `pending`
- Admin bypass decision: `pending`
- Force-push restriction verified: `pending`
- Deletion restriction verified: `pending`
- Notes: `pending`

## Not Registry Release Evidence

GitHub branch protection evidence is governance/process evidence for the public
mirror. It is not registry release evidence and does not replace release
artifacts, checksums, build logs or release evidence from the Russian
build-runner.

## Russian Contour Note

Primary registry-readiness contour remains Russian Gitea plus the planned
Russian build-runner. GitHub remains public mirror validation only.

## Guardrails

- Do not record secrets, tokens, private URLs or account recovery details.
- Do not include private employee/customer data in screenshots.
- Do not claim branch protection is verified until maintainer evidence exists.
- Do not claim completed registry submission.
- Do not claim certification.
- Do not claim SIEM/DLP replacement.
- Do not claim ML/LLM-based detection.
- Do not claim automatic remediation.
