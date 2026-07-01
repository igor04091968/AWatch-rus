# Contributing

GitHub is public mirror validation only. Primary registry release evidence is
produced separately on the Russian build-runner and documented under
`docs/registry/`.

All contributions must follow the production-first governance entrypoint:
`.github/GOVERNANCE.md`. AWatch-rus is already deployed in a real company, so
reliability, operational maturity, security and backward compatibility take
priority over new functionality.

## Branching

- Работайте в feature-ветке, не пушьте напрямую в `main`.
- Именование: `feat/...`, `fix/...`, `docs/...`, `chore/...`.

## Commit style

- Предпочтительно Conventional Commits:
  - `feat(...)`
  - `fix(...)`
  - `docs(...)`
  - `chore(...)`

## Required checks before PR

- `cargo fmt --all --check` from `adk-rust/`.
- `cargo test --workspace` from `adk-rust/`, unless the PR is documentation-only
  and the skip is documented.
- `cargo clippy --workspace --all-targets -- -D warnings` from `adk-rust/`.
- `bash -n` для всех changed `*.sh`.
- `bash scripts/registry_readiness_check.sh` when registry docs/process changes.
- `node scripts/deployment-readiness-smoke.mjs` when Node.js is available.
- `node scripts/pilot-validation-smoke.mjs` when Node.js is available.
- `Invoke-ScriptAnalyzer` для `windows/*.ps1`, `windows/*.psm1`, `windows/*.psd1`.
- Проверка, что нет секретов (`secrets/deploy.secrets.env` не должен быть в
  индексе git).
- Обновлены инструкции и runbook при изменении поведения.

## Registry-readiness docs

- Registry-readiness documents live in `docs/registry/`.
- Public GitHub CI is not registry release evidence.
- Registry release evidence must be generated on the Russian build-runner.
- GitHub remains public mirror validation only.

## Secrets and personal data

- Do not commit secrets, tokens, passwords, cookies or private keys.
- Do not commit personal data.
- Do not commit real employee logs.
- Use demo/anonymized evidence for issues, PRs, docs and screenshots.

## PR content

- Изменения и обоснование.
- Purpose.
- Operational impact.
- Risk assessment.
- Rollback strategy.
- Какие команды валидации были выполнены.
- Documentation changes.
- Acceptance criteria.
- Какие проверки были пропущены и почему, если пропуск был необходим.
