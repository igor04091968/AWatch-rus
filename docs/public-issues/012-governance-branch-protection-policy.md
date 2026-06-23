# Public issue template 012

## Title

[governance] Add branch protection policy

## Labels

`governance`, `github`, `policy`

## Purpose

Verify and, after maintainer review, configure GitHub branch protection aligned
with the advisory policy.

## Background

Branch protection policy is documented as advisory. It must not be claimed as
enabled until repository settings are verified and evidence is recorded.

## Scope

- Review advisory branch protection policy.
- Verify current repository settings.
- Configure settings if approved.
- Record screenshots or textual evidence after verification.

## Non-goals

- No claim that branch protection is enabled before verification.
- No destructive repository setting changes without maintainer review.
- No runtime/API/UI change.

## Acceptance criteria

- Current branch protection state is documented.
- Approved settings are recorded.
- Evidence is attached or linked after verification.
- If blocked, blockers are recorded.

## Evidence required

- Repository settings notes or screenshots.
- Required status checks list.
- Maintainer approval note.
- Blocker list if settings cannot be changed.

## Safety/privacy guardrails

- Do not publish admin tokens or private repository settings that expose
  sensitive access details.
- Redact account-level private information in screenshots.
- Keep emergency access policy documented.

## Registry-positioning guardrails

- Do not claim branch protection is enabled until settings are verified.
- Branch protection is governance control, not registry release evidence.
- GitHub remains public mirror validation only.

## Checklist

- [ ] Review advisory policy.
- [ ] Verify current settings.
- [ ] Configure approved settings if authorized.
- [ ] Record evidence.
- [ ] Update status docs only after verification.
