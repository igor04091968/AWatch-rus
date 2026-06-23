# Public issue template 008

## Title

[ops] Validate release artifacts storage in RF

## Labels

`ops`, `release`, `registry`

## Purpose

Validate the storage location, retention and integrity process for release
artifacts in the Russian contour.

## Background

Release evidence requires reproducible artifacts and checksums stored in the
approved contour. Storage remains pending until verified.

## Scope

- Identify storage path or service in the Russian contour.
- Document retention and access model.
- Verify checksum procedure.
- Document backup or immutability expectations.

## Non-goals

- No publication of private artifact URLs if access is restricted.
- No release creation.
- No runtime/API/UI change.

## Acceptance criteria

- Storage location is documented in non-sensitive form.
- Retention policy is documented.
- Access model is documented.
- Checksum verification procedure is documented.

## Evidence required

- Storage policy note.
- Checksum verification example.
- Retention setting or procedure.
- Access model review note.

## Safety/privacy guardrails

- Do not publish credentials or private storage tokens.
- Do not expose private URLs that grant access.
- Redact internal storage topology where needed.

## Registry-positioning guardrails

- Storage validation is a prerequisite for release evidence, not proof of
  registry submission.
- Do not claim release package completion until artifacts exist.
- Primary evidence remains in the Russian contour.

## Checklist

- [ ] Identify storage contour.
- [ ] Document retention.
- [ ] Document access model.
- [ ] Verify checksum procedure.
- [ ] Record blockers.
