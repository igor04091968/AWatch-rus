# AWatch-rus: branch protection evidence

Дата: 2026-06-24

branch_protection_status: "verified_active_ruleset"

GitHub issue: https://github.com/igor04091968/AWatch-rus/issues/49

Этот документ фиксирует maintainer-verified GitHub ruleset / branch protection
evidence для публичного зеркала. Подтверждение относится только к GitHub public
mirror governance.

## Target

- Repository: `igor04091968/AWatch-rus`.
- Platform role: GitHub public mirror validation only.
- Protected branch: `main`.
- Ruleset name: `main`.
- Enforcement: `active`.
- Policy source: `docs/BRANCH_PROTECTION_POLICY_RU.md`.
- Evidence owner: maintainer.

## Verified Settings

Maintainer manually verified the following GitHub UI state:

- verification date: `2026-06-24`;
- maintainer: `maintainer`;
- repository: `github.com/igor04091968/AWatch-rus`;
- ruleset name: `main`;
- enforcement: `active`;
- target branch: `main`;
- applies_to_targets: `1`;
- bypass_list: `empty`;
- required pull request: `enabled`;
- required approvals: `1`;
- dismiss stale pull request approvals when new commits are pushed: `enabled`;
- require review from Code Owners: `enabled`;
- block force pushes: `enabled`.

## Verified Required Status Checks

GitHub ruleset required status checks verified in the UI:

- `Coverage baseline`
- `security`
- `rust-checks`
- `docs-registry-checks`
- `smoke-checks`

These names are the ruleset-visible required check names from GitHub UI. They
may differ from the human-readable workflow job names shown inside workflow
files.

## Evidence Record

- Screenshot filename placeholder:
  `docs/evidence/github-ruleset-main-2026-06-24.png`
- Date: `2026-06-24`
- Maintainer: `maintainer`
- Repository: `github.com/igor04091968/AWatch-rus`
- Ruleset: `main`
- Protected branch: `main`
- Enforcement: `active`
- Applies to targets: `1`
- Bypass list: `empty`
- Required checks verified: `Coverage baseline`, `security`, `rust-checks`,
  `docs-registry-checks`, `smoke-checks`
- Force-push restriction verified: `enabled`
- Notes: GitHub UI confirms active ruleset for target branch `main`.

## First Protected PR Validation

- PR URL: `https://github.com/igor04091968/AWatch-rus/pull/50`
- Linked governance issue: `https://github.com/igor04091968/AWatch-rus/issues/49`
- PR source branch: `docs/verified-github-ruleset-evidence`
- PR target branch: `main`
- Runtime/API/UI/product code changes: `none`
- Required checks status: `passed`
- Required checks: `Coverage baseline`, `security`, `rust-checks`,
  `docs-registry-checks`, `smoke-checks`
- Review requirement status: `pending_review_required`
- Merge status: `open`
- Admin bypass used: `false`
- Outcome note: PR #50 confirms required checks execute under the active ruleset;
  review/merge evidence is not yet complete.

## Future Reverification Procedure

1. Open repository settings for `igor04091968/AWatch-rus`.
2. Open repository rules/rulesets for `main`.
3. Confirm enforcement remains `active`.
4. Confirm target branch remains `main`.
5. Confirm bypass list remains empty.
6. Confirm required PR, approvals, stale dismissal, Code Owners review, status
   checks and force-push block.
7. Capture screenshot evidence without private account data or tokens.

## Not Registry Release Evidence

GitHub ruleset / branch protection evidence is public governance evidence only.
It is not Russian registry release evidence and does not replace Russian
Gitea/build-runner release contour, release artifacts, checksums, build logs or
release evidence from the Russian build-runner.

## Russian Contour Note

Primary registry-readiness contour remains Russian Gitea plus the planned
Russian build-runner. GitHub remains public mirror validation only.

## Guardrails

- Do not record secrets, tokens, private URLs or account recovery details.
- Do not include private employee/customer data in screenshots.
- Do not claim completed registry submission.
- Do not claim certification.
- Do not claim SIEM/DLP replacement.
- Do not claim ML/LLM-based detection.
- Do not claim automatic remediation.
