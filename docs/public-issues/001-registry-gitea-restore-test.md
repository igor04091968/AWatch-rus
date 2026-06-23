# Public issue template 001

## Title

[registry] Perform Gitea backup restore test

## Labels

`registry`, `ops`, `evidence`

## Purpose

Prove that the documented Gitea backup can be restored on a separate host and
that restore evidence is reproducible.

## Background

The Russian Gitea contour and backup process are documented, but restore proof
is not complete. The registry evidence manifest must keep restore status pending
until a separate-host restore drill is recorded.

## Scope

- Run a restore drill on a separate test host or isolated environment.
- Verify backup checksum before restore.
- Verify repository availability after restore.
- Record commands, logs, timestamps and rollback notes in non-sensitive form.

## Non-goals

- No production restore.
- No change to runtime services, API, UI or business logic.
- No claim that registry submission is complete.

## Acceptance criteria

- Restore log is attached or linked.
- SHA256 verification is recorded.
- Post-restore repository checks are recorded.
- Rollback or cleanup notes are recorded.
- Registry evidence manifest is updated only after evidence exists.

## Evidence required

- Backup artifact name without secrets.
- Checksum verification output.
- Restore command log with sensitive values redacted.
- Post-restore repository clone or integrity check.
- Reviewer note confirming evidence location.

## Safety/privacy guardrails

- Do not publish passwords, tokens, private keys or recovery codes.
- Do not publish customer identifiers, employee data or private infrastructure
  details.
- Redact internal paths when they expose sensitive topology.

## Registry-positioning guardrails

- Keep `restore_tested=false` until evidence is recorded.
- Do not describe the restore contour as registry-ready until the drill is
  complete and reviewed.
- GitHub issue visibility is public roadmap visibility, not registry release
  evidence.

## Checklist

- [ ] Select isolated restore target.
- [ ] Verify backup checksum.
- [ ] Perform restore.
- [ ] Run post-restore repository checks.
- [ ] Record evidence location.
- [ ] Update manifest only after evidence exists.
