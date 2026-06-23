# Public issue template 006

## Title

[security] Prepare external security/code review checklist

## Labels

`security`, `review`, `governance`

## Purpose

Prepare a public checklist for future visible external security/code review.

## Background

Review checklist and CODEOWNERS exist, but active external peer review is not
claimed until public reviewed pull requests or equivalent evidence exist.

## Scope

- Extend review evidence expectations from `docs/REVIEW_CHECKLIST_RU.md`.
- Define security review scope and artifacts.
- Define how reviewed PRs will be referenced.
- Define forbidden data for public review comments.

## Non-goals

- No claim that external review is already active.
- No publication of sensitive findings before triage.
- Forbidden claim: automatic remediation is not claimed.

## Acceptance criteria

- External/security review checklist is documented.
- Evidence format for reviewed PRs is defined.
- Sensitive disclosure handling is documented.
- First review remains pending until public evidence exists.

## Evidence required

- Checklist document.
- Link to review policy.
- Future reviewed PR URL or placeholder status.
- Security disclosure guardrails.

## Safety/privacy guardrails

- Do not publish exploit details before coordinated handling.
- Do not publish customer data, employee data or secrets.
- Keep vulnerability handling aligned with `SECURITY.md`.

## Registry-positioning guardrails

- Do not claim active external peer review until public reviewed PRs exist.
- Security review evidence is governance evidence, not certification.
- Do not claim FSTEC/FSB certification.

## Checklist

- [ ] Draft external review checklist.
- [ ] Define evidence requirements.
- [ ] Define sensitive disclosure rules.
- [ ] Link to `docs/REVIEW_CHECKLIST_RU.md`.
- [ ] Record first reviewed PR only after it exists.
