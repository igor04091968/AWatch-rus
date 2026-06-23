# Public issue template 003

## Title

[release] Produce first release evidence package

## Labels

`release`, `registry`, `evidence`

## Purpose

Produce the first release evidence package from the Russian build-runner once
the runner is available.

## Background

Release evidence scripts exist, but the first real release evidence build must
run in the Russian build contour before it can be treated as registry evidence.

## Scope

- Run release evidence scripts on the Russian build-runner.
- Collect logs, checksums, artifact manifest and command versions.
- Store evidence in the documented Russian storage contour.
- Link evidence from registry documentation after review.

## Non-goals

- No claim that release evidence is already produced.
- No publication of secret build logs.
- No runtime, API or UI changes.

## Acceptance criteria

- Release evidence manifest exists.
- Build logs are retained with secrets redacted.
- Checksums are recorded.
- Artifact storage path is documented.
- Review note confirms evidence completeness.

## Evidence required

- Release manifest.
- Build logs.
- SHA256 checksums.
- Cargo metadata/tree or equivalent dependency evidence.
- Artifact retention note.

## Safety/privacy guardrails

- Do not publish credentials, private paths with sensitive data or customer
  environment identifiers.
- Redact tokens and private repository access details.
- Keep evidence links scoped to approved public-safe material.

## Registry-positioning guardrails

- Do not treat GitHub Actions as release evidence.
- Do not claim registry submission is complete.
- Keep evidence pending until artifacts and checksums exist.

## Checklist

- [ ] Confirm build-runner readiness.
- [ ] Run release evidence script.
- [ ] Verify generated checksums.
- [ ] Store artifacts in Russian contour.
- [ ] Review logs for sensitive data.
- [ ] Record evidence links.
