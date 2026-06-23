# AWatch-rus: PR review evidence

Дата: 2026-06-23

pr_review_evidence_status: "pending_until_first_reviewed_pr_is_merged"

GitHub issue: https://github.com/igor04091968/AWatch-rus/issues/48

Этот документ фиксирует, что будет считаться evidence для PR-based review
workflow. Он не утверждает, что external peer review уже выполнен.

## Evidence Criteria

Первый evidence-backed reviewed PR должен содержать:

- PR URL;
- linked issue URL;
- completed pull request template;
- passed checks;
- reviewer approval;
- merge commit;
- no bypass, or documented bypass with reason and follow-up checks.

## Evidence Record

- PR URL: `pending`
- Linked issue URL: `pending`
- Reviewer: `pending`
- Reviewer type: `pending`
- Approval URL or screenshot filename: `pending`
- Passed checks: `pending`
- Merge commit: `pending`
- Bypass used: `pending`
- Date: `YYYY-MM-DD`
- Maintainer note: `pending`

## Reviewer Interpretation

Review by the same maintainer improves change discipline but does not prove
external peer review. External peer review must not be marked completed unless a
reviewed public PR includes a reviewer who is not the submitting maintainer and
the review is visible.

## Current Status

- PR workflow documentation: ready.
- PR template: ready.
- CODEOWNERS routing: ready.
- First reviewed PR evidence: pending.
- External peer review completed: not claimed.

## Not Registry Release Evidence

PR review evidence is governance/process evidence for public development
visibility. It is not registry release evidence and does not replace artifacts,
checksums, logs or release evidence from the Russian build-runner.

## Russian Contour Note

Primary registry-readiness contour remains Russian Gitea plus the planned
Russian build-runner. GitHub remains public mirror validation only.

## Guardrails

- Do not publish secrets, private URLs, private account data or customer
  identifiers.
- Do not claim completed external peer review until the evidence record is
  filled from a real reviewed PR.
- Do not claim branch protection verification from PR evidence alone.
- Do not claim completed registry submission.
- Do not claim certification.
- Do not claim SIEM/DLP replacement.
- Do not claim ML/LLM-based detection.
- Do not claim automatic remediation.
