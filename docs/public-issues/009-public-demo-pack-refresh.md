# Public issue template 009

## Title

[docs] Refresh public demo pack and screenshots

## Labels

`docs`, `demo`, `public`

## Purpose

Refresh public demo materials and screenshots while keeping them free of
sensitive data.

## Background

Public demo evidence improves transparency, but demo assets must not expose
customer infrastructure, employee data or secrets.

## Scope

- Review demo pack and screenshots.
- Replace stale screenshots where needed.
- Confirm demo data is synthetic or anonymized.
- Update public demo references.

## Non-goals

- No use of real employee activity logs.
- No customer infrastructure disclosure.
- No product behavior change.

## Acceptance criteria

- Demo assets are current.
- Sensitive data review is recorded.
- Screenshots use synthetic/anonymized data.
- README/docs links remain valid.

## Evidence required

- Updated demo asset list.
- Screenshot review note.
- Secret/PII scan result.
- Link validation notes where applicable.

## Safety/privacy guardrails

- Do not publish secrets, tokens, internal hostnames, private IPs, employee
  names or customer identifiers.
- Use synthetic data for examples.
- Remove metadata from images when needed.

## Registry-positioning guardrails

- Demo pack is public visibility, not registry release evidence.
- Do not claim customer adoption from demo assets.
- Do not imply certification.

## Checklist

- [ ] Inventory demo assets.
- [ ] Refresh stale screenshots.
- [ ] Check for secrets and PII.
- [ ] Update references.
- [ ] Record review result.
