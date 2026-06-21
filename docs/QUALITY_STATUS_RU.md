# Quality status

Статус: public engineering transparency document.

## CI status

Public GitHub Actions workflows are available for mirror validation:

- `CI`: Rust checks, registry/docs checks and smoke checks.
- `Coverage`: cargo-llvm-cov baseline summary.
- `Security`: cargo audit, cargo deny, secret-pattern check and dependency
  review for pull requests.

GitHub Actions is public mirror validation only. Public CI is not registry release evidence and is not the primary registry build contour.

## Coverage baseline policy

Coverage threshold is not enforced yet. The first stage is tracking and
regression visibility:

- collect `cargo llvm-cov --workspace --summary-only`;
- store coverage summary artifact;
- avoid failing early public builds by percentage before baseline review;
- add future threshold after the first stable baseline is reviewed.

## Registry release build

Registry release build and release evidence must be produced on the Russian
build-runner described in
`docs/registry/RU_BUILD_RUNNER_READINESS_RU.md`.

## Security checks

Public security checks are advisory/public validation. Registry release
security evidence must be generated in the Russian build contour.

## Conservative positioning

The quality layer does not claim certification, does not position AWatch-rus as
a SIEM/DLP replacement and does not claim legal completion of Russian software
registry registration.
