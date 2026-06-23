# Public issue template 002

## Title

[registry] Prepare temporary Russian build-runner awatch-build-01

## Labels

`registry`, `build-runner`, `ops`

## Purpose

Prepare the Russian build-runner contour required for registry release
evidence.

## Background

GitHub Actions is public mirror validation only. Registry release evidence must
be produced in the Russian contour connected to the Russian Gitea source.

## Scope

- Define provisioning notes for `awatch-build-01`.
- Document toolchain, OS baseline, access model and Gitea clone method.
- Document required checks for release evidence builds.
- Keep runner status pending until provisioning evidence exists.

## Non-goals

- No production deployment.
- No automatic release.
- No claim that the build-runner is already ready.

## Acceptance criteria

- Build-runner setup notes exist.
- Toolchain list is documented.
- Gitea access method is documented without secrets.
- Required checks list is documented.
- Known blockers are recorded.

## Evidence required

- Host provisioning notes without sensitive addresses.
- Toolchain versions.
- Gitea access verification with credentials redacted.
- Planned release evidence command list.

## Safety/privacy guardrails

- Do not publish credentials, VPN data, SSH keys or private network topology.
- Do not include live internal IPs or host access details in the public issue.
- Use sanitized host labels where possible.

## Registry-positioning guardrails

- Do not mark `awatch-build-01` as ready before evidence exists.
- Do not use GitHub Actions output as registry release evidence.
- Primary registry contour remains Russian Gitea plus Russian build-runner.

## Checklist

- [ ] Confirm target OS and hosting contour.
- [ ] Install required toolchain.
- [ ] Verify Russian Gitea clone path.
- [ ] Document required checks.
- [ ] Record blockers.
- [ ] Update registry evidence docs only after verification.
