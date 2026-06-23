# Public issue template 007

## Title

[compat] Test Russian OS compatibility matrix

## Labels

`compat`, `qa`, `registry`

## Purpose

Build an evidence-backed compatibility matrix for target Russian operating
systems.

## Background

Compatibility must be tested and documented. Unsupported compatibility claims
must not be made before evidence exists.

## Scope

- Define target OS versions.
- Run installation and smoke checks where applicable.
- Record pass/fail/blocked status.
- Document gaps and next actions.

## Non-goals

- No claim of support for untested OS versions.
- No certification claims.
- No runtime change in this issue.

## Acceptance criteria

- Compatibility matrix exists.
- Each target OS has status and evidence reference.
- Failed or blocked cases include next action.
- Public wording avoids unsupported claims.

## Evidence required

- OS/version list.
- Test command summary.
- Smoke check results.
- Known gaps and blockers.

## Safety/privacy guardrails

- Do not publish customer infrastructure identifiers.
- Do not publish private hostnames, credentials or internal IPs.
- Use sanitized environment descriptions.

## Registry-positioning guardrails

- Compatibility matrix is evidence support, not registry completion.
- Do not claim FSTEC/FSB certification.
- Do not claim support until test evidence exists.

## Checklist

- [ ] Define OS list.
- [ ] Run installation checks.
- [ ] Run smoke checks.
- [ ] Record evidence.
- [ ] Update compatibility matrix.
