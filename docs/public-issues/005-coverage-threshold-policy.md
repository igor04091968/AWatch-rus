# Public issue template 005

## Title

[qa] Define coverage threshold policy

## Labels

`qa`, `coverage`, `policy`

## Purpose

Define a conservative coverage threshold policy after the baseline is stable and
reviewed.

## Background

Coverage workflow exists for visibility, but threshold enforcement is not
enabled yet. Premature thresholds can create noisy failures before the baseline
is understood.

## Scope

- Review current coverage baseline.
- Identify crates or modules where thresholds are meaningful.
- Propose a staged threshold policy.
- Document exceptions and review cadence.

## Non-goals

- No immediate hard threshold without baseline review.
- No claim that coverage proves absence of defects.
- No runtime, API or UI changes.

## Acceptance criteria

- Baseline coverage summary is reviewed.
- Initial threshold proposal is documented.
- Exceptions are documented.
- Enforcement plan is staged and reversible.

## Evidence required

- Coverage workflow artifact reference.
- Baseline review notes.
- Proposed threshold values.
- Rationale for exclusions or delayed enforcement.

## Safety/privacy guardrails

- Do not publish private test data or production logs.
- Keep coverage artifacts free of secrets and customer identifiers.
- Avoid copying sensitive paths into public issue text.

## Registry-positioning guardrails

- Coverage visibility is quality evidence, not registry release evidence.
- Threshold policy must not imply certification.
- GitHub remains public mirror validation only.

## Checklist

- [ ] Review coverage baseline.
- [ ] Identify meaningful threshold scope.
- [ ] Document proposed values.
- [ ] Document exclusions.
- [ ] Decide when enforcement can start.
