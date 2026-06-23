# Public issue template 011

## Title

[governance] Enable PR-based review workflow

## Labels

`governance`, `review`, `process`

## Purpose

Move visible changes through pull requests where practical and record review
evidence.

## Background

PR template, CODEOWNERS and review checklist exist. Active visible external
review is still pending until reviewed public PRs exist.

## Scope

- Define PR-based workflow for public changes.
- Run a documented dry-run PR or first reviewed PR.
- Record required status checks.
- Record review evidence expectations.

## Non-goals

- No claim that external review is already active.
- No bypass of emergency maintainer control for security incidents.
- No runtime behavior change.

## Acceptance criteria

- PR workflow is documented.
- First reviewed PR or dry-run PR is recorded.
- Required evidence and checks are listed.
- Open blockers are documented.

## Evidence required

- Reviewed PR URL or dry-run PR URL after creation.
- Checklist completion note.
- CI/security/coverage status notes.
- Review comment or approval evidence when available.

## Safety/privacy guardrails

- Do not publish secrets or private customer context in PRs or issues.
- Do not expose security-sensitive details before triage.
- Keep emergency fixes possible under documented policy.

## Registry-positioning guardrails

- PR review workflow is governance evidence, not registry release evidence.
- Do not claim external peer review is active until public reviewed PRs exist.
- GitHub remains public mirror validation only.

## Checklist

- [ ] Define PR workflow.
- [ ] Create dry-run or first reviewed PR.
- [ ] Record checks.
- [ ] Record review evidence.
- [ ] Update status docs after evidence exists.
